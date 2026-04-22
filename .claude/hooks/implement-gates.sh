#!/usr/bin/env bash
# Pre-check hook for /implement.
# Argument: $1 = issue number.

set -euo pipefail

issue_number="${1:-}"

if [ -z "$issue_number" ]; then
  echo "implement-gates: issue number required as first argument" >&2
  exit 1
fi

# Gate 1: must be inside a git repo.
if ! git rev-parse --git-dir >/dev/null 2>&1; then
  echo "implement-gates: not inside a git repository" >&2
  exit 1
fi

# Gate 2: gh CLI must be installed.
if ! command -v gh >/dev/null 2>&1; then
  echo "implement-gates: gh CLI not installed. Install: brew install gh" >&2
  exit 1
fi
