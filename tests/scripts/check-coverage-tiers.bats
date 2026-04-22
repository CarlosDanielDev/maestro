#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/check-coverage-tiers.sh"
  MANIFEST="$REPO_ROOT/scripts/coverage-tiers.yml"
  FIXTURES="$REPO_ROOT/tests/scripts/fixtures"
  cd "$REPO_ROOT"
}

@test "100% coverage passes core floor of 90%" {
  run bash "$SCRIPT" "$FIXTURES/lcov-100pct-core.info"
  [ "$status" -eq 0 ]
  [[ "$output" == *"core: 100.0%"* ]]
}

@test "70% coverage fails core floor of 90%" {
  run bash "$SCRIPT" "$FIXTURES/lcov-70pct-core.info"
  [ "$status" -eq 1 ]
  [[ "$output" == *"VIOLATION"* ]]
  [[ "$output" == *"core: 70.0%"* ]]
}

@test "mixed tiers: core passes, tui fails floor, excluded ignored" {
  run bash "$SCRIPT" "$FIXTURES/lcov-mixed-tiers.info"
  [ "$status" -eq 1 ]
  [[ "$output" == *"core: 95.0%"* ]]
  [[ "$output" == *"tui: 50.0%"* ]]
  [[ "$output" == *"VIOLATION"* ]]
  # main.rs is excluded, should not appear.
  [[ "$output" != *"main.rs"* ]]
}
