#!/bin/sh
# Stub `claude` REPL for the interactive PTY integration test (issue #749).
#
# Mimics the observable contract of `claude --session-id <uuid>`:
#  - writes a `mode` line to $HOME/.claude/projects/<munged-pwd>/<id>.jsonl
#    at boot (the readiness marker the backend polls for);
#  - after receiving one turn on stdin, appends an assistant entry and the
#    `turn_duration` end-of-turn marker;
#  - exits 0 when a line containing `/exit` arrives.
#
# The PTY translates the submitted `\r` to `\n` (ICRNL), so plain `read`
# works line-by-line.

SESSION_ID=""
prev=""
for a in "$@"; do
  if [ "$prev" = "--session-id" ]; then
    SESSION_ID="$a"
  fi
  prev="$a"
done

DIR="$HOME/.claude/projects/$(pwd | sed 's/[/.]/-/g')"
mkdir -p "$DIR"
T="$DIR/$SESSION_ID.jsonl"

printf '%s\n' '{"type":"mode","mode":"normal","sessionId":"stub"}' >>"$T"

IFS= read -r _turn || exit 1

printf '%s\n' '{"type":"assistant","message":{"id":"m1","type":"message","role":"assistant","model":"claude-haiku-4-5-20251001","content":[{"type":"text","text":"hello from stub"}],"stop_reason":"end_turn","usage":{"input_tokens":7,"cache_creation_input_tokens":0,"cache_read_input_tokens":100,"output_tokens":4}}}' >>"$T"
printf '%s\n' '{"type":"system","subtype":"turn_duration","durationMs":5,"messageCount":2}' >>"$T"

while IFS= read -r line; do
  case "$line" in
  *"/exit"*) exit 0 ;;
  esac
done
exit 0
