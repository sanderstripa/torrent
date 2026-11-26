param([Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference='Stop'
$pins=Get-Content (Join-Path $PSScriptRoot 'components-pin.json') -Raw | ConvertFrom-Json
New-Item -ItemType Directory -Force $Destination | Out-Null
$stamps=@{}
foreach($id in @('xray','sudoku')){
 $pin=$pins.$id
 if($pin.url -notmatch ('^https://github.com/'+[regex]::Escape($pin.repository)+'/releases/download/') -or $pin.sha256 -notmatch '^[a-f0-9]{64}$'){throw 'Invalid VPN component pin'}
 $temp=Join-Path ([IO.Path]::GetTempPath()) ('torrent-core-'+[guid]::NewGuid().ToString('N'))
 New-Item -ItemType Directory -Force $temp | Out-Null
 try{
  $archive=Join-Path $temp 'core.zip';Invoke-WebRequest $pin.url -OutFile $archive
  if((Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne $pin.sha256){throw "$id archive digest mismatch"}
  $unpacked=Join-Path $temp 'expanded';Expand-Archive $archive $unpacked
  $exe=Get-ChildItem $unpacked -Filter "$id.exe" -File -Recurse
  if(@($exe).Count -ne 1){throw "$id executable missing or duplicated"}
  $target=Join-Path $Destination $id;New-Item -ItemType Directory -Force $target | Out-Null
  Copy-Item $exe.FullName (Join-Path $target "$id.exe") -Force
  @(
    "Unmodified $id core $($pin.version), distributed as an external process."
    "Upstream and corresponding source: https://github.com/$($pin.repository)/tree/$($pin.version)"
    "Source archive: https://github.com/$($pin.repository)/archive/refs/tags/$($pin.version).tar.gz"
    "Binary archive: $($pin.url)"
    "Binary archive SHA-256: $($pin.sha256)"
  ) | Set-Content (Join-Path $target 'SOURCE.txt') -Encoding utf8NoBOM
  foreach($data in @('geoip.dat','geosite.dat')){Get-ChildItem $unpacked -Filter $data -File -Recurse | Select-Object -First 1 | ForEach-Object {Copy-Item $_.FullName (Join-Path $target $data) -Force}}
  $stamps[$id]=@{version=$pin.version;sha256=(Get-FileHash (Join-Path $target "$id.exe") -Algorithm SHA256).Hash.ToLowerInvariant()}
 }finally{Remove-Item $temp -Recurse -Force -ErrorAction SilentlyContinue}
}
$stamps|ConvertTo-Json -Depth 5|Set-Content (Join-Path $Destination 'components.json') -Encoding utf8NoBOM
$manifest = Join-Path $PSScriptRoot '../Cargo.toml'
cargo build --locked --manifest-path $manifest --target x86_64-pc-windows-msvc --release --features vpn --bin TorrentVpnHost
if ($LASTEXITCODE -ne 0) { throw 'Rust VPN helper build failed' }
$metadata = cargo metadata --locked --manifest-path $manifest --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cargo target directory lookup failed' }
$binary = Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc/release/TorrentVpnHost.exe'
Copy-Item -LiteralPath $binary -Destination $Destination -Force
