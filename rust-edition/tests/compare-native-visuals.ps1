param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output
)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $Executable).Path
New-Item -ItemType Directory -Path $Output -Force | Out-Null
$out = (Resolve-Path -LiteralPath $Output).Path
$work = New-Item -ItemType Directory (Join-Path $env:RUNNER_TEMP ('torrent-visual-' + [guid]::NewGuid()))

Add-Type -AssemblyName System.Drawing.Common
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class TorrentVisual {
    public delegate bool Visitor(IntPtr window, IntPtr arg);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool EnumWindows(Visitor visitor, IntPtr arg);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int length);
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
    public static long[] Compare(int[] expected, int[] actual, int width) {
        long changed = 0, sum = 0;
        int left = width, top = expected.Length / width, right = -1, bottom = -1;
        for (int index = 0; index < expected.Length; ++index) {
            int a = expected[index] & 0xFFFFFF, b = actual[index] & 0xFFFFFF;
            if (a == b) continue;
            ++changed;
            int x = index % width, y = index / width;
            left = Math.Min(left, x); right = Math.Max(right, x);
            top = Math.Min(top, y); bottom = Math.Max(bottom, y);
            for (int shift = 0; shift < 24; shift += 8) {
                sum += Math.Abs(((a >> shift) & 255) - ((b >> shift) & 255));
            }
        }
        return new long[] { changed, sum, left, top, right, bottom };
    }
}
'@

function Wait-Window([Diagnostics.Process]$Process, [string]$Title) {
    for ($attempt = 0; $attempt -lt 150; $attempt++) {
        $Process.Refresh()
        if ($Process.HasExited) { throw "Torrent exited before capture ($($Process.ExitCode))" }
        $window = [TorrentVisual]::Find($Process.Id, $Title)
        if ($window -ne [IntPtr]::Zero) { return $window }
        Start-Sleep -Milliseconds 100
    }
    throw "Window did not appear: $Title"
}
function Save-Capture([IntPtr]$Window, [string]$Path) {
    $bounds = [TorrentVisual+Rect]::new()
    if (![TorrentVisual]::GetClientRect($Window, [ref]$bounds)) { throw 'Could not read capture bounds' }
    $bitmap = [Drawing.Bitmap]::new($bounds.Right, $bounds.Bottom)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $dc = $graphics.GetHdc()
            try {
                if (![TorrentVisual]::PrintWindow($Window, $dc, 3)) { throw 'PrintWindow failed' }
            } finally { $graphics.ReleaseHdc($dc) }
        } finally { $graphics.Dispose() }
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
}
function Read-Pixels([Drawing.Bitmap]$Bitmap, [Drawing.Rectangle]$Region) {
    $data = $Bitmap.LockBits($Region, [Drawing.Imaging.ImageLockMode]::ReadOnly, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $pixels = [int[]]::new($Region.Width * $Region.Height)
        for ($row = 0; $row -lt $Region.Height; $row++) {
            [Runtime.InteropServices.Marshal]::Copy(
                [IntPtr]::Add($data.Scan0, $row * $data.Stride), $pixels, $row * $Region.Width, $Region.Width)
        }
        return ,$pixels
    } finally { $Bitmap.UnlockBits($data) }
}

$archive = Join-Path $work.FullName 'baseline.zip'
Invoke-WebRequest -Uri 'https://github.com/sanderstripa/torrent/releases/download/v1.4.4/Torrent-Windows-x64.zip' -OutFile $archive
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne '85a388da12c154cff7e687010636a7bf08a068a03cfb944f2278994f9ba18ee4') {
    throw 'Reference 1.4.4 archive SHA-256 differs from the published asset'
}
$expanded = Join-Path $work.FullName 'baseline-bin'
Expand-Archive -LiteralPath $archive -DestinationPath $expanded
$baseline = @(Get-ChildItem -LiteralPath $expanded -Recurse -Filter 'Torrent.exe')
if ($baseline.Count -ne 1) { throw 'The reference archive does not contain a unique Torrent.exe' }

