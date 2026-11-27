# INT-71: installs a checksum-verified native build on Windows x64. No services, PATH entries or
# registry keys are changed.
#   Install:    irm https://github.com/1vecera/richview/releases/latest/download/install.ps1 | iex
#   Options:    & ([scriptblock]::Create((irm <the same URL>))) -Prefix D:\tools\richview
#   Uninstall:  powershell -ExecutionPolicy Bypass -File <prefix>\uninstall.ps1 -Uninstall
# -Prefix defaults to .richview in the user's home folder and must not exist yet; its parent must.
# -BaseUrl serves the release assets directly: HTTPS, or HTTP on loopback for local verification.
# -Commit optionally confirms the installer's approved build identity.
# Uninstall refuses changed files or additional content in its owned folder.
param([string]$Prefix = '', [string]$BaseUrl = '', [string]$Commit = '', [switch]$Uninstall)
# The body runs in its own scope, so `irm | iex` leaves no variable or preference behind. It ends by
# throwing, never by `exit`, which would close the window that ran it.
& {
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
# Replaced only by scripts/assemble-release.ts after the archive is checked.
$release_commit='UNSTAMPED'
$release_base_url='UNSTAMPED'
$release_windows_sha256='UNSTAMPED'
$platform = 'x86_64-pc-windows-msvc'
function Fail([string]$Message) { throw "richview install: $Message" }
function Digest([string]$Path) { (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant() }
function Quote([string]$Text) { "'" + $Text.Replace("'", "''") + "'" }
function Linked($Item) { [bool]($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) }
function SafePath([string]$Path) {
  if ($Path -cnotmatch '^[a-zA-Z0-9_./-]+$') { return $false }
  foreach ($part in $Path.Split('/')) { if ($part -eq '' -or $part -eq '.' -or $part -eq '..') { return $false } }
  return $true
}
# Every file and folder below $Root except the installer's two records, as ./relative/path in
# code-unit order.
function Entries([string]$Root) {
  $base = (Get-Item -LiteralPath $Root -Force).FullName.TrimEnd('\', '/')
  # Typed, so that Sort orders this array and not a converted copy of it.
  [string[]]$names = @(Get-ChildItem -LiteralPath $base -Recurse -Force | ForEach-Object { './' + $_.FullName.Substring($base.Length + 1).Replace('\', '/') } | Where-Object { $_ -cne './.richview-install' -and $_ -cne './.richview-files' })
  [Array]::Sort($names, [StringComparer]::Ordinal)
  return $names
}
function VerifyPayload([string]$Dir) {
  $sums = Join-Path $Dir 'SHA256SUMS'
  if (-not (Test-Path -LiteralPath $sums -PathType Leaf)) { Fail 'archive has no file checksums' }
  $listed = New-Object System.Collections.Generic.List[string]
  foreach ($line in [IO.File]::ReadAllLines($sums)) {
    if ($line -cnotmatch '^([0-9a-f]{64})  (\S+)$') { Fail 'invalid file checksum record' }
    $expected = $Matches[1]; $file = $Matches[2]
    if (-not (SafePath $file)) { Fail 'unsafe checksum path' }
    if ($file -ceq 'SHA256SUMS') { Fail 'self-referencing checksum' }
    $path = Join-Path $Dir $file
    if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Linked (Get-Item -LiteralPath $path -Force))) { Fail "missing regular file: $file" }
    if ((Digest $path) -cne $expected) {
      if ($Uninstall) { Fail "file checksum mismatch: $file. Installation kept at $Prefix; restore this file before retrying." }
      Fail "file checksum mismatch: $file"
    }
    $listed.Add("./$file")
  }
  $expectedFiles = $listed.ToArray()
  [Array]::Sort($expectedFiles, [StringComparer]::Ordinal)
  $actualFiles = @(Entries $Dir | Where-Object { $_ -cne './SHA256SUMS' -and (Test-Path -LiteralPath (Join-Path $Dir $_) -PathType Leaf) })
  if (($expectedFiles -join "`n") -cne ($actualFiles -join "`n")) { Fail 'unlisted or duplicate archive files' }
}
if ($env:OS -ne 'Windows_NT') { Fail 'install.ps1 installs the Windows build. On macOS and Linux use install.sh.' }
$machine = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($machine -ne 'AMD64') { Fail 'This platform is not supported yet. On Windows RichView supports x64.' }
if (-not $Prefix) { $Prefix = Join-Path $HOME '.richview' }
if (-not [IO.Path]::IsPathRooted($Prefix)) { Fail 'prefix must be absolute' }
$Prefix = [IO.Path]::GetFullPath($Prefix).TrimEnd('\', '/')
$parent = Split-Path -Parent $Prefix
if (-not $parent -or -not (Test-Path -LiteralPath $parent -PathType Container)) { Fail 'the prefix parent folder must exist' }
if ($Uninstall) {
  $record = Join-Path $Prefix '.richview-install'
  $files = Join-Path $Prefix '.richview-files'
  if (-not (Test-Path -LiteralPath $Prefix -PathType Container) -or (Linked (Get-Item -LiteralPath $Prefix -Force)) -or -not (Test-Path -LiteralPath $record -PathType Leaf)) { Fail "prefix is not an owned installation: $Prefix" }
  foreach ($file in $files, (Join-Path $Prefix 'SHA256SUMS')) { if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { Fail 'installation metadata changed' } }
  if (@(Get-ChildItem -LiteralPath $Prefix -Recurse -Force | Where-Object { Linked $_ }).Count) { Fail 'installation contains a link' }
  if ([IO.File]::ReadAllText($record) -cne "richview-install-v1 $(Digest (Join-Path $Prefix 'SHA256SUMS')) $(Digest $files)`n") { Fail 'installation ownership record changed' }
  $current = @(Entries $Prefix)
  $recorded = @([IO.File]::ReadAllLines($files))
  $changes = @($current | Where-Object { $recorded -cnotcontains $_ } | ForEach-Object { "  added: $_" }) + @($recorded | Where-Object { $current -cnotcontains $_ } | ForEach-Object { "  missing: $_" })
  if ($changes.Count) { Fail ("installation contains additional or missing content. Move or back up added paths and restore missing paths before retrying; $Prefix was kept.`n" + ($changes -join "`n")) }
  VerifyPayload $Prefix
  # A rename moves the whole folder or nothing, so a file in use leaves the installation whole.
  $removed = Join-Path $parent ('.richview-uninstall.' + [IO.Path]::GetRandomFileName())
  try { [IO.Directory]::Move($Prefix, $removed) } catch { Fail "cannot remove $Prefix while a program uses it, such as a running richview.exe or a terminal inside the folder. Nothing was removed; close it and retry." }
  Remove-Item -LiteralPath $removed -Recurse -Force
  # The folders the CLI keeps state in, in its own order of preference.
  $state = if ($env:XDG_STATE_HOME) { $env:XDG_STATE_HOME } elseif ($env:HOME) { Join-Path (Join-Path $env:HOME '.local') 'state' } else { $env:LOCALAPPDATA }
  "Removed RichView from $Prefix"
  "Your reports and RichView state in $(Join-Path $state 'richview') (SQL snapshots and upload journals) were kept."
  "Remove any PATH entry you added for $(Join-Path $Prefix 'bin')."
  return
}
if (Test-Path -LiteralPath $Prefix) { Fail "prefix already exists: $Prefix. Use its uninstall.ps1 for a complete installation; save any personal files and move a partial one aside before reinstalling." }
if ($release_commit -cnotmatch '^[0-9a-f]{40}$') { Fail 'installer has not been assembled for a release' }
if ($Commit -and $Commit -cne $release_commit) { Fail 'requested commit differs from the approved release' }
if ($release_windows_sha256 -cnotmatch '^[0-9a-f]{64}$') { Fail 'installer has no approved archive digest' }
if (-not $BaseUrl) { $BaseUrl = $release_base_url }
$address = $null
if (-not [Uri]::TryCreate($BaseUrl, [UriKind]::Absolute, [ref]$address) -or $address.UserInfo -or $address.Query -or $address.Fragment) { Fail 'base URL must be absolute, without credentials, query or fragment' }
$loopback = $address.Scheme -eq 'http' -and @('127.0.0.1', 'localhost', '[::1]') -contains $address.Host
if ($address.Scheme -ne 'https' -and -not $loopback) { Fail '-BaseUrl requires HTTPS (or loopback HTTP)' }
# Windows 10 version 1803 and later ship this tar; another tar on PATH may read drive letters as hosts.
if (-not $env:SystemRoot) { Fail 'SystemRoot is not set' }
$tar = Join-Path (Join-Path $env:SystemRoot 'System32') 'tar.exe'
if (-not (Test-Path -LiteralPath $tar -PathType Leaf)) { Fail 'tar.exe is required; it is part of Windows 10 version 1803 and later' }
$work = Join-Path $parent ('.richview-install.' + [IO.Path]::GetRandomFileName())
try { New-Item -ItemType Directory -Path $work | Out-Null } catch { Fail "cannot create installation scratch in $parent; choose a writable -Prefix" }
try {
  $archive = Join-Path $work 'archive'
  $url = $BaseUrl.TrimEnd('/') + "/richview-$platform.tar.gz"
  # The stamped digest decides what is installed, whatever the download passed through.
  try { Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing -MaximumRedirection $(if ($loopback) { 0 } else { 5 }) }
  catch { Fail "could not download archive on $platform from ${url}: $($_.Exception.Message) Installation was not started." }
  if ((Digest $archive) -cne $release_windows_sha256) { Fail 'archive checksum mismatch; nothing installed' }
  $entries = @(& $tar -tzf $archive)
  if ($LASTEXITCODE) { Fail 'cannot read the archive' }
  foreach ($entry in $entries) {
    $entry = ($entry -replace '^\./', '') -replace '/$', ''
    if ($entry -eq '' -or $entry -eq '.') { continue }
    if (-not (SafePath $entry)) { Fail 'unsafe archive path' }
    if ($entry -cmatch '(^|/)\.richview-(install|files)$') { Fail 'archive contains installer-owned metadata' }
  }
  $types = @(& $tar -tvzf $archive)
  if ($LASTEXITCODE -or @($types | Where-Object { $_ -cnotmatch '^[-d]' }).Count) { Fail 'archive contains a link or special file' }
  $payload = Join-Path $work 'payload'
  New-Item -ItemType Directory -Path $payload | Out-Null
  & $tar -xzf $archive -C $payload
  if ($LASTEXITCODE) { Fail 'cannot unpack the archive' }
  VerifyPayload $payload
  foreach ($file in 'BUILD', 'bin/richview.exe', 'bin/richview-sql-worker.exe', 'uninstall.ps1') {
    if (-not (Test-Path -LiteralPath (Join-Path $payload $file) -PathType Leaf)) { Fail "archive file missing: $file" }
  }
  if ([IO.File]::ReadAllText((Join-Path $payload 'BUILD')) -cne "commit=$release_commit`nplatform=$platform`n") { Fail 'archive commit/platform differs from the request' }
  $plain = New-Object System.Text.UTF8Encoding $false
  [IO.File]::WriteAllText((Join-Path $payload '.richview-files'), ((@(Entries $payload) -join "`n") + "`n"), $plain)
  [IO.File]::WriteAllText((Join-Path $payload '.richview-install'), "richview-install-v1 $(Digest (Join-Path $payload 'SHA256SUMS')) $(Digest (Join-Path $payload '.richview-files'))`n", $plain)
  # The scratch folder is beside the prefix, so this rename installs everything or nothing.
  try { [IO.Directory]::Move($payload, $Prefix) } catch { Fail 'prefix appeared during download; nothing installed' }
} finally {
  if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
}
$cli = Quote (Join-Path (Join-Path $Prefix 'bin') 'richview.exe')
# The version is the one the installed CLI reports, so it matches `richview --version`.
$printed = ''
try { $printed = "$(& (Join-Path (Join-Path $Prefix 'bin') 'richview.exe') --version 2>$null | Select-Object -First 1)" } catch { $printed = '' }
if ($printed -cmatch '^richview ([0-9A-Za-z.+-]+) ') {
  "Installed RichView $($Matches[1]) (commit $($release_commit.Substring(0, 10))) in $Prefix"
} else {
  [Console]::Error.WriteLine("richview install: the installed CLI did not report its version; run & $cli --version to see why.")
  "Installed RichView build $release_commit in $Prefix"
}
"Add $(Join-Path $Prefix 'bin') to PATH, or invoke the CLI by its full path."
"Start a report: & $cli init my-report"
"Agent MCP command: & $cli mcp"
"Run your own report server, RichView Cowork: & $cli server --help"
'When you publish, choose RichView Cowork hosted by RichView at app.richview.ai, or your own server; no hosted service was started.'
"Uninstall: powershell -ExecutionPolicy Bypass -File $(Quote (Join-Path $Prefix 'uninstall.ps1')) -Uninstall -Prefix $(Quote $Prefix)"
}
