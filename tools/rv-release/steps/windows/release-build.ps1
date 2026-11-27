# Build the release programs of the commit checked out in -Dir and list their sha256 in -Out\SHA256SUMS.
# A commit whose .cargo\config.toml links the C runtime statically is built with no RUSTFLAGS; an
# older commit gets the shipped setting (+crt-static) from here. Then the DLLs each program depends
# on (dumpbin /dependents): none may be a Visual C++ runtime DLL.
param([Parameter(Mandatory = $true)][string]$Commit, [Parameter(Mandatory = $true)][string]$Dir, [Parameter(Mandatory = $true)][string]$Out)
$ErrorActionPreference = "Continue"
$env:PATH = "C:\rv\git\cmd;$env:USERPROFILE\.cargo\bin;$env:PATH"
Set-Location $Dir
git config --global --add safe.directory ($Dir.Replace("\", "/")) 2>$null
git config core.fileMode false
$head = (git rev-parse HEAD).Trim()
"head $head in $Dir"
if ($head -ne $Commit) { "wrong commit"; exit 1 }
if (@(git status --short).Count -ne 0) { "dirty tree"; git status --short; exit 1 }
$env:CARGO_TARGET_DIR = "C:\rv\target-release"
$env:RICHVIEW_RUNTIME_DIR = "$Dir\dist\web"
Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
if ((Test-Path .cargo\config.toml) -and (Select-String -Path .cargo\config.toml -Pattern "crt-static" -Quiet)) { "C runtime: static by the tracked .cargo\config.toml, no RUSTFLAGS" } else { $env:RUSTFLAGS = "-C target-feature=+crt-static"; "C runtime: static by RUSTFLAGS from this script (the commit has no tracked setting)" }
"== build $(Get-Date -Format o)"
cargo build --release --locked -p richview-app -p richview-editor --features richview-app/embedded-runtime,richview-editor/embedded-runtime 2>&1 | ForEach-Object { "$_" } | Where-Object { $_ -notmatch "^\s+(Compiling|Checking)" }
$built = $LASTEXITCODE
"== built $(Get-Date -Format o) exit $built"
if ($built -ne 0) { exit 1 }
if (@(git status --short).Count -ne 0) { "the build left the tree dirty"; git status --short; exit 1 }
New-Item -ItemType Directory -Path $Out -Force | Out-Null
$sums = foreach ($name in "richview.exe", "richview-sql-worker.exe", "richview-editor.exe") {
  Copy-Item "C:\rv\target-release\release\$name" $Out -Force
  "{0}  {1}" -f (Get-FileHash -Algorithm SHA256 (Join-Path $Out $name)).Hash.ToLowerInvariant(), $name
}
[IO.File]::WriteAllText((Join-Path $Out "SHA256SUMS"), (($sums -join "`n") + "`n"))
$sums
"version: $(& (Join-Path $Out 'richview.exe') --version)"
$dumpbin = Get-ChildItem -Recurse -Filter dumpbin.exe "C:\Program Files*\Microsoft Visual Studio" -ErrorAction SilentlyContinue | Where-Object FullName -match "Hostx64\\x64" | Select-Object -First 1
$runtime = 0
foreach ($name in "richview.exe", "richview-sql-worker.exe", "richview-editor.exe") {
  $dlls = @(& $dumpbin.FullName /nologo /dependents (Join-Path $Out $name) | ForEach-Object { $_.Trim() } | Where-Object { $_ -match "\.dll$" })
  "dependents of ${name}: $($dlls -join ", ")"
  $runtime += @($dlls | Where-Object { $_ -match "^(vcruntime|msvcp|msvcr|ucrtbase|api-ms-win-crt-)" }).Count
}
"Visual C++ runtime DLLs among them: $runtime"
"== done $(Get-Date -Format o)"
