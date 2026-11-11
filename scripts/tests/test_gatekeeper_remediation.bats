#!/usr/bin/env bats
# Tests for scripts/gatekeeper-remediation.sh — the only code path that turns
# a gatekeeper report into `gh` side effects. The stub `gh` logs its argv, so
# every test can prove no unvalidated report string ever reaches it (#1038).

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/gatekeeper-remediation.sh"
  TEST_DIR="$(mktemp -d)"
  GH_LOG="$TEST_DIR/gh.log"

  SHIM_DIR="$(mktemp -d)"
  cat > "$SHIM_DIR/gh" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "$GH_LOG"
exit 0
EOF
  chmod +x "$SHIM_DIR/gh"
  PATH="$SHIM_DIR:$PATH"
  export PATH
}

teardown() {
  rm -rf "$SHIM_DIR" "$TEST_DIR"
}

# report <comment-body> [label...]
report() {
  local body="$1"
  shift
  local labels="[]"
  if [ "$#" -gt 0 ]; then
    labels="$(printf '%s\n' "$@" | jq -R . | jq -s .)"
  fi
  jq -n --arg body "$body" --argjson labels "$labels" \
    '{report_version: 1, status: "FAIL", task_type: "implementation",
      dor: {passed: false},
      remediation: {comment_body: $body, labels_to_add: $labels}}' \
    > "$TEST_DIR/report.json"
}

@test "posts comment body and NUL-safe label" {
  report "Please add the Overview section" "needs-info"
  run "$SCRIPT" 7 "$TEST_DIR/report.json"
  [ "$status" -eq 0 ]

  grep -Fx 'issue comment 7 --body Please add the Overview section' "$GH_LOG"
  grep -Fx 'issue edit 7 --add-label -- needs-info' "$GH_LOG"
}

@test "adds each label as a single argv element" {
  report "fix the DOR" "needs-info" "blocked"
  run "$SCRIPT" 7 "$TEST_DIR/report.json"
  [ "$status" -eq 0 ]

  grep -Fx 'issue edit 7 --add-label -- needs-info' "$GH_LOG"
  grep -Fx 'issue edit 7 --add-label -- blocked' "$GH_LOG"
  [ "$(grep -c 'add-label' "$GH_LOG")" -eq 2 ]
}

@test "never passes flag-like label to gh" {
  report "fix the DOR" "--add-assignee"
  run "$SCRIPT" 7 "$TEST_DIR/report.json"
  [ "$status" -ne 0 ]
  [ ! -e "$GH_LOG" ]
}

@test "rejects word-split injection string" {
  # Test hint from issue #1038: the whole string must never reach gh argv.
  report "fix the DOR" "needs-info --add-assignee evil"
  run "$SCRIPT" 7 "$TEST_DIR/report.json"
  [ "$status" -ne 0 ]
  [ ! -e "$GH_LOG" ]
}

@test "rejects label matching pattern but outside allowlist" {
  report "fix the DOR" "wip"
  run "$SCRIPT" 7 "$TEST_DIR/report.json"
  [ "$status" -ne 0 ]
  [ ! -e "$GH_LOG" ]
}

@test "rejects comment body over the length cap" {
  local body
  body="$(printf 'x%.0s' $(seq 1 10001))"
  report "$body"
  run "$SCRIPT" 7 "$TEST_DIR/report.json"
  [ "$status" -ne 0 ]
  [ ! -e "$GH_LOG" ]
}

@test "rejects empty comment body" {
  report ""
  run "$SCRIPT" 7 "$TEST_DIR/report.json"
  [ "$status" -ne 0 ]
  [ ! -e "$GH_LOG" ]
}

@test "fails closed on missing or empty report file" {
  run "$SCRIPT" 7 "$TEST_DIR/does-not-exist.json"
  [ "$status" -ne 0 ]
  [ ! -e "$GH_LOG" ]

  : > "$TEST_DIR/empty.json"
  run "$SCRIPT" 7 "$TEST_DIR/empty.json"
  [ "$status" -ne 0 ]
  [ ! -e "$GH_LOG" ]
}

@test "requires two arguments" {
  run "$SCRIPT" 7
  [ "$status" -ne 0 ]
  [ ! -e "$GH_LOG" ]
}