$results = foreach ($theme in @('light', 'dark')) {
    $flag = if ($theme -eq 'dark') { '--visual-test-dark' } else { '--visual-test' }
    $old = New-Item -ItemType Directory (Join-Path $work.FullName "$theme-reference")
    $reference = Start-Process -FilePath $baseline[0].FullName -WorkingDirectory $old.FullName -ArgumentList $flag -PassThru
    try {
        if (!$reference.WaitForExit(30000)) { $reference.Kill(); throw 'Reference capture timed out' }
        if ($reference.ExitCode) { throw "Reference visual mode exited with $($reference.ExitCode)" }
    } finally { $reference.Dispose() }
    Copy-Item -LiteralPath (Join-Path $old.FullName 'main.png') -Destination (Join-Path $out "$theme-reference-main.png")
    Copy-Item -LiteralPath (Join-Path $old.FullName 'settings.png') -Destination (Join-Path $out "$theme-reference-settings.png")
    Copy-Item -LiteralPath (Join-Path $old.FullName 'files.png') -Destination (Join-Path $out "$theme-reference-files.png")

    $new = New-Item -ItemType Directory (Join-Path $work.FullName "$theme-rust")
    $process = Start-Process -FilePath $exe -WorkingDirectory $new.FullName -ArgumentList $flag -PassThru
    try {
        $main = Wait-Window $process 'Torrent Rust Edition'
        Start-Sleep -Milliseconds 250
        Save-Capture $main (Join-Path $out "$theme-rust-main.png")
        [TorrentVisual]::PostMessage($main, 0x111, [IntPtr]110, [IntPtr]::Zero) | Out-Null
        $settings = Wait-Window $process 'Настройки'
        Start-Sleep -Milliseconds 300
        Save-Capture $settings (Join-Path $out "$theme-rust-settings.png")
        [TorrentVisual]::PostMessage($settings, 0x111, [IntPtr]111, [IntPtr]::Zero) | Out-Null
        for ($attempt = 0; $attempt -lt 100 -and [TorrentVisual]::IsWindowVisible($settings); $attempt++) {
            Start-Sleep -Milliseconds 50
        }
        [TorrentVisual]::PostMessage($main, 0x800d, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        $files = Wait-Window $process 'Файлы торрента'
        Start-Sleep -Milliseconds 250
        Save-Capture $files (Join-Path $out "$theme-rust-files.png")
        [TorrentVisual]::PostMessage($files, 0x111, [IntPtr]112, [IntPtr]::Zero) | Out-Null
        [TorrentVisual]::PostMessage($main, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        if (!$process.WaitForExit(10000) -or $process.ExitCode) { throw 'Rust visual fixture did not close cleanly' }
    } finally {
        if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $process.Dispose()
    }

    foreach ($view in @('main', 'settings', 'files')) {
        $referenceImage = [Drawing.Bitmap]::new((Join-Path $out "$theme-reference-$view.png"))
        $actualImage = [Drawing.Bitmap]::new((Join-Path $out "$theme-rust-$view.png"))
        try {
            $width = [Math]::Min($referenceImage.Width, $actualImage.Width)
            $height = [Math]::Min($referenceImage.Height, $actualImage.Height)
            $left = if ($view -eq 'settings') { [Math]::Floor(($referenceImage.Width - $width) / 2.0) } else { 0 }
            $oldPixels = Read-Pixels $referenceImage ([Drawing.Rectangle]::new($left, 0, $width, $height))
            $newPixels = Read-Pixels $actualImage ([Drawing.Rectangle]::new(0, 0, $width, $height))
            $difference = [TorrentVisual]::Compare($oldPixels, $newPixels, $width)
            [pscustomobject]@{
                Theme = $theme
                View = $view
                Reference = "$($referenceImage.Width)x$($referenceImage.Height)"
                Rust = "$($actualImage.Width)x$($actualImage.Height)"
                Compared = $width * $height
                Changed = $difference[0]
                Bounds = if ($difference[0] -gt 0) { "$($difference[2]),$($difference[3])–$($difference[4]),$($difference[5])" } else { '' }
                MeanChannelError = [Math]::Round($difference[1] / (3.0 * $width * $height), 3)
            }
        } finally {
            $referenceImage.Dispose()
            $actualImage.Dispose()
        }
    }
}
$results | ConvertTo-Json | Set-Content (Join-Path $out 'pixel-comparison.json') -Encoding utf8
$results | Format-Table -AutoSize
