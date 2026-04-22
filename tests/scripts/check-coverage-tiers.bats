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

@test "absolute lcov paths are normalized via REPO_ROOT_PREFIX env" {
  # cargo-llvm-cov on CI emits absolute paths like
  # /home/runner/work/maestro/maestro/src/tui/app.rs. The script's
  # normalize_path strips a configurable prefix so globs like src/tui/**
  # can match.
  REPO_ROOT_PREFIX="/home/runner/work/maestro/maestro/" run bash "$SCRIPT" "$FIXTURES/lcov-absolute-paths.info"
  [ "$status" -eq 1 ]
  [[ "$output" == *"core: 95.0%"* ]]
  [[ "$output" == *"tui: 40.0%"* ]]
  [[ "$output" != *"main.rs"* ]]
}
