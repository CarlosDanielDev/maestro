#!/usr/bin/env bats
#
# Bats tests live in scripts/tests/ — the one directory the CI bats job globs
# (#1355, #1378). A .bats file anywhere else would never run in CI.

@test "every tracked .bats file is under scripts/tests/" {
  cd "$(dirname "$BATS_TEST_FILENAME")/../.."
  run bash -c "git ls-files '*.bats' | grep -v -e '^scripts/tests/'"
  [ -z "$output" ] || { echo "bats outside scripts/tests/: $output"; false; }
}
