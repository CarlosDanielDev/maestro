#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "${MAESTRO_MOCK_CODEX_VERSION:-codex 1.2.3}"
  exit 0
fi

if [ -n "$MAESTRO_MOCK_CODEX_ARGV" ]; then
  printf '%s\n' "$@" > "$MAESTRO_MOCK_CODEX_ARGV"
fi

if [ -n "$MAESTRO_MOCK_CODEX_CWD" ]; then
  pwd > "$MAESTRO_MOCK_CODEX_CWD"
fi

if [ -n "$MAESTRO_MOCK_CODEX_STDERR" ]; then
  printf '%s\n' "$MAESTRO_MOCK_CODEX_STDERR" >&2
fi

if [ -n "$MAESTRO_MOCK_CODEX_STDOUT_FILE" ]; then
  cat "$MAESTRO_MOCK_CODEX_STDOUT_FILE"
fi

if [ -n "$MAESTRO_MOCK_CODEX_EXIT" ]; then
  exit "$MAESTRO_MOCK_CODEX_EXIT"
fi

exit 0
