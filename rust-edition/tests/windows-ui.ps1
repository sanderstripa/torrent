param([Parameter(Mandatory)][string]$Executable)

$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path $Executable).Path
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class RustTorrentUI {
    public delegate bool Visitor(IntPtr window, IntPtr arg);
    [DllImport("user32.dll")] public static extern bool EnumWindows(Visitor visitor, IntPtr arg);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    public static IntPtr Find(uint pid, string title) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, _) => {
            uint process;
            GetWindowThreadProcessId(window, out process);
            if (process != pid || !IsWindowVisible(window)) return true;
            var caption = new StringBuilder(256);
            GetWindowText(window, caption, caption.Capacity);
            if (caption.ToString() != title) return true;
            found = window;
            return false;
        }, IntPtr.Zero);
        return found;
    }
}
'@

function Wait-Visible([string]$Title) {
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        $process.Refresh()
        if ($process.HasExited) { throw "Process exited early ($($process.ExitCode))" }
        $window = [RustTorrentUI]::Find($process.Id, $Title)
        if ($window -ne [IntPtr]::Zero) { return $window }
        Start-Sleep -Milliseconds 100
    }
    throw "Window did not appear: $Title"
}

function Command([IntPtr]$Window, [int]$Id) {
    if (![RustTorrentUI]::PostMessage($Window, 0x111, [IntPtr]$Id, [IntPtr]::Zero)) {
        throw "Could not post action $Id"
    }
}

$folder = New-Item -ItemType Directory (Join-Path $env:RUNNER_TEMP ([guid]::NewGuid().ToString()))
$profile = New-Item -ItemType Directory (Join-Path $folder.FullName 're-ui-test-data')
$download = New-Item -ItemType Directory (Join-Path $folder.FullName 'Downloads')
$prefs = Join-Path $profile.FullName 'settings.txt'
[IO.File]::WriteAllText($prefs, "$($download.FullName)`n1 0 0 0 0", [Text.UTF8Encoding]::new($false))
$fixture = Join-Path $folder.FullName 'fixture.torrent'
$metadata = [Text.Encoding]::ASCII.GetBytes('d4:infod6:lengthi4e4:name7:fixture12:piece lengthi16384e6:pieces20:')
$digest = [Security.Cryptography.SHA1]::HashData([Text.Encoding]::ASCII.GetBytes('aaaa'))
[IO.File]::WriteAllBytes($fixture, [byte[]]($metadata + $digest + [byte[]](101, 101)))
$process = Start-Process $exe -WorkingDirectory $folder.FullName -ArgumentList @('--interaction-test', "`"$fixture`"") -PassThru
try {
    $main = Wait-Visible 'Torrent Rust Edition'
    Write-Host 'Main window appeared'
    $files = Wait-Visible 'Файлы торрента'
    Write-Host 'File-selection dialog appeared'
    Command $files 111
    $list = [RustTorrentUI]::GetDlgItem($main, 200)
    if ($list -eq [IntPtr]::Zero -or [RustTorrentUI]::SendMessage($list, 0x18B, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32() -ne 1) {
        throw 'Torrent was not added to the UI queue'
    }
    [RustTorrentUI]::SendMessage($list, 0x186, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Command $main 105
    Command $main 110
    $settings = Wait-Visible 'Настройки'
    Write-Host 'Settings sheet appeared'
    $trace = Join-Path $folder.FullName 're-ui-trace.txt'
    $animationComplete = $false
    for ($attempt = 0; $attempt -lt 50; $attempt++) {
        if ((Get-Content -LiteralPath $trace -ErrorAction SilentlyContinue) -contains 'settings animation completed') {
            $animationComplete = $true
            break
        }
        Start-Sleep -Milliseconds 100
    }
    if (!$animationComplete) { throw 'Settings animation did not complete' }
    Command $settings 126
    for ($attempt = 0; $attempt -lt 50 -and (Get-Content $prefs)[1] -ne '1 1 0 0 0'; $attempt++) {
        Start-Sleep -Milliseconds 100
    }
    if ((Get-Content $prefs)[1] -ne '1 1 0 0 0') {
        throw 'Changing the theme did not save the preference while settings were open'
    }
    Command $settings 129
    for ($attempt = 0; $attempt -lt 50 -and (Get-Content $prefs)[1] -ne '1 1 0 1 0'; $attempt++) {
        Start-Sleep -Milliseconds 100
    }
    if ((Get-Content $prefs)[1] -ne '1 1 0 1 0') {
        throw 'Changing notification area behavior did not save the preference'
    }
    Command $settings 111
    Start-Sleep -Milliseconds 400
    [RustTorrentUI]::PostMessage($main, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    for ($attempt = 0; $attempt -lt 50 -and [RustTorrentUI]::IsWindowVisible($main); $attempt++) {
        Start-Sleep -Milliseconds 100
    }
    $process.Refresh()
    if ($process.HasExited -or [RustTorrentUI]::IsWindowVisible($main)) {
        throw 'Closing with notification area enabled did not hide the running client'
    }
    [RustTorrentUI]::PostMessage($main, 0x800B, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    if ((Wait-Visible 'Torrent Rust Edition') -ne $main) {
        throw 'Reopening from the notification area did not restore the same window'
    }
    [RustTorrentUI]::PostMessage($main, 0x800C, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    if (!$process.WaitForExit(30000)) { throw 'Torrent did not close cleanly' }
    if ($process.ExitCode) { throw "Torrent exited with $($process.ExitCode)" }
    $sidecars = @(Get-ChildItem (Join-Path $profile.FullName 'session') -Filter '*.state')
    if ($sidecars.Count -ne 1 -or (Get-Content $sidecars[0].FullName -Raw) -ne '1 0') {
        throw 'Stopping and accepting file selection did not reach the stored queue'
    }
    if (!(Test-Path (Join-Path $profile.FullName 'window-position.txt'))) {
        throw 'Window position was not saved'
    }
} finally {
    foreach ($diagnostic in @('re-ui-panic.txt', 're-ui-trace.txt')) {
        $file = Join-Path $folder.FullName $diagnostic
        if (Test-Path $file) { Get-Content $file -Tail 55 }
    }
    if ($process.HasExited -and $process.ExitCode) {
        Get-WinEvent -FilterHashtable @{LogName='Application'; Id=1000; StartTime=(Get-Date).AddMinutes(-3)} -ErrorAction SilentlyContinue |
            Where-Object Message -match 'torrent-re-windows' | Select-Object -First 1 -ExpandProperty Message
    }
    if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
    Remove-Item -LiteralPath $folder.FullName -Recurse -Force
}
