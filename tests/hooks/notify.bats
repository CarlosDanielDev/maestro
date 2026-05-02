#!/usr/bin/env bats
#
# Tests for .claude/hooks/notify.sh
#
# Covers the sanitization fix from issue #583:
#   - Slack JSON payload built via jq --arg (no heredoc interpolation).
#   - PowerShell title/message single-quote escape and newline strip.
#   - macOS tr-based sanitization regression guard.
#   - Linux notify-send Pango markup escape and option separator.
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
# JSON parsing tests
# ---------------------------------------------------------------------------

@test "extract_json_field decodes JSON escapes" {
  local json='{"tool_name":"Bash \"quoted\" path\\to\\file \u00e9"}'
  local expected
  expected="$(printf 'Bash "quoted" path\\to\\file \303\251')"

  run extract_json_field "$json" "tool_name"
  [ "$status" -eq 0 ]
  [ "$output" = "$expected" ]
}

@test "extract_message_field decodes JSON escapes and embedded newline" {
  local json='{"message":"line1\nline2 \"quoted\" path\\to\\file \u263a"}'
  local expected
  expected="$(printf 'line1\nline2 "quoted" path\\to\\file \342\230\272')"

  run extract_message_field "$json"
  [ "$status" -eq 0 ]
  [ "$output" = "$expected" ]
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
# Linux regression test
# ---------------------------------------------------------------------------

@test "linux notification escapes markup and terminates options (regression)" {
  cat > "$FAKE_BIN/notify-send" <<'STUB'
#!/bin/bash
{
  printf '%s\n' "$#"
  for arg in "$@"; do
    printf '<%s>\n' "$arg"
  done
} > "$FAKE_BIN/notify-send.log"
STUB
  chmod +x "$FAKE_BIN/notify-send"
  PATH="$FAKE_BIN:$PATH"
  export PATH FAKE_BIN

  local title="--icon=/etc/passwd & <bad>"
  local message='<a href="https://evil/">click</a> &amp; >'

  run send_linux_notification "$title" "$message"
  [ "$status" -eq 0 ]

  [ -f "$FAKE_BIN/notify-send.log" ]
  argv_count="$(sed -n '1p' "$FAKE_BIN/notify-send.log")"
  argv_separator="$(sed -n '2p' "$FAKE_BIN/notify-send.log")"
  argv_title="$(sed -n '3p' "$FAKE_BIN/notify-send.log")"
  argv_message="$(sed -n '4p' "$FAKE_BIN/notify-send.log")"

  [ "$argv_count" = "3" ]
  [ "$argv_separator" = "<-->" ]
  [ "$argv_title" = "<--icon=/etc/passwd &amp; &lt;bad&gt;>" ]
  [ "$argv_message" = '<&lt;a href="https://evil/"&gt;click&lt;/a&gt; &amp;amp; &gt;>' ]
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
