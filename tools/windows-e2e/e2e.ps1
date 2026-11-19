# The end-to-end walk of RichView on Windows (ARC-67 as amended): everything a macOS or Linux user can
# do, on one fresh Windows machine, recorded. Usage:
#   e2e.ps1 -Commit <full commit> [-Version 3.0.0-…] [-Editor C:\rv\e2e\editor\richview-editor.exe] [-SkipPrevious]
# Inputs in C:\rv\e2e, sent by the run: release\ (the archive and the install.ps1 stamped for
# http://127.0.0.1:8765), examples\ (hello-world is not needed: init writes it; sql-report with its
# data\warehouse.duckdb; openai-vs-anthropic), editor\richview-editor.exe.
# Writes C:\rv\e2e\out: e2e.log (a transcript ending in E2E PASS or E2E FAIL), screenshots (*.png),
# the generated HTML and PDF files, and observations.txt (facts recorded, not judged).
param([Parameter(Mandatory)][string]$Commit, [string]$Version, [string]$Editor = 'C:\rv\e2e\editor\richview-editor.exe', [switch]$SkipPrevious)
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$root = 'C:\rv\e2e'
$out = Join-Path $root 'out'
$work = Join-Path $root 'work'
foreach ($dir in $out, $work) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
Start-Transcript -Path (Join-Path $out 'e2e.log') -Force | Out-Null
$failed = New-Object System.Collections.Generic.List[string]
$script:passed = 0
function Step([string]$Name) { "`n== $Name  $(Get-Date -Format o)" }
function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
  if ($Ok) { $script:passed++; "PASS  $Name" } else { "FAIL  $Name $Detail"; $failed.Add($Name) }
}
function Note([string]$Text) { "NOTE  $Text"; Add-Content -Path (Join-Path $out 'observations.txt') -Value $Text }
$prefix = Join-Path $HOME '.richview'
$rv = Join-Path $prefix 'bin\richview.exe'
$edge = @("${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe", "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe") | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
# Charts draw with WebGL2. A cloud machine has no GPU, so headless Edge gets SwiftShader as its
# fallback; on a PC Edge draws through the GPU or WARP.
function Shot([string]$Url, [string]$Name, [int]$Height = 1600, [switch]$Charts) {
  $file = Join-Path $out $Name
  $p = Start-Process -FilePath $edge -PassThru -WindowStyle Hidden -ArgumentList '--headless', '--enable-unsafe-swiftshader', '--disable-lcd-text', '--hide-scrollbars', '--no-first-run', "--user-data-dir=$out\edge-shots", "--screenshot=$file", "--window-size=1280,$Height", '--virtual-time-budget=15000', $Url
  if (-not $p.WaitForExit(90000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
  Check "screenshot $Name" ((Test-Path -LiteralPath $file) -and (Get-Item -LiteralPath $file).Length -gt 5000)
  if ($Charts -and (Test-Path -LiteralPath $file)) {
    $colored = Colored $file
    "$Name has $colored strongly coloured pixels"
    Check "the charts in $Name are drawn" ($colored -gt 1500) "($colored strongly coloured pixels; a chart still loading has almost none)"
  }
}
# Pixels whose channels differ by more than 80: chart marks and a few coloured words. LCD text is
# off in these screenshots, so text adds no coloured fringes.
function Colored([string]$Png) {
  Add-Type -AssemblyName System.Drawing
  $bitmap = [Drawing.Bitmap]::FromFile($Png)
  try {
    $count = 0
    for ($y = 0; $y -lt $bitmap.Height; $y += 2) {
      for ($x = 0; $x -lt $bitmap.Width; $x += 2) {
        $c = $bitmap.GetPixel($x, $y)
        $high = [Math]::Max($c.R, [Math]::Max($c.G, $c.B)); $low = [Math]::Min($c.R, [Math]::Min($c.G, $c.B))
        if ($high - $low -gt 80) { $count++ }
      }
    }
    $count * 4
  } finally { $bitmap.Dispose() }
}
function FileUrl([string]$Path) { 'file:///' + $Path.Replace('\', '/') }
function Marked([string]$Dir) {
  $files = @(Get-ChildItem -LiteralPath $Dir -Recurse -File -Force)
  $marked = @($files | Where-Object { Get-Item -LiteralPath $_.FullName -Stream Zone.Identifier -ErrorAction SilentlyContinue })
  "$($marked.Count) of $($files.Count) installed files carry a Zone.Identifier stream$(if ($marked.Count) { ': ' + (($marked | ForEach-Object { $_.Name }) -join ', ') })"
}
function Wait-Port([int]$Port, [int]$Seconds = 30) {
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    try { $c = New-Object Net.Sockets.TcpClient; $c.Connect('127.0.0.1', $Port); $c.Close(); return $true } catch { Start-Sleep -Milliseconds 300 }
  }
  return $false
}
Add-Type -AssemblyName System.Web.Extensions
$json = New-Object System.Web.Script.Serialization.JavaScriptSerializer
$json.MaxJsonLength = [int]::MaxValue

Step 'the machine'
$os = Get-CimInstance Win32_OperatingSystem
"windows: $($os.Caption) $($os.Version) build $($os.BuildNumber); user $env:USERNAME; elevated $(([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))"
Note "windows: $($os.Caption) $($os.Version)"
Check 'Microsoft Edge is installed' ([bool]$edge)
if ($edge) { Note "edge: $((Get-Item $edge).VersionInfo.ProductVersion) at $edge" }
Check 'no RichView is installed yet' (-not (Test-Path -LiteralPath $prefix))

Step 'serve the release files on loopback, as the release server would'
$server = Start-Job -ArgumentList (Join-Path $root 'release') -ScriptBlock {
  param($dir)
  $listener = [Net.HttpListener]::new(); $listener.Prefixes.Add('http://127.0.0.1:8765/'); $listener.Start()
  while ($listener.IsListening) {
    $context = $listener.GetContext()
    $file = Join-Path $dir ([IO.Path]::GetFileName($context.Request.Url.AbsolutePath))
    if (Test-Path -LiteralPath $file -PathType Leaf) { $bytes = [IO.File]::ReadAllBytes($file); $context.Response.ContentLength64 = $bytes.Length; $context.Response.OutputStream.Write($bytes, 0, $bytes.Length) }
    else { $context.Response.StatusCode = 404 }
    $context.Response.Close()
  }
}
Check 'the release server listens' (Wait-Port 8765)
"archive sha256 $((Get-FileHash -Algorithm SHA256 (Join-Path $root 'release\richview-x86_64-pc-windows-msvc.tar.gz')).Hash.ToLowerInvariant())"

$kept = Join-Path $work 'kept-report'
if (-not $SkipPrevious) {
  Step 'the previous release, with the public one-liner'
  powershell -NoProfile -ExecutionPolicy Bypass -Command 'irm https://github.com/1vecera/richview/releases/latest/download/install.ps1 | iex'
  Check 'the published one-liner installs the previous release' ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $rv))
  $previous = & $rv --version
  "previous: $previous"
  Note "previous release: $previous"
  Note ("after the public one-liner: " + (Marked $prefix))
  Copy-Item -Recurse (Join-Path $root 'examples\openai-vs-anthropic') $kept
  & $rv generate (Join-Path $kept 'report.rmd')
  Check 'the previous release generates a report' ($LASTEXITCODE -eq 0 -and (Test-Path (Join-Path $kept 'Rendered\report.html')))

  Step 'upgrade: uninstall the previous release, install the candidate'
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $prefix 'uninstall.ps1') -Uninstall -Prefix $prefix
  Check 'the previous release uninstalls' ($LASTEXITCODE -eq 0 -and -not (Test-Path -LiteralPath $prefix))
  Check 'its report is kept' (Test-Path (Join-Path $kept 'Rendered\report.html'))
}

