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
  [[ "$block" == *'cargo nextest run --workspace --locked --partition hash:${{ matrix.shard }}/4'* ]] || false
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

@test "doctest job keeps doctests and lockfile check" {
  block="$(job_block test-full)"
  [ -n "$block" ]
  [[ "$block" == *"cargo test --workspace --doc --locked"* ]] || false
  [[ "$block" == *"git diff --exit-code Cargo.lock"* ]] || false
}

# insta never prints "pending snapshots", so a grep for it can never fail.
# Unreviewed snapshots fail through INSTA_UPDATE=no on the shards instead.
@test "no pending-snapshots grep in CI or the wave gate" {
  run grep -n 'pending snapshots' "$CI" "$REPO_ROOT/.wave.json"
  [ "$status" -eq 1 ]
}

# --workspace (#1305): maestro-core's tests live in their own crate.
@test "wave gate runs nextest" {
  grep -q '"cargo nextest run --workspace --locked"' "$REPO_ROOT/.wave.json"
}
