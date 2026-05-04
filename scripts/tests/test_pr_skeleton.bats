#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/pr-skeleton.sh"
}

teardown() {
  rm -rf "${GATE_LOG_DIR:-}"
}

@test "standard template rendering" {
  run "$SCRIPT" 556 "refactor: mechanize pushup scaffolding"
  [ "$status" -eq 0 ]

  expected="$(mktemp)"
  cat > "$expected" <<'EOF'
## Summary

- <PLACEHOLDER bullet 1>
- <PLACEHOLDER bullet 2>
- <PLACEHOLDER bullet 3 — delete if not needed>

Closes #556

## Test plan

- [ ] cargo test --quiet
- [ ] cargo clippy -- -D warnings -A dead_code
- [ ] cargo fmt --check
- [ ] manual verification: <PLACEHOLDER>
EOF
  diff -u "$expected" <(printf '%s\n' "$output")
}

@test "placeholder tokens remain visible for orchestrator edits" {
  run "$SCRIPT" 556 "refactor: mechanize pushup scaffolding"
  [ "$status" -eq 0 ]

  [[ "$output" == *"<PLACEHOLDER bullet 1>"* ]]
  [[ "$output" == *"<PLACEHOLDER bullet 2>"* ]]
  [[ "$output" == *"<PLACEHOLDER bullet 3 — delete if not needed>"* ]]
  [[ "$output" == *"manual verification: <PLACEHOLDER>"* ]]
}

@test "substitutes issue number in closes line" {
  run "$SCRIPT" 987 "docs: update pushup flow"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Closes #987"* ]]
  [[ "$output" != *"Closes #556"* ]]
}

@test "writes PR draft when GATE_LOG_DIR is set" {
  GATE_LOG_DIR="$(mktemp -d)"
  export GATE_LOG_DIR

  run "$SCRIPT" 556 "refactor: mechanize pushup scaffolding"
  [ "$status" -eq 0 ]
  [ -f "$GATE_LOG_DIR/pr-draft.md" ]
  diff -u <(printf '%s\n' "$output") "$GATE_LOG_DIR/pr-draft.md"
}

@test "commit subject is not embedded in body" {
  run "$SCRIPT" 556 "refactor: mechanize pushup scaffolding"
  [ "$status" -eq 0 ]
  [[ "$output" != *"refactor: mechanize pushup scaffolding"* ]]
}