Step 'install the candidate with the one-liner'
powershell -NoProfile -ExecutionPolicy Bypass -Command 'irm http://127.0.0.1:8765/install.ps1 | iex'
Check 'the one-liner installs the candidate' ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $rv))
Note ("after the candidate's one-liner (loopback): " + (Marked $prefix))
$printed = & $rv --version
$printed
Check 'the version names the commit' ("$printed" -match [regex]::Escape($Commit.Substring(0, 10)))
if ($Version) { Check "the version is $Version" ("$printed" -match ('^richview ' + [regex]::Escape($Version) + ' ')) }
if (-not $SkipPrevious) {
  & $rv check (Join-Path $kept 'report.rmd')
  Check 'the kept report checks with the candidate' ($LASTEXITCODE -eq 0)
}

Step 'init, write, check, generate'
$mine = Join-Path $work 'my-report'
& $rv init $mine
$rmd = Join-Path $mine 'report.rmd'
Check 'init writes a report folder' ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $rmd))
Add-Content -LiteralPath $rmd -Encoding UTF8 -Value @'

## Written on Windows

```metric
value: 3
label: Platforms with the same RichView
```
'@
& $rv check $rmd
Check 'check passes on the edited report' ($LASTEXITCODE -eq 0)
& $rv generate $rmd
$html = Join-Path $mine 'Rendered\report.html'
Check 'generate writes the report' ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $html))
Check 'the report carries the new section' ((Test-Path -LiteralPath $html) -and (Get-Content -Raw -LiteralPath $html).Contains('Written on Windows'))
Shot (FileUrl $html) '1-generated-report.png' -Charts

