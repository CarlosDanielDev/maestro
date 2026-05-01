#!/usr/bin/env bash
# Tests for sentinel-path.sh — verifies the path resolution chain.
# Run with: bash .claude/hooks/tests/test-sentinel.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HELPER="$SCRIPT_DIR/../sentinel-path.sh"

if [ ! -f "$HELPER" ]; then
  echo "FAIL: $HELPER does not exist"
  exit 1
fi

pass=0
fail=0

run_case() {
  local name="$1"
  local actual="$2"
  local expected="$3"
  if [ "$actual" = "$expected" ]; then
    echo "PASS: $name"
    pass=$((pass + 1))
  else
    echo "FAIL: $name — expected '$expected', got '$actual'"
    fail=$((fail + 1))
  fi
}

# Case 1: with XDG_RUNTIME_DIR set, that path is preferred.
fake_xdg=$(mktemp -d)
actual=$(env -i HOME=/tmp/fake-home XDG_RUNTIME_DIR="$fake_xdg" bash -c "
  source '$HELPER'
  echo \"\$SENTINEL_PATH\"
")
run_case "xdg_runtime_dir_preferred" "$actual" "$fake_xdg/maestro-current-gate-dir"
rm -rf "$fake_xdg"

# Case 2: without XDG_RUNTIME_DIR, falls back to $HOME/.cache/maestro.
fake_home=$(mktemp -d)
actual=$(env -i HOME="$fake_home" bash -c "
  source '$HELPER'
  echo \"\$SENTINEL_PATH\"
")
run_case "no_xdg_uses_home_cache" "$actual" "$fake_home/.cache/maestro/maestro-current-gate-dir"
# Helper must have created the directory.
if [ -d "$fake_home/.cache/maestro" ]; then
  echo "PASS: helper creates \$HOME/.cache/maestro on first run"
  pass=$((pass + 1))
else
  echo "FAIL: helper did NOT create \$HOME/.cache/maestro"
  fail=$((fail + 1))
fi
rm -rf "$fake_home"

echo ""
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
