#!/usr/bin/env bash
# Sourced helper. Sets $SENTINEL_PATH to the canonical
# maestro-current-gate-dir sentinel location.
#
# Resolution order (symlink-attack hardening on multi-user Linux):
#   1. $XDG_RUNTIME_DIR        (Linux systemd/elogind; tmpfs, mode 0700)
#   2. $HOME/.cache/maestro    (Linux + macOS portable fallback)
#   3. ${TMPDIR:-/tmp}         (last resort; macOS pre-XDG, oddly-configured)
#
# The chosen directory is created with mkdir -p when missing. Output is
# silent unless mkdir fails.

if [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -d "$XDG_RUNTIME_DIR" ]; then
  SENTINEL_DIR="$XDG_RUNTIME_DIR"
elif [ -n "${HOME:-}" ]; then
  SENTINEL_DIR="$HOME/.cache/maestro"
  mkdir -p "$SENTINEL_DIR" 2>/dev/null || SENTINEL_DIR="${TMPDIR:-/tmp}"
else
  SENTINEL_DIR="${TMPDIR:-/tmp}"
fi

SENTINEL_PATH="$SENTINEL_DIR/maestro-current-gate-dir"
export SENTINEL_PATH
