#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/dor-lint.sh"
  FIXTURES="$REPO_ROOT/scripts/tests/fixtures"
  TEST_DIR="$(mktemp -d)"
  cp "$FIXTURES"/issue-*.json "$TEST_DIR"/

  SHIM_DIR="$(mktemp -d)"
  cat > "$SHIM_DIR/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [ "${1:-}" = "issue" ] && [ "${2:-}" = "view" ]; then
  issue="${3:-}"
  case "$issue" in
    42) echo '{"state":"CLOSED"}' ;;
    99) echo '{"state":"OPEN"}' ;;
    *) echo '{"state":"CLOSED"}' ;;
  esac
  exit 0
fi

echo "fake gh: unsupported command $*" >&2
exit 2
EOF
  chmod +x "$SHIM_DIR/gh"
  PATH="$SHIM_DIR:$PATH"
  export PATH
}

teardown() {
  rm -rf "$SHIM_DIR" "$TEST_DIR"
}

@test "passes conformant issue with closed blocker" {
  run "$SCRIPT" "$TEST_DIR/issue-conformant.json"
  [ "$status" -eq 0 ]

  output_file="$TEST_DIR/dor-lint.json"
  [ -f "$output_file" ]
  [ "$(jq -r .passed "$output_file")" = "true" ]
  [ "$(jq -r '.blocker_states["42"]' "$output_file")" = "CLOSED" ]
  [ "$(jq -r .task_type "$output_file")" = "feature" ]
}

@test "fails when Acceptance Criteria is missing" {
  run "$SCRIPT" "$TEST_DIR/issue-missing-ac.json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .passed "$TEST_DIR/dor-lint.json")" = "false" ]
  jq -e '.missing | index("Acceptance Criteria")' "$TEST_DIR/dor-lint.json" >/dev/null
}

@test "fails when Blocked By is missing" {
  issue_json="$(mktemp)"
  jq 'del(.body) + {body: "## Overview\n\nx\n\n## Expected Behavior\n\nx\n\n## Acceptance Criteria\n\n- [ ] x\n\n## Definition of Done\n\n- [ ] done\n"}' \
    "$TEST_DIR/issue-conformant.json" > "$issue_json"

  run "$SCRIPT" "$issue_json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .passed "$(dirname "$issue_json")/dor-lint.json")" = "false" ]
  jq -e '.missing | index("Blocked By")' "$(dirname "$issue_json")/dor-lint.json" >/dev/null
}

@test "fails when a blocker is open" {
  run "$SCRIPT" "$TEST_DIR/issue-with-open-blockers.json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .passed "$TEST_DIR/dor-lint.json")" = "false" ]
  [ "$(jq -r '.blocker_states["99"]' "$TEST_DIR/dor-lint.json")" = "OPEN" ]
  jq -e '.reasons | index("open blocker: #99")' "$TEST_DIR/dor-lint.json" >/dev/null
}

@test "cross repo blocker forces fall-through" {
  issue_json="$(mktemp)"
  jq '.body |= sub("- #42"; "- owner/repo#42")' "$TEST_DIR/issue-conformant.json" > "$issue_json"

  run "$SCRIPT" "$issue_json"
  [ "$status" -eq 0 ]

  lint_json="$(dirname "$issue_json")/dor-lint.json"
  [ "$(jq -r .passed "$lint_json")" = "false" ]
  jq -e '.reasons | index("cross-repo blocker requires gatekeeper")' "$lint_json" >/dev/null
  [ "$(jq -r '.blockers | length' "$lint_json")" -eq 0 ]
}

@test "contract reference forces fall-through" {
  run "$SCRIPT" "$TEST_DIR/issue-with-contract-ref.json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .passed "$TEST_DIR/dor-lint.json")" = "false" ]
  jq -e '.reasons | index("contract validation required")' "$TEST_DIR/dor-lint.json" >/dev/null
}

@test "bug label requires bug-only sections and maps task type" {
  issue_json="$(mktemp)"
  jq '.labels = [{"name":"bug"}]' "$TEST_DIR/issue-conformant.json" > "$issue_json"

  run "$SCRIPT" "$issue_json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .task_type "$(dirname "$issue_json")/dor-lint.json")" = "bug" ]
  jq -e '.missing | index("Current Behavior")' "$(dirname "$issue_json")/dor-lint.json" >/dev/null
  jq -e '.missing | index("Steps to Reproduce")' "$(dirname "$issue_json")/dor-lint.json" >/dev/null
}

@test "label maps documentation to docs" {
  issue_json="$(mktemp)"
  jq '.labels = [{"name":"documentation"}]' "$TEST_DIR/issue-conformant.json" > "$issue_json"

  run "$SCRIPT" "$issue_json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .task_type "$(dirname "$issue_json")/dor-lint.json")" = "docs" ]
}

@test "label maps type docs to docs" {
  issue_json="$(mktemp)"
  jq '.labels = [{"name":"type:docs"}]' "$TEST_DIR/issue-conformant.json" > "$issue_json"

  run "$SCRIPT" "$issue_json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .task_type "$(dirname "$issue_json")/dor-lint.json")" = "docs" ]
}

@test "label maps tech debt to refactor" {
  issue_json="$(mktemp)"
  jq '.labels = [{"name":"tech-debt"}]' "$TEST_DIR/issue-conformant.json" > "$issue_json"

  run "$SCRIPT" "$issue_json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .task_type "$(dirname "$issue_json")/dor-lint.json")" = "refactor" ]
}

@test "label maps refactor to refactor" {
  issue_json="$(mktemp)"
  jq '.labels = [{"name":"refactor"}]' "$TEST_DIR/issue-conformant.json" > "$issue_json"

  run "$SCRIPT" "$issue_json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .task_type "$(dirname "$issue_json")/dor-lint.json")" = "refactor" ]
}

@test "label maps enhancement to feature" {
  run "$SCRIPT" "$TEST_DIR/issue-conformant.json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .task_type "$TEST_DIR/dor-lint.json")" = "feature" ]
}

@test "label maps type feature to feature" {
  issue_json="$(mktemp)"
  jq '.labels = [{"name":"type:feature"}]' "$TEST_DIR/issue-conformant.json" > "$issue_json"

  run "$SCRIPT" "$issue_json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .task_type "$(dirname "$issue_json")/dor-lint.json")" = "feature" ]
}

@test "unknown labels map to trivial" {
  issue_json="$(mktemp)"
  jq '.labels = [{"name":"question"}]' "$TEST_DIR/issue-conformant.json" > "$issue_json"

  run "$SCRIPT" "$issue_json"
  [ "$status" -eq 0 ]

  [ "$(jq -r .task_type "$(dirname "$issue_json")/dor-lint.json")" = "trivial" ]
}
