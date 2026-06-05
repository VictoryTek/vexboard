#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "=== VexBoard Preflight Checks ==="
FAILED=0

step() { echo ""; echo "--- $1 ---"; }
pass() { echo "[PASS] $1"; }
fail() { echo "[FAIL] $1"; FAILED=$((FAILED + 1)); }

# 1. Format
step "Formatting"
if cargo fmt --all -- --check; then
  pass "cargo fmt"
else
  fail "cargo fmt (run 'cargo fmt --all' to fix)"
fi

# 2. Clippy
step "Lint (clippy)"
if cargo clippy --workspace -- -D warnings; then
  pass "cargo clippy"
else
  fail "cargo clippy"
fi

# 3. Tests (frontend is wasm32-only; exclude it from native test runs)
step "Tests"
if cargo test -p vexboard-server; then
  pass "cargo test"
else
  fail "cargo test"
fi

# 4. Backend release build
step "Backend build (release)"
if cargo build --release --bin vexboard-server; then
  pass "cargo build --release --bin vexboard-server"
else
  fail "cargo build --release --bin vexboard-server"
fi

# 5. Security audit (optional — skip if cargo-audit not installed)
step "Security audit"
if command -v cargo-audit &>/dev/null || cargo audit --version &>/dev/null 2>&1; then
  # RUSTSEC-2023-0071: rsa via sqlx-mysql — not compiled into this workspace
  # (sqlite-only features); rsa only appears via the sqlx-macros proc-macro
  # build path, never in the runtime binary.
  if cargo audit --ignore RUSTSEC-2023-0071; then
    pass "cargo audit"
  else
    fail "cargo audit (vulnerabilities found)"
  fi
else
  echo "[SKIP] cargo-audit not installed"
fi

echo ""
echo "==================================="
if [ "$FAILED" -eq 0 ]; then
  echo "All preflight checks passed."
  exit 0
else
  echo "$FAILED check(s) failed."
  exit 1
fi
