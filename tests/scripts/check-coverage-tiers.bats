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

@test "70% core: report mode (default) exits 0 but prints VIOLATION" {
  run bash "$SCRIPT" "$FIXTURES/lcov-70pct-core.info"
  [ "$status" -eq 0 ]
  [[ "$output" == *"VIOLATION"* ]]
  [[ "$output" == *"core: 70.0%"* ]]
  [[ "$output" == *"report mode"* ]]
}

@test "70% core: --enforce mode exits 1 on violation" {
  run bash "$SCRIPT" --enforce "$FIXTURES/lcov-70pct-core.info"
  [ "$status" -eq 1 ]
  [[ "$output" == *"VIOLATION"* ]]
  [[ "$output" == *"core: 70.0%"* ]]
  # Should NOT show the "report mode" hint in enforce mode.
  [[ "$output" != *"report mode"* ]]
}

@test "mixed tiers: report mode exits 0, still prints violations" {
  run bash "$SCRIPT" "$FIXTURES/lcov-mixed-tiers.info"
  [ "$status" -eq 0 ]
  [[ "$output" == *"core: 95.0%"* ]]
  [[ "$output" == *"tui: 50.0%"* ]]
  [[ "$output" == *"VIOLATION"* ]]
  [[ "$output" != *"main.rs"* ]]
}

@test "mixed tiers: --enforce mode exits 1" {
  run bash "$SCRIPT" --enforce "$FIXTURES/lcov-mixed-tiers.info"
  [ "$status" -eq 1 ]
  [[ "$output" == *"core: 95.0%"* ]]
  [[ "$output" == *"tui: 50.0%"* ]]
}

@test "absolute lcov paths are normalized via REPO_ROOT_PREFIX env" {
  # cargo-llvm-cov on CI emits absolute paths like
  # /home/runner/work/maestro/maestro/src/tui/app.rs. The script's
  # normalize_path strips a configurable prefix so globs like src/tui/**
  # can match.
  REPO_ROOT_PREFIX="/home/runner/work/maestro/maestro/" run bash "$SCRIPT" --enforce "$FIXTURES/lcov-absolute-paths.info"
  [ "$status" -eq 1 ]
  [[ "$output" == *"core: 95.0%"* ]]
  [[ "$output" == *"tui: 40.0%"* ]]
  [[ "$output" != *"main.rs"* ]]
}
