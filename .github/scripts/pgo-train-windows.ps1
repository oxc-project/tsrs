# PGO training for the Windows release binary. The conformance runner exercises the
# compiler in process; a separate CLI run covers argument parsing and disk I/O.
param(
    [Parameter(Mandatory = $true)][string]$BinDir,
    [Parameter(Mandatory = $true)][string]$RawDir
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $RawDir | Out-Null
$env:TSRS_LIB_PATH = Join-Path $env:GITHUB_WORKSPACE "crates/tsrs_vfs/libs"
$env:TSRS_TEST_RESULTS = Join-Path $env:RUNNER_TEMP "pgo/test-results"

$env:LLVM_PROFILE_FILE = Join-Path $RawDir "suite-%m.profraw"
& (Join-Path $BinDir "tsrs-test.exe") run --suite all --jobs 2 --timeout 120
if ($LASTEXITCODE -ne 0) { throw "tsrs-test exited with $LASTEXITCODE" }

$summary = Join-Path $env:TSRS_TEST_RESULTS "summary.json"
if (-not (Test-Path $summary)) { throw "missing conformance summary $summary" }
$tests = (Get-Content $summary -Raw | ConvertFrom-Json).tests
if ($tests.Count -lt 10000) { throw "conformance training ran only $($tests.Count) cases" }

foreach ($name in @("crash", "timeout")) {
    $file = Join-Path $env:TSRS_TEST_RESULTS "$name.txt"
    if (-not (Test-Path $file)) { throw "missing conformance result $file" }
    if ((Get-Item $file).Length -ne 0) { throw "conformance runner reported $name cases" }
}

$env:LLVM_PROFILE_FILE = Join-Path $RawDir "cli-%m.profraw"
& (Join-Path $BinDir "tsrs.exe") -p testdata/split-check/declaration-errors --noEmit --incremental false --pretty false | Out-Null
if ($LASTEXITCODE -notin @(0, 2)) { throw "tsrs CLI training exited with $LASTEXITCODE" }

foreach ($prefix in @("suite", "cli")) {
    $profiles = @(Get-ChildItem $RawDir -Filter "$prefix-*.profraw" | Where-Object Length -gt 0)
    if ($profiles.Count -eq 0) { throw "no nonempty $prefix profile" }
}
