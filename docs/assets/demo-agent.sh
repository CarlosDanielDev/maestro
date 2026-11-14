#!/bin/sh
# Scripted stand-in for `claude` in the demo capture (docs/assets/capture.sh).
# Emits the headless stream-json a real bug-fix run produces — reads, a
# failing repro, an edit, a green test run, a result with cost — paced with
# sleeps so the recording shows sessions working. It edits nothing.
#
# The scenario is picked from the prompt text (#1155 / #1154 / #1153 / #1157).
if [ "$1" = "--version" ]; then
  echo "2.1.0 (Claude Code)"
  exit 0
fi

case "$*" in
*1155*) S=a ;;
*1154*) S=b ;;
*1153*) S=c ;;
*) S=d ;;
esac

ID="d3m0000$S-0000-4000-8000-00000000000$S"
emit() { printf '%s\n' "$1"; }
say() {
  emit "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-5-5\",\"content\":[{\"type\":\"text\",\"text\":\"$1\"}],\"usage\":{\"input_tokens\":$2,\"output_tokens\":$3,\"cache_read_input_tokens\":$4}}}"
}
tool() {
  emit "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-5-5\",\"content\":[{\"type\":\"tool_use\",\"name\":\"$1\",\"input\":{$2}}]}}"
}
ok() { emit "{\"type\":\"tool_result\",\"tool_name\":\"$1\",\"is_error\":${2:-false}}"; }
file() { tool "$1" "\"file_path\":\"$2\""; sleep "$3"; ok "$1"; }
run() { tool Bash "\"command\":\"$1\""; sleep "$2"; ok Bash "${3:-false}"; }

emit "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$ID\",\"model\":\"claude-opus-5-5\"}"

case "$S" in
a) # #1155 unknown model priced as $0.00
  say "Reproducing: an unknown model shows \$0.00." 8200 140 41000; sleep 2
  file Read src/agent_provider/claude_pricing.rs 2
  file Read src/session/parser.rs 2
  run "cargo test claude_pricing::unknown_model" 3 true
  say "Red. Unknown models fall back to a zero rate." 21000 380 64000; sleep 2
  file Edit src/agent_provider/claude_pricing.rs 3
  file Edit src/commands/status.rs 2
  run "cargo test claude_pricing::" 4
  say "Green. Unknown cost now shows as \$?." 30500 520 88000; sleep 1
  COST=1.18 ;;
b) # #1154 transient network error aborts run
  say "Reproducing: one network blip aborts maestro run." 7600 120 39000; sleep 3
  file Read src/provider/github/transport/cli.rs 2
  file Read src/provider/github/transport/errors.rs 3
  run "cargo test transport::retries_transient" 3 true
  say "Red. Only rate limits hit the retry path." 19800 410 61000; sleep 2
  file Edit src/provider/github/transport/errors.rs 4
  run "cargo test transport::" 4
  file Edit CHANGELOG.md 2
  say "Green. Transient errors retry with backoff." 33100 610 97000; sleep 2
  COST=1.42 ;;
c) # #1153 TUI freezes while a session pushes
  say "Reproducing: the TUI freezes while a session finishes." 9100 150 45000; sleep 2
  file Read src/commands/run.rs 3
  file Read src/git.rs 2
  run "cargo test run::completion_off_render_loop" 4 true
  say "Red. The completion step blocks the render loop." 24400 450 70000; sleep 3
  file Edit src/commands/run.rs 3
  run "cargo test run::" 3
  say "Green. Completion runs off the render loop." 36800 700 104000; sleep 2
  COST=1.67 ;;
*) # #1157 home panes cut labels at 60x15
  say "Reproducing: labels cut mid-word at 60x15." 6900 110 36000; sleep 2
  file Read src/tui/screens/home/draw.rs 2
  run "cargo insta test -- home_60x15" 3 true
  file Edit src/tui/screens/home/draw.rs 3
  run "cargo insta test -- home_" 3
  say "Green. Log row kept, Quick Actions ellipsized." 18200 330 58000; sleep 1
  COST=0.74 ;;
esac

emit "{\"type\":\"result\",\"subtype\":\"success\",\"result\":\"Fixed and tested.\",\"session_id\":\"$ID\",\"cost_usd\":$COST,\"usage\":{\"input_tokens\":4200,\"output_tokens\":900,\"max_input_tokens\":200000}}"
exit 0