Step 'the local preview, and its reload'
$preview = Start-Process -FilePath $rv -ArgumentList 'serve', $rmd, '--port', '4711' -PassThru -WindowStyle Hidden
Check 'the preview listens' (Wait-Port 4711)
try { $page = Invoke-WebRequest -UseBasicParsing http://127.0.0.1:4711/; Check 'the preview serves the report' ($page.StatusCode -eq 200 -and $page.Content.Contains('Written on Windows')) }
catch { Check 'the preview serves the report' $false $_.Exception.Message }
Shot 'http://127.0.0.1:4711/' '2-local-preview.png' -Charts
(Get-Content -Raw -LiteralPath $rmd).Replace('## Written on Windows', '## Edited while the preview runs') | Set-Content -LiteralPath $rmd -Encoding UTF8 -NoNewline
$reloaded = $false
foreach ($i in 1..30) {
  Start-Sleep -Milliseconds 500
  try { if ((Invoke-WebRequest -UseBasicParsing http://127.0.0.1:4711/).Content.Contains('Edited while the preview runs')) { $reloaded = $true; break } } catch {}
}
Check 'the preview shows the edit' $reloaded
Shot 'http://127.0.0.1:4711/' '3-local-preview-after-edit.png'
Stop-Process -Id $preview.Id -Force -ErrorAction SilentlyContinue

Step 'the editor service, as the VS Code extension drives it'
Check 'the editor service is present' (Test-Path -LiteralPath $Editor)
if (Test-Path -LiteralPath $Editor) {
  # The client reads the editor on its own thread, with timed receives, so a silent editor fails a
  # check instead of hanging the walk. The document text is a plain .NET string: Get-Content's
  # carries provider properties that ConvertTo-Json serializes as a deep object.
  Add-Type -TypeDefinition @'
using System; using System.Collections.Concurrent; using System.Diagnostics; using System.Text; using System.Threading;
public class LspPipe {
  readonly Process process;
  readonly BlockingCollection<string> inbox = new BlockingCollection<string>();
  public readonly StringBuilder Errors = new StringBuilder();
  public LspPipe(string exe) {
    var start = new ProcessStartInfo(exe) { UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true, CreateNoWindow = true };
    process = Process.Start(start);
    new Thread(ReadAll) { IsBackground = true }.Start();
    new Thread(() => { string line; while ((line = process.StandardError.ReadLine()) != null) lock (Errors) Errors.AppendLine(line); }) { IsBackground = true }.Start();
  }
  void ReadAll() {
    var stream = process.StandardOutput.BaseStream;
    try {
      while (true) {
        var header = new StringBuilder();
        while (header.Length < 4 || header.ToString(header.Length - 4, 4) != "\r\n\r\n") { int b = stream.ReadByte(); if (b < 0) return; header.Append((char)b); }
        int length = int.Parse(System.Text.RegularExpressions.Regex.Match(header.ToString(), @"Content-Length: (\d+)").Groups[1].Value);
        var body = new byte[length]; int read = 0;
        while (read < length) { int r = stream.Read(body, read, length - read); if (r <= 0) return; read += r; }
        inbox.Add(Encoding.UTF8.GetString(body));
      }
    } catch (Exception) { } finally { inbox.CompleteAdding(); }
  }
  public void Send(string json) {
    var body = Encoding.UTF8.GetBytes(json); var head = Encoding.ASCII.GetBytes("Content-Length: " + body.Length + "\r\n\r\n");
    var input = process.StandardInput.BaseStream; input.Write(head, 0, head.Length); input.Write(body, 0, body.Length); input.Flush();
  }
  public string Receive(int milliseconds) { string message; try { return inbox.TryTake(out message, milliseconds) ? message : null; } catch (InvalidOperationException) { return null; } }
  public bool WaitForExit(int milliseconds) { return process.WaitForExit(milliseconds); }
  public int ExitCode { get { return process.ExitCode; } }
  public void Kill() { try { process.Kill(); } catch (Exception) { } }
}
'@
  $lsp = New-Object LspPipe $Editor
  $script:notes = New-Object Collections.Generic.List[object]
  function Send($message) { $lsp.Send(($message | ConvertTo-Json -Depth 20 -Compress)) }
  function Ask([int]$Id, [string]$Method, $Params, [int]$Seconds = 120) {
    Send @{ jsonrpc = '2.0'; id = $Id; method = $Method; params = $Params }
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
      $text = $lsp.Receive(1000); if ($null -eq $text) { continue }
      $m = $json.DeserializeObject($text)
      if ($m.ContainsKey('id') -and $m['id'] -eq $Id) { return $m }
      $script:notes.Add($m)
    }
    return $null
  }
  function Diagnosed([string]$Uri, [int]$Seconds = 10) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
      if (@($script:notes | Where-Object { $_['method'] -eq 'textDocument/publishDiagnostics' -and $_['params']['uri'] -eq $Uri }).Count) { return $true }
      $text = $lsp.Receive(500); if ($null -ne $text) { $script:notes.Add($json.DeserializeObject($text)) }
    }
    return $false
  }
  # VS Code's own form of a Windows file URI: lower-case drive, colon escaped.
  $uri = 'file:///' + $rmd.Substring(0, 1).ToLowerInvariant() + '%3A' + $rmd.Substring(2).Replace('\', '/')
  "document $uri"
  $init = Ask 1 'initialize' @{ processId = $PID; rootUri = $null; capabilities = @{} }
  Check 'the editor service answers initialize' ($null -ne $init -and $init.ContainsKey('result'))
  Send @{ jsonrpc = '2.0'; method = 'initialized'; params = @{} }
  Send @{ jsonrpc = '2.0'; method = 'textDocument/didOpen'; params = @{ textDocument = @{ uri = $uri; languageId = 'richview'; version = 1; text = [IO.File]::ReadAllText($rmd) } } }
  Check 'the editor publishes diagnostics for the document' (Diagnosed $uri)
  $preview = Ask 2 'richview/preview' @{ uri = $uri }
  $previewHtml = if ($preview -and $preview.ContainsKey('result')) { $preview['result']['html'] } else { $null }
  Check 'the editor renders its preview' ($previewHtml -and $previewHtml.Contains('Edited while the preview runs')) $(if ($preview -and $preview.ContainsKey('error')) { $preview['error']['message'] })
  if ($previewHtml) { $file = Join-Path $out 'editor-preview.html'; [IO.File]::WriteAllText($file, $previewHtml); Shot (FileUrl $file) '4-editor-preview.png' }
  $export = Ask 3 'richview/export' @{ uri = $uri; output = 'pdf' }
  $exportText = if ($export) { $json.Serialize($export) } else { 'no answer' }
  "editor export: $($exportText.Substring(0, [Math]::Min(400, $exportText.Length)))"
  Check 'the editor exports a PDF' ($export -and $export.ContainsKey('result') -and (Test-Path (Join-Path $mine 'Rendered\report.pdf')))
  [void](Ask 4 'shutdown' $null 30); Send @{ jsonrpc = '2.0'; method = 'exit' }
  $exited = $lsp.WaitForExit(10000)
  Check 'the editor service exits' ($exited -and $lsp.ExitCode -eq 0)
  if (-not $exited) { $lsp.Kill() }
  $errors = $lsp.Errors.ToString(); if ($errors) { "editor stderr: $($errors.Substring(0, [Math]::Min(600, $errors.Length)))" }
}

Step 'PDF from the command line, and nothing left running'
$before = @(Get-Process msedge, chrome -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
$flagship = Join-Path $work 'flagship'
Copy-Item -Recurse (Join-Path $root 'examples\openai-vs-anthropic') $flagship
$printedPdf = & $rv generate (Join-Path $flagship 'report.rmd') --to pdf --format json 2>&1
"$printedPdf".Substring(0, [Math]::Min(600, "$printedPdf".Length))
$pdf = Join-Path $flagship 'Rendered\report.pdf'
Check 'generate --to pdf exits 0' ($LASTEXITCODE -eq 0)
$head = if (Test-Path -LiteralPath $pdf) { [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($pdf), 0, 5) } else { '' }
Check 'the PDF is a PDF' ($head -eq '%PDF-')
if (Test-Path -LiteralPath $pdf) { Note "flagship PDF: $((Get-Item $pdf).Length) bytes; $([regex]::Match("$printedPdf", '"pages":\s*\d+').Value)" }
Start-Sleep -Seconds 3
$left = @(Get-Process msedge, chrome -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id })
Check 'no browser process is left after printing' ($left.Count -eq 0) "($($left.Count) left)"
Copy-Item $pdf (Join-Path $out 'flagship.pdf') -ErrorAction SilentlyContinue

Step 'SQL data sources'
$sql = Join-Path $work 'sql-report'
Copy-Item -Recurse (Join-Path $root 'examples\sql-report') $sql
Check 'the SQL example has its database' (Test-Path (Join-Path $sql 'data\warehouse.duckdb'))
$sqlOut = & $rv generate (Join-Path $sql 'report.rmd') 2>&1
$sqlOut | Select-Object -First 5
$sqlHtml = Join-Path $sql 'Rendered\report.html'
Check 'a SQL report generates' ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $sqlHtml))
Check 'its chart carries the warehouse rows' ((Test-Path -LiteralPath $sqlHtml) -and (Get-Content -Raw -LiteralPath $sqlHtml) -match '180')
Shot (FileUrl $sqlHtml) '6-sql-report.png' -Charts
$hostile = Join-Path $work 'hostile-sql'
Copy-Item -Recurse $sql $hostile
Remove-Item -Recurse -Force (Join-Path $hostile 'Rendered') -ErrorAction SilentlyContinue
$winIni = (Join-Path $env:SystemRoot 'win.ini').Replace('\', '/')
Set-Content -LiteralPath (Join-Path $hostile 'queries\orders-by-month.sql') -Value "select content as month, 1 as orders from read_text('$winIni')"
(Get-Content -Raw -LiteralPath (Join-Path $hostile 'report.rmd')).Replace('key: orders-by-month', 'key: hostile').Replace('ttl: 1h', 'refresh: true') | Set-Content -LiteralPath (Join-Path $hostile 'report.rmd') -Encoding UTF8 -NoNewline
$hostileOut = & $rv generate (Join-Path $hostile 'report.rmd') 2>&1
$hostileOut | Select-Object -First 4
Check 'SQL that reads a host file is refused with the refusal diagnostic' ($LASTEXITCODE -ne 0 -and "$hostileOut" -match 'data\.connector\.failed' -and "$hostileOut" -match 'SQL external access was refused' -and -not ("$hostileOut" -match 'for 16-bit app support'))
Check 'no SQL worker is left running' (@(Get-Process richview-sql-worker -ErrorAction SilentlyContinue).Count -eq 0)

Step 'the MCP server'
$mcpIn = @('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}}', '{"jsonrpc":"2.0","method":"notifications/initialized"}', '{"jsonrpc":"2.0","id":2,"method":"tools/list"}') -join "`n"
$mcpOut = @($mcpIn | & $rv mcp)
Check 'the MCP server answers initialize' ("$($mcpOut[0])" -match '"result"' -and "$($mcpOut[0])" -match 'serverInfo')
Check 'the MCP server lists its tools' ("$mcpOut" -match 'richview_learn')

Step 'publish to a RichView server and open the result'
# As docs/public/publishing.md does it: a disposable token, the application on 127.0.0.1 and the
# reports on localhost, anyone with the link may read.
$env:RICHVIEW_SERVER = 'http://127.0.0.1:8090'
$env:RICHVIEW_TOKEN = -join ((1..48) | ForEach-Object { '{0:x}' -f (Get-Random -Maximum 16) })
$cowork = Start-Process -FilePath $rv -ArgumentList 'server', '--data-dir', (Join-Path $work 'server-data'), '--port', '8090', '--public-url', 'http://127.0.0.1:8090', '--report-port', '8091', '--report-origin', 'http://localhost:8091', '--default-visibility', 'public' -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $out 'server.log') -RedirectStandardError (Join-Path $out 'server.err.log')
Check 'the server listens' (Wait-Port 8090)
try { Check 'the server answers /health' ((Invoke-WebRequest -UseBasicParsing http://127.0.0.1:8090/health).StatusCode -eq 200) } catch { Check 'the server answers /health' $false $_.Exception.Message }
$published = & $rv publish $rmd --server http://127.0.0.1:8090 --slug windows-walk --new --format json 2>&1
Check 'publish exits 0' ($LASTEXITCODE -eq 0)
Remove-Item Env:\RICHVIEW_TOKEN, Env:\RICHVIEW_SERVER
"$published".Substring(0, [Math]::Min(600, "$published".Length))
$address = [regex]::Matches("$published", 'http://(127\.0\.0\.1|localhost):\d+/[^"\s]*') | ForEach-Object { $_.Value } | Where-Object { $_ -notmatch '/api/' } | Select-Object -First 1
"published at $address"
if ($address) {
  try { $page = Invoke-WebRequest -UseBasicParsing $address; Check 'the published report opens' ($page.StatusCode -eq 200 -and $page.Content.Contains('Edited while the preview runs')) }
  catch { Check 'the published report opens' $false $_.Exception.Message }
  Shot $address '7-published-report.png' -Charts
} else { Check 'the publication names its address' $false }
Stop-Process -Id $cowork.Id -Force -ErrorAction SilentlyContinue

Step 'uninstall'
# The CLI's state folder, by its own order of preference (install.ps1 names the same one).
$state = Join-Path $(if ($env:XDG_STATE_HOME) { $env:XDG_STATE_HOME } elseif ($env:HOME) { Join-Path (Join-Path $env:HOME '.local') 'state' } else { $env:LOCALAPPDATA }) 'richview'
$hadState = Test-Path -LiteralPath $state
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $prefix 'uninstall.ps1') -Uninstall -Prefix $prefix
Check 'uninstall exits 0' ($LASTEXITCODE -eq 0)
Check 'the installation folder is gone' (-not (Test-Path -LiteralPath $prefix))
Check 'the reports are kept' ((Test-Path -LiteralPath $html) -and (Test-Path -LiteralPath $sqlHtml))
if ($hadState) { Check 'the RichView state is kept' (Test-Path -LiteralPath $state) }
Stop-Job $server; Remove-Job $server -Force

Step 'result'
"$($script:passed) checks passed, $($failed.Count) failed"
if ($failed.Count) { "E2E FAIL: $($failed -join '; ')" } else { 'E2E PASS' }
Stop-Transcript | Out-Null
