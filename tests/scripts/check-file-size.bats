#!/usr/bin/env bats
#
# Tests for scripts/check-file-size.sh
# Covers: existing 500-LOC cap, new deadline parsing, allowlist migration, stale paths.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/check-file-size.sh"
  FIXTURES="$REPO_ROOT/tests/scripts/fixtures"
  TEST_REPO="$(mktemp -d -t file-size-test-XXXXXX)"
  mkdir -p "$TEST_REPO/src" "$TEST_REPO/scripts"
  cp "$SCRIPT" "$TEST_REPO/scripts/check-file-size.sh"
  cd "$TEST_REPO"
}

teardown() {
  cd /
  rm -rf "$TEST_REPO"
}

@test "legacy-format allowlist is still honored (backward compat)" {
  # Set up a repo with a big file and a legacy allowlist.
  cp "$FIXTURES/allowlist-sample-legacy.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/big_legacy.rs
  run bash scripts/check-file-size.sh
  [ "$status" -eq 0 ]
}

@test "deadline in future passes (entry is honored)" {
  cp "$FIXTURES/allowlist-sample-valid.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/big_deadlined.rs
  run bash scripts/check-file-size.sh
  [ "$status" -eq 0 ]
}

@test "deadline in past fails the check" {
  cp "$FIXTURES/allowlist-sample-expired.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/big_expired.rs
  run bash scripts/check-file-size.sh
  [ "$status" -ne 0 ]
  [[ "$output" == *"DEADLINE PAST"* ]]
}

@test "legacy entry without deadline field is tolerated (no warning)" {
  cp "$FIXTURES/allowlist-sample-legacy.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/big_legacy.rs
  run bash scripts/check-file-size.sh
  [ "$status" -eq 0 ]
  [[ "$output" != *"DEADLINE"* ]]
}

@test "file over cap not on allowlist fails (regression check)" {
  cp "$FIXTURES/allowlist-sample-valid.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/unrelated_big.rs
  run bash scripts/check-file-size.sh
  [ "$status" -ne 0 ]
  [[ "$output" == *"VIOLATION"* ]]
}

@test "new 400 LOC cap: file at 450 LOC not on allowlist fails" {
  printf 'line\n%.0s' {1..450} > src/new_bigfile.rs
  echo "# empty" > scripts/allowlist-large-files.txt
  run bash scripts/check-file-size.sh
  [ "$status" -ne 0 ]
  [[ "$output" == *"VIOLATION"* ]]
  [[ "$output" == *"max 400"* ]]
}

@test "new 400 LOC cap: file at 390 LOC passes without allowlist" {
  printf 'line\n%.0s' {1..390} > src/small_file.rs
  echo "# empty" > scripts/allowlist-large-files.txt
  run bash scripts/check-file-size.sh
  [ "$status" -eq 0 ]
}
