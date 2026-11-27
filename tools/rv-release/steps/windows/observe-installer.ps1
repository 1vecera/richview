# Diagnostic on the disposable walk guest. Parse the installer without executing it.
$ErrorActionPreference = 'Stop'
$url = 'http://127.0.0.1:8765/install.ps1'
$file = 'C:\rv\e2e\release\install.ps1'
function Hash([byte[]]$bytes) {
    $hash = [Security.Cryptography.SHA256]::Create()
    try { [BitConverter]::ToString($hash.ComputeHash($bytes)).Replace('-', '').ToLowerInvariant() } finally { $hash.Dispose() }
}
function Parsed([string]$text) {
    $tokens = $null
    $problems = $null
    $null = [Management.Automation.Language.Parser]::ParseInput($text, [ref]$tokens, [ref]$problems)
    [pscustomobject]@{chars=$text.Length; sha256=(Hash ([Text.Encoding]::UTF8.GetBytes($text))); errors=@($problems | ForEach-Object { $_.ErrorId }) }
}
$deadline = [DateTime]::UtcNow.AddSeconds(45)
do {
    try { $http = Invoke-WebRequest -UseBasicParsing -Uri $url -TimeoutSec 3; break }
    catch { if ([DateTime]::UtcNow -gt $deadline) { throw }; Start-Sleep -Milliseconds 250 }
} while ($true)
$body = [IO.MemoryStream]::new()
try {
    $http.RawContentStream.Position = 0
    $http.RawContentStream.CopyTo($body)
    $bytes = $body.ToArray()
} finally { $body.Dispose() }
$disk = [IO.File]::ReadAllBytes($file)
$result = [ordered]@{psVersion=$PSVersionTable.PSVersion.ToString(); diskBytes=$disk.Length; disk=(Parsed ([Text.Encoding]::UTF8.GetString($disk))); status=$http.StatusCode; contentType=$http.Headers['Content-Type']; declaredLength=$http.Headers['Content-Length']; httpBytes=$bytes.Length; httpSha256=(Hash $bytes); irm=@()}
irm $url -TimeoutSec 5 | ForEach-Object { $result.irm += [pscustomobject]@{type=$_.GetType().FullName; text=(Parsed ([string]$_))} }
$result | ConvertTo-Json -Depth 6 -Compress

# Nested-child probe (lead, 24 Nov). The failed step ran a child PowerShell whose -Command holds a pipe,
# from a -File script with a transcript on. Its error positions ("& {" at line 1, iex at char 143 of the
# full powershell.exe command line) say the pipe ran in the parent and iex got the installer line by line.
# Each sample repeats that call shape with a harmless consumer and records which process ran the consumer
# and how many objects reached it. Expected: one String of 12076 chars, in a child process.
$ErrorActionPreference = 'Continue'
try {
    "probe: observer pid $PID"
    Get-Command powershell -All | ForEach-Object { "probe: powershell resolves to $($_.CommandType) $($_.Source)" }
    Start-Transcript -Path 'C:\rv\observe-transcript.log' -Force | Out-Null
    $sink = 'C:\rv\observe-probe.txt'
    for ($i = 0; $i -lt 30; $i++) {
        if ($i % 5 -eq 4) { cmd /c "for /L %n in (1,1,60) do @echo filler %n" }
        Remove-Item -LiteralPath $sink -Force -ErrorAction SilentlyContinue
        powershell -NoProfile -ExecutionPolicy Bypass -Command 'irm http://127.0.0.1:8765/install.ps1 | % { Add-Content C:\rv\observe-probe.txt ([string]$PID + '' '' + $_.GetType().Name + '' '' + ([string]$_).Length) }'
        $exit = $LASTEXITCODE
        $rows = @(Get-Content -LiteralPath $sink -ErrorAction SilentlyContinue)
        $first = if ($rows.Count) { $rows[0] } else { '' }
        $inParent = @($rows | Where-Object { $_ -like "$PID *" }).Count
        "probe $i $(Get-Date -Format o) exit=$exit records=$($rows.Count) inParent=$inParent first=[$first]"
        Start-Sleep -Seconds 5
    }
    Stop-Transcript | Out-Null
} catch { "probe: stopped: $_" }
