param([Parameter(Mandatory)][string]$Executable)
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
