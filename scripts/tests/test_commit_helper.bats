#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/commit-helper.sh"
  FIXTURES="$REPO_ROOT/scripts/tests/fixtures"

  SHIM_DIR="$(mktemp -d)"
  cat > "$SHIM_DIR/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

fixture_dir="${COMMIT_HELPER_FIXTURES:?}"

if [ "${1:-}" = "issue" ] && [ "${2:-}" = "view" ] && [ "${4:-}" = "--json" ] && [ "${5:-}" = "labels" ]; then
  case "${3:-}" in
    101) cat "$fixture_dir/labels-bug.json" ;;
    102) printf '%s\n' '{"labels":[{"name":"security"}]}' ;;
    103) printf '%s\n' '{"labels":[{"name":"tech-debt"}]}' ;;
    104) printf '%s\n' '{"labels":[{"name":"refactor"}]}' ;;
    105) printf '%s\n' '{"labels":[{"name":"testing"}]}' ;;
    106) printf '%s\n' '{"labels":[{"name":"documentation"}]}' ;;
    107) printf '%s\n' '{"labels":[{"name":"type:docs"}]}' ;;
    108) printf '%s\n' '{"labels":[{"name":"enhancement"}]}' ;;
    109) printf '%s\n' '{"labels":[{"name":"type:feature"}]}' ;;
    110) cat "$fixture_dir/labels-empty.json" ;;
    111) cat "$fixture_dir/labels-multi-bug-feat.json" ;;
    112) cat "$fixture_dir/labels-enhancement-tech-debt.json" ;;
    404) echo "issue not found" >&2; exit 1 ;;
    *) echo "unexpected issue ${3:-}" >&2; exit 2 ;;
  esac
  exit 0
fi

echo "fake gh: unsupported command $*" >&2
exit 2
EOF
  chmod +x "$SHIM_DIR/gh"
  export COMMIT_HELPER_FIXTURES="$FIXTURES"
  PATH="$SHIM_DIR:$PATH"
  export PATH
}

teardown() {
  rm -rf "$SHIM_DIR" "${GATE_LOG_DIR:-}"
}

assert_commit_draft() {
  issue="$1"
  prefix="$2"

  run "$SCRIPT" "$issue"
  [ "$status" -eq 0 ]
  [ "$output" = "$(printf '%s: <PLACEHOLDER subject — fill me>\n\nCloses #%s' "$prefix" "$issue")" ]
}

@test "bug label maps to fix" {
  assert_commit_draft 101 fix
}

@test "security label maps to fix" {
  assert_commit_draft 102 fix
}

@test "tech-debt label maps to refactor" {
  assert_commit_draft 103 refactor
}

@test "refactor label maps to refactor" {
  assert_commit_draft 104 refactor
}

@test "testing label maps to test" {
  assert_commit_draft 105 test
}

@test "documentation label maps to docs" {
  assert_commit_draft 106 docs
}

@test "type docs label maps to docs" {
  assert_commit_draft 107 docs
}

@test "enhancement label maps to feat" {
  assert_commit_draft 108 feat
}

@test "type feature label maps to feat" {
  assert_commit_draft 109 feat
}

@test "no matching labels maps to chore" {
  assert_commit_draft 110 chore
}

@test "bug beats feature in multi-label precedence" {
  assert_commit_draft 111 fix
}

@test "tech-debt beats enhancement in multi-label precedence" {
  assert_commit_draft 112 refactor
}

@test "writes commit draft when GATE_LOG_DIR is set" {
  GATE_LOG_DIR="$(mktemp -d)"
  export GATE_LOG_DIR

  run "$SCRIPT" 108
  [ "$status" -eq 0 ]
  [ -f "$GATE_LOG_DIR/commit-draft.txt" ]
  diff -u <(printf '%s\n' "$output") "$GATE_LOG_DIR/commit-draft.txt"
}

@test "missing issue exits non-zero with stderr message" {
  run "$SCRIPT" 404
  [ "$status" -ne 0 ]
  [[ "$output" == *"commit-helper: failed to fetch issue #404"* ]]
}

@test "missing gh exits non-zero with stderr message" {
  run env PATH="/usr/bin:/bin" "$SCRIPT" 101
  [ "$status" -ne 0 ]
  [[ "$output" == *"commit-helper: gh CLI is required"* ]]
}
