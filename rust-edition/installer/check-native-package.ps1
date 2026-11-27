param([Parameter(Mandatory)][string]$Executable, [string]$ArtifactDirectory)
$ErrorActionPreference = 'Stop'
$crate = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$root = New-Item -ItemType Directory (Join-Path $env:RUNNER_TEMP ('torrent-native-setup-' + [guid]::NewGuid()))
$stage = New-Item -ItemType Directory (Join-Path $root.FullName 'payload')
$licenses = New-Item -ItemType Directory (Join-Path $stage.FullName 'licenses')
Copy-Item -LiteralPath $Executable -Destination (Join-Path $stage.FullName 'Torrent-RE.exe')
Copy-Item -LiteralPath (Join-Path $crate '../assets/fonts/OFL.txt') -Destination (Join-Path $licenses.FullName 'Montserrat-OFL.txt')
Copy-Item -LiteralPath (Join-Path $crate 'librqbit/vendor/LICENSE') -Destination (Join-Path $licenses.FullName 'librqbit-NOTICE.txt')
Copy-Item -LiteralPath (Join-Path $crate 'librqbit/vendor/LICENSE-APACHE') -Destination (Join-Path $licenses.FullName 'librqbit-Apache-2.0.txt')
& (Join-Path $crate 'vpn/prepare.ps1') -Destination (Join-Path $stage.FullName 'vpn')

$env:TORRENT_INSTALL_PAYLOAD = $stage.FullName
try {
    cargo build --locked --target x86_64-pc-windows-msvc --release --features installer --manifest-path "$crate/Cargo.toml" --bin torrent-re-setup
    if ($LASTEXITCODE -ne 0) { throw 'Native installer build failed' }
} finally {
    Remove-Item Env:TORRENT_INSTALL_PAYLOAD
}

function Run-Checked([string]$Path, [string[]]$Arguments, [int]$ExpectedCode = 0) {
    $info = [Diagnostics.ProcessStartInfo]::new($Path)
    $info.UseShellExecute = $false
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::Start($info)
    try {
        if (!$process.WaitForExit(60000)) { $process.Kill(); throw "Timed out: $Path" }
        if ($process.ExitCode -ne $ExpectedCode) { throw "Exit $($process.ExitCode) from $Path (expected $ExpectedCode)" }
    } finally { $process.Dispose() }
}

$setup = Join-Path $crate 'target/x86_64-pc-windows-msvc/release/torrent-re-setup.exe'
$installed = Join-Path $root.FullName 'installed edition'
Run-Checked $setup @('--install', $installed)
$application = Join-Path $installed 'Torrent-RE.exe'
Run-Checked $application @('--smoke-test')
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\TorrentRustEdition'
if ((Get-ItemPropertyValue $uninstallKey DisplayVersion) -ne '1.4.4 RE') { throw 'Installer did not register its version' }
if ((Get-ItemPropertyValue 'HKCU:\Software\RegisteredApplications' 'Torrent Rust Edition') -ne 'Software\TorrentRustEdition\Capabilities') {
    throw 'Installer did not register file associations'
}
foreach ($directory in @(
    [Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory),
    [Environment]::GetFolderPath([Environment+SpecialFolder]::Programs)
)) {
    if (!(Test-Path -LiteralPath (Join-Path $directory 'Torrent Rust Edition.lnk'))) { throw 'Installer did not create a shortcut' }
}
$sentinel = Join-Path $installed 'user-file.txt'
Set-Content -LiteralPath $sentinel -Value 'untouched'
Run-Checked $setup @('--install', $installed) 1
if ((Get-Content -LiteralPath $sentinel) -ne 'untouched') { throw 'Update modified an unrelated file' }
Remove-Item -LiteralPath $sentinel
Run-Checked $setup @('--install', $installed)
Run-Checked $application @('--smoke-test')

