#!/bin/sh
# Stub agent for the E2E tests. Emits the stream-json lines a Claude-style
# headless run produces, so maestro can complete a session without a real agent.
#
# Success path only: this stub always exits 0 and emits a success result. The app marks
# sessions `completed` even for failing stubs, so do not read status as proof of success.
# The `--version` branch is not exercised by e2e_run_once.rs (whose `--skip-doctor`
# skips the health check that would call it); tests/e2e_aimem_shim.rs drives it
# through an ai-memory shim, where maestro's doctor runs `<command> --version`.
#
# Interactive mode (#1112): when the caller runs the genuine REPL shape —
# a pinned `--session-id` with no `--print` (maestro's PTY transport) — the
# stub acts as a one-turn REPL like testdata/claude-transcript/stub-claude.sh:
# it writes the boot marker and the turn entries to the transcript maestro
# tails, then keeps reading stdin until `/exit`, so the child stays parked
# like the real claude REPL. Everything else stays headless. `--session-id`
# alone is NOT enough: ai-memory (observed 3.2.2) pins a session id onto
# managed launches even for `--print` runs, and in real claude `--print` is
# headless one-shot regardless of a pinned id.
if [ "$1" = "--version" ]; then
  echo "${MAESTRO_STUB_AGENT_VERSION:-stub-agent 1.0.0}"
  exit 0
fi

SESSION_ID=""
HAS_PRINT=""
prev=""
for a in "$@"; do
  if [ "$prev" = "--session-id" ]; then
    SESSION_ID="$a"
  fi
  case "$a" in
  --print | --print=*) HAS_PRINT=1 ;;
  esac
  prev="$a"
done

if [ -n "$SESSION_ID" ] && [ -z "$HAS_PRINT" ]; then
  DIR="$HOME/.claude/projects/$(pwd | sed 's/[/.]/-/g')"
  mkdir -p "$DIR"
  T="$DIR/$SESSION_ID.jsonl"

  printf '%s\n' '{"type":"mode","mode":"normal","sessionId":"stub"}' >>"$T"

  IFS= read -r _turn || exit 1

  printf '%s\n' '{"type":"assistant","message":{"id":"m1","type":"message","role":"assistant","model":"claude-haiku-4-5-20251001","content":[{"type":"text","text":"stub reply"}],"stop_reason":"end_turn","usage":{"input_tokens":7,"cache_creation_input_tokens":0,"cache_read_input_tokens":100,"output_tokens":4}}}' >>"$T"
  printf '%s\n' '{"type":"system","subtype":"turn_duration","durationMs":5,"messageCount":2}' >>"$T"

  while IFS= read -r line; do
    case "$line" in
    *"/exit"*) exit 0 ;;
    esac
  done
  exit 0
fi

echo '{"type":"system","subtype":"init","session_id":"e2e00000-0000-0000-0000-000000000001"}'
echo '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"stub reply"}]}}'
echo '{"type":"result","subtype":"success","result":"stub reply","session_id":"e2e00000-0000-0000-0000-000000000001"}'
exit 0
