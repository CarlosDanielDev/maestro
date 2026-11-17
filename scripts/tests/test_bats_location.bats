#!/usr/bin/env bats
#
# Script tests live in scripts/tests/ — the directory the CI bats job globs
# (#1355). tests/hooks/ is the one other known home (not yet in CI). A .bats
# file anywhere else would never run in CI.

@test "every tracked .bats file is under scripts/tests/ or tests/hooks/" {
  cd "$(dirname "$BATS_TEST_FILENAME")/../.."
  run bash -c "git ls-files '*.bats' | grep -v -e '^scripts/tests/' -e '^tests/hooks/'"
  [ -z "$output" ] || { echo "bats outside scripts/tests/: $output"; false; }
}
