param(
    [Parameter(Mandatory)][string]$Baseline,
    [Parameter(Mandatory)][string]$Preview
)
$ErrorActionPreference = 'Stop'
$baselineExe = (Resolve-Path $Baseline).Path
$previewExe = (Resolve-Path $Preview).Path
foreach ($entry in @(@{Name='C++'; Path=$baselineExe}, @{Name='Rust Edition'; Path=$previewExe})) {
    $times = @()
    $peaks = @()
    for ($i = 0; $i -lt 7; $i++) {
        $folder = New-Item -ItemType Directory -Path (Join-Path $env:RUNNER_TEMP ([guid]::NewGuid().ToString()))
        $clock = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process $entry.Path -ArgumentList '--smoke-test' -WorkingDirectory $folder.FullName -PassThru
        $peak = 0L
        try {
            while (!$process.WaitForExit(10)) {
                try {
                    $process.Refresh()
                    $peak = [Math]::Max($peak, $process.PeakWorkingSet64)
                } catch [InvalidOperationException] {
                    if (!$process.HasExited) { throw }
                }
                if ($clock.Elapsed.TotalSeconds -gt 30) {
                    $process.Kill()
                    throw 'Smoke test timed out'
                }
            }
            $clock.Stop()
            if ($process.ExitCode -ne 0) { throw 'Smoke run failed' }
            if ($i -gt 0) {
                $times += $clock.Elapsed.TotalMilliseconds
                if ($peak -gt 0) { $peaks += $peak }
            }
        } finally {
            $process.Dispose()
            Remove-Item -LiteralPath $folder.FullName -Recurse -Force
        }
    }
    $sortedTimes = @($times | Sort-Object)
    $sortedPeaks = @($peaks | Sort-Object)
    $medianTime = [Math]::Round(($sortedTimes[2] + $sortedTimes[3]) / 2, 1)
    $medianPeak = if ($sortedPeaks.Count -eq 6) {
        ($sortedPeaks[2] + $sortedPeaks[3]) / 2
    } else { "unavailable ($($sortedPeaks.Count)/6 samples)" }
    "$($entry.Name): EXE bytes $((Get-Item $entry.Path).Length); median smoke duration $medianTime ms; median sampled peak working set $medianPeak bytes"
}
