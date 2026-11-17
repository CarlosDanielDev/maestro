#!/usr/bin/env bats
#
# Script tests live in scripts/tests/ — the only directory the CI bats job
# runs (#1355). A .bats file anywhere else under tests/scripts/ never runs in CI.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  cd "$REPO_ROOT"
}

@test "no bats files left in tests/scripts/ (CI does not run them)" {
  run git ls-files 'tests/scripts/*.bats'
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "CI bats job runs scripts/tests/*.bats" {
  grep -q 'run: bats scripts/tests/\*\.bats' .github/workflows/ci.yml
}
