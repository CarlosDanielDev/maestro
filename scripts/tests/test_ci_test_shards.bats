#!/usr/bin/env bats
# Structural checks for the sharded CI test stage (#1308).
# "Test" is a required status check, so it must stay a single job that
# fails when any shard fails — a skipped aggregator reports success.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  CI="$REPO_ROOT/.github/workflows/ci.yml"
}

# Print one job block (from "  <id>:" up to the next top-level job key).
job_block() {
  awk -v id="  $1:" '
    $0 == id { inside = 1; print; next }
    inside && /^  [A-Za-z0-9_-]+:$/ { exit }
    inside { print }
  ' "$CI"
}

@test "shard job is a 4-way nextest hash partition" {
  block="$(job_block test-shard)"
  [ -n "$block" ]
  [[ "$block" == *"shard: [1, 2, 3, 4]"* ]] || false
  [[ "$block" == *'cargo nextest run --locked --partition hash:${{ matrix.shard }}/4'* ]] || false
  [[ "$block" == *'INSTA_UPDATE: "no"'* ]] || false
}

@test "required check name Test is kept by the aggregator job" {
  block="$(job_block test)"
  [[ "$block" == *"name: Test"$'\n'* ]] || false
  [[ "$block" == *"needs: [changes, test-shard, test-full]"* ]] || false
}

@test "aggregator fails instead of skipping when a shard fails" {
  block="$(job_block test)"
  [[ "$block" == *"if: always()"* ]] || false
  [[ "$block" == *'needs.test-shard.result'* ]] || false
  [[ "$block" == *'needs.test-full.result'* ]] || false
}

@test "full-suite job keeps doctests, snapshot grep and lockfile check" {
  block="$(job_block test-full)"
  [ -n "$block" ]
  [[ "$block" == *"cargo test --doc --locked"* ]] || false
  [[ "$block" == *'cargo test --verbose --locked 2>&1 | (! grep -q "pending snapshots")'* ]] || false
  [[ "$block" == *"git diff --exit-code Cargo.lock"* ]] || false
}

@test "wave gate runs nextest" {
  grep -q '"cargo nextest run --locked"' "$REPO_ROOT/.wave.json"
}