$uninstaller = Join-Path $installed 'Uninstall.exe'
Run-Checked $uninstaller @('--uninstall', $installed)
for ($attempt = 0; $attempt -lt 200 -and ((Test-Path -LiteralPath $installed) -or (Test-Path $uninstallKey)); $attempt++) {
    Start-Sleep -Milliseconds 100
}
if ((Test-Path -LiteralPath $installed) -or (Test-Path $uninstallKey)) { throw 'Native uninstaller did not clear the application and registration' }
Write-Output 'Native installer verified for installation, update, app startup, shortcuts, file registration and removal.'

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NativeTorrentSetup {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr w, string value);
    public static bool Click(IntPtr window, int x, int y) {
        return PostMessage(window, 0x202, IntPtr.Zero, new IntPtr(y * 0x10000 + x));
    }
}
'@
$guiTarget = Join-Path $root.FullName 'installed through setup window'
$gui = Start-Process -FilePath $setup -PassThru
try {
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        $gui.Refresh()
        if ($gui.HasExited) { throw "Installer window exited early ($($gui.ExitCode))" }
        $window = $gui.MainWindowHandle
        if ($window -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    }
    if ($window -eq [IntPtr]::Zero -or ![NativeTorrentSetup]::Click($window, 105, 365)) {
        throw 'Could not open installer settings'
    }
    $edit = [IntPtr]::Zero
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        $edit = [NativeTorrentSetup]::GetDlgItem($window, 101)
        if ($edit -ne [IntPtr]::Zero -and [NativeTorrentSetup]::IsWindowVisible($edit)) { break }
        Start-Sleep -Milliseconds 100
    }
    if ($edit -eq [IntPtr]::Zero -or ![NativeTorrentSetup]::IsWindowVisible($edit)) {
        throw 'Editable installation path did not appear'
    }
    if ([NativeTorrentSetup]::SendMessage($edit, 0xC, [IntPtr]::Zero, $guiTarget) -eq [IntPtr]::Zero) {
        throw 'Could not change the installation path'
    }
    if (![NativeTorrentSetup]::Click($window, 70, 290) -or ![NativeTorrentSetup]::Click($window, 150, 410)) {
        throw 'Could not start installation from native settings'
    }
    for ($attempt = 0; $attempt -lt 200; $attempt++) {
        if ((Test-Path (Join-Path $guiTarget 'Torrent-RE.exe')) -and (Test-Path $uninstallKey)) { break }
        Start-Sleep -Milliseconds 100
    }
    if (!(Test-Path (Join-Path $guiTarget 'Torrent-RE.exe')) -or !(Test-Path $uninstallKey)) {
        throw 'The setup window did not install to the selected path'
    }
    Start-Sleep -Milliseconds 600
    if (![NativeTorrentSetup]::Click($window, 302, 390) -or ![NativeTorrentSetup]::Click($window, 420, 337)) {
        throw 'Could not finish native setup'
    }
    if (!$gui.WaitForExit(10000) -or $gui.ExitCode) { throw 'Native setup finish page did not close' }
    Run-Checked $setup @('--cleanup', $guiTarget, '0')
} finally {
    if (!$gui.HasExited) { $gui.Kill(); $gui.WaitForExit() }
    $gui.Dispose()
}

if ($ArtifactDirectory) {
    New-Item -ItemType Directory -Path $ArtifactDirectory -Force | Out-Null
    $installerName = 'Torrent-RE-Setup-Windows-x64.exe'
    $portableName = 'Torrent-RE-Portable-Windows-x64.zip'
    Copy-Item -LiteralPath $setup -Destination (Join-Path $ArtifactDirectory $installerName)
    Compress-Archive -Path "$($stage.FullName)/*" -DestinationPath (Join-Path $ArtifactDirectory $portableName)
    $portableCheck = Join-Path $root.FullName 'portable'
    Expand-Archive -LiteralPath (Join-Path $ArtifactDirectory $portableName) -DestinationPath $portableCheck
    Run-Checked (Join-Path $portableCheck 'Torrent-RE.exe') @('--smoke-test')
    $sums = foreach ($name in @($installerName, $portableName)) {
        $digest = (Get-FileHash -LiteralPath (Join-Path $ArtifactDirectory $name) -Algorithm SHA256).Hash.ToLowerInvariant()
        "$digest  $name"
    }
    $sums | Set-Content -LiteralPath (Join-Path $ArtifactDirectory 'SHA256SUMS.txt') -Encoding ascii
}
