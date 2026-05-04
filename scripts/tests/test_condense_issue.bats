#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/condense-issue.sh"
  FIXTURES="$REPO_ROOT/scripts/tests/fixtures"
}

@test "full DOR issue summary matches snapshot" {
  run "$SCRIPT" "$FIXTURES/issue-conformant.json"
  [ "$status" -eq 0 ]
  expected="$(mktemp)"
  cat > "$expected" <<'EOF'
# Add configurable retry policy

## Overview

Sessions need a configurable retry policy for transient failures.

## Expected Behavior

Transient failures retry according to config; permanent failures do not retry.

## Acceptance Criteria

- [ ] Retry count is configurable
- [ ] Permanent errors do not retry

## Blocked By

- #42

## Files to Modify

- src/session/retry.rs
- src/config/sessions.rs
EOF
  diff -u "$expected" <(printf '%s\n' "$output")
}

@test "missing sections are omitted" {
  run "$SCRIPT" "$FIXTURES/issue-missing-ac.json"
  [ "$status" -eq 0 ]
  [[ "$output" != *"## Acceptance Criteria"* ]]
  [[ "$output" == *"## Overview"* ]]
  [[ "$output" == *"## Blocked By"* ]]
}

@test "output is deterministic" {
  first="$(mktemp)"
  second="$(mktemp)"

  "$SCRIPT" "$FIXTURES/issue-conformant.json" > "$first"
  "$SCRIPT" "$FIXTURES/issue-conformant.json" > "$second"

  diff -u "$first" "$second"
}
