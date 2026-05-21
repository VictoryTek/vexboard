$ErrorActionPreference = 'Stop'
$Failed = 0

function Step($name) { Write-Host ""; Write-Host "--- $name ---" }
function Pass($msg) { Write-Host "[PASS] $msg" -ForegroundColor Green }
function Fail($msg) { Write-Host "[FAIL] $msg" -ForegroundColor Red; $script:Failed++ }

Write-Host "=== VexBoard Preflight Checks ==="

Set-Location "$PSScriptRoot\.."

# 1. Format
Step "Formatting"
cargo fmt --all -- --check
if ($LASTEXITCODE -eq 0) { Pass "cargo fmt" } else { Fail "cargo fmt (run 'cargo fmt --all' to fix)" }

# 2. Clippy
Step "Lint (clippy)"
cargo clippy --workspace -- -D warnings
if ($LASTEXITCODE -eq 0) { Pass "cargo clippy" } else { Fail "cargo clippy" }

# 3. Tests
Step "Tests"
cargo test --workspace
if ($LASTEXITCODE -eq 0) { Pass "cargo test" } else { Fail "cargo test" }

# 4. Backend build
Step "Backend build (release)"
cargo build --release --bin vexboard-server
if ($LASTEXITCODE -eq 0) { Pass "cargo build --release" } else { Fail "cargo build --release" }

# 5. Security audit (optional)
Step "Security audit"
$auditAvailable = (Get-Command cargo-audit -ErrorAction SilentlyContinue) -ne $null
if ($auditAvailable) {
    # RUSTSEC-2023-0071: rsa via sqlx-mysql — sqlx's optional mysql dep is
    # always present in Cargo.lock but never compiled (sqlite-only features).
    cargo audit --ignore RUSTSEC-2023-0071
    if ($LASTEXITCODE -eq 0) { Pass "cargo audit" } else { Fail "cargo audit" }
} else {
    Write-Host "[SKIP] cargo-audit not installed"
}

Write-Host ""
Write-Host "==================================="
if ($Failed -eq 0) {
    Write-Host "All preflight checks passed." -ForegroundColor Green
    exit 0
} else {
    Write-Host "$Failed check(s) failed." -ForegroundColor Red
    exit 1
}
