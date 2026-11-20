param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Bridge,
    [Parameter(Mandatory)][string]$DependencyShares
)
$ErrorActionPreference = 'Stop'
$packageRoot = Join-Path $env:RUNNER_TEMP ('torrent-re-package-' + [guid]::NewGuid())
$stage = New-Item -ItemType Directory (Join-Path $packageRoot 'app') -Force
$licenses = New-Item -ItemType Directory (Join-Path $stage.FullName 'licenses')
Copy-Item $Executable (Join-Path $stage.FullName 'Torrent-RE.exe')
Copy-Item $Bridge (Join-Path $stage.FullName 'torrent_re_libtorrent.dll')
Copy-Item "$PSScriptRoot/../../assets/fonts/OFL.txt" (Join-Path $licenses.FullName 'Montserrat-OFL.txt')
Get-ChildItem $DependencyShares -Filter copyright -Recurse | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $licenses.FullName ($_.Directory.Name + '.txt'))
}
$installer = Join-Path $packageRoot 'Torrent-1.4.0-RE.2-Setup-x64.exe'
$nsis = Join-Path ${env:ProgramFiles(x86)} 'NSIS/makensis.exe'
if (!(Test-Path $nsis)) {
    choco install nsis --no-progress -y
    if ($LASTEXITCODE) { throw 'NSIS installation failed' }
}
Push-Location $PSScriptRoot
try {
    & $nsis /WX /INPUTCHARSET UTF8 "/DSTAGE=$($stage.FullName)" "/DOUTPUT=$installer" Torrent-RE.nsi
    if ($LASTEXITCODE) { throw 'Rust Edition installer build failed' }
} finally {
    Pop-Location
}
$installed = Join-Path $packageRoot 'installed'
function Run-Checked([string]$Path, [string]$Arguments) {
    $info = [Diagnostics.ProcessStartInfo]::new($Path, $Arguments)
    $info.UseShellExecute = $false
    $process = [Diagnostics.Process]::Start($info)
    try {
        if (!$process.WaitForExit(30000)) { $process.Kill(); throw "Timed out: $Path" }
        if ($process.ExitCode) { throw "Exit $($process.ExitCode): $Path" }
    } finally { $process.Dispose() }
}
Run-Checked $installer "/S /D=$installed"
$registered = Get-ItemPropertyValue 'HKCU:\Software\RegisteredApplications' 'Torrent Rust Edition'
if ($registered -ne 'Software\TorrentRustEdition\Capabilities') { throw 'File registration was not installed' }
Run-Checked (Join-Path $installed 'Torrent-RE.exe') '--smoke-test'
if (!(Test-Path (Join-Path $installed 'torrent_re_libtorrent.dll'))) { throw 'Missing packaged libtorrent adapter' }
$copy = Join-Path $packageRoot 'Cleanup.exe'
Copy-Item (Join-Path $installed 'Uninstall.exe') $copy
Run-Checked $copy "/S _?=$installed"
if ((Test-Path (Join-Path $installed 'Torrent-RE.exe')) -or
    (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\TorrentRustEdition' -ErrorAction SilentlyContinue)) {
    throw 'Uninstaller left the installed app or registry entry'
}
Write-Output 'Package verified: install, application startup, file registration, and uninstall.'
