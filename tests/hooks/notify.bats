#!/usr/bin/env bats
#
# Tests for .claude/hooks/notify.sh
#
# Covers the sanitization fix from issue #583:
#   - Slack JSON payload built via jq --arg (no heredoc interpolation).
#   - PowerShell title/message single-quote escape and newline strip.
#   - macOS tr-based sanitization regression guard.
#   - shellcheck static-analysis gate.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HOOK="$REPO_ROOT/.claude/hooks/notify.sh"

  # Sourcing the script only loads helpers; main is gated behind a
  # sourced-vs-executed check inside the hook itself.
  # shellcheck source=/dev/null
  source "$HOOK"

  FAKE_BIN="$(mktemp -d)"
  export FAKE_BIN
}

teardown() {
  rm -rf "$FAKE_BIN"
}

# ---------------------------------------------------------------------------
# Slack payload tests
# ---------------------------------------------------------------------------

@test "slack payload is valid JSON for benign input" {
  run build_slack_payload "U123" "Hello world"
  [ "$status" -eq 0 ]
  echo "$output" | jq -e . > /dev/null
}

@test "slack payload preserves text verbatim under JSON injection attempt" {
  local channel="U123"
  local text='test", "channel": "#hijack'

  run build_slack_payload "$channel" "$text"
  [ "$status" -eq 0 ]

  echo "$output" | jq -e . > /dev/null

  parsed_text="$(echo "$output" | jq -r .text)"
  [ "$parsed_text" = "$text" ]

  parsed_channel="$(echo "$output" | jq -r .channel)"
  [ "$parsed_channel" = "$channel" ]
}

@test "slack payload handles backslash and embedded newline" {
  local text
  text=$'path\\to\\file\nline2'

  run build_slack_payload "U999" "$text"
  [ "$status" -eq 0 ]
  echo "$output" | jq -e . > /dev/null

  parsed_text="$(echo "$output" | jq -r .text)"
  [ "$parsed_text" = "$text" ]
}

# ---------------------------------------------------------------------------
# PowerShell escape tests
# ---------------------------------------------------------------------------

@test "escape_powershell_string strips newlines" {
  local input
  input=$'title with\nnewline'

  run escape_powershell_string "$input"
  [ "$status" -eq 0 ]
  [[ "$output" != *$'\n'* ]]
}

@test "escape_powershell_string strips carriage returns" {
  local input
  input=$'title with\rCR'

  run escape_powershell_string "$input"
  [ "$status" -eq 0 ]
  [[ "$output" != *$'\r'* ]]
}

@test "escape_powershell_string doubles single quotes" {
  local input="it's a \"test\""

  run escape_powershell_string "$input"
  [ "$status" -eq 0 ]
  [[ "$output" == *"''"* ]]
}

@test "escape_powershell_string handles combined quote and newline injection" {
  local input
  input=$'it\'s a "test"\nrm -rf /'

  run escape_powershell_string "$input"
  [ "$status" -eq 0 ]

  [[ "$output" != *$'\n'* ]]
  [[ "$output" == *"''"* ]]
  [[ "$output" == *"rm -rf /"* ]]
}

@test "escape_powershell_string preserves empty input" {
  run escape_powershell_string ""
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

# ---------------------------------------------------------------------------
# macOS regression test
# ---------------------------------------------------------------------------

@test "macos sanitization strips special chars (regression)" {
  if [[ "$OSTYPE" != "darwin"* ]]; then
    skip "macOS only"
  fi

  cat > "$FAKE_BIN/osascript" <<'STUB'
#!/bin/bash
echo "$@" >> "$FAKE_BIN/osascript.log"
STUB
  chmod +x "$FAKE_BIN/osascript"
  PATH="$FAKE_BIN:$PATH"
  export PATH FAKE_BIN

  run send_macos_notification "it's broken!" "rm -rf /"
  [ "$status" -eq 0 ]

  [ -f "$FAKE_BIN/osascript.log" ]
  log_content="$(cat "$FAKE_BIN/osascript.log")"

  [[ "$log_content" != *"it's"* ]]
  [[ "$log_content" != *"!"* ]]
}

# ---------------------------------------------------------------------------
# shellcheck gate
# ---------------------------------------------------------------------------

@test "shellcheck passes on notify.sh with no warnings" {
  if ! command -v shellcheck &> /dev/null; then
    skip "shellcheck not installed"
  fi
  run shellcheck -x "$HOOK"
  [ "$status" -eq 0 ]
}
