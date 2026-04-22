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

# Gate 3: gh must be authenticated.
if ! gh auth status >/dev/null 2>&1; then
  echo "implement-gates: gh not authenticated. Run: gh auth login" >&2
  exit 1
fi

# Gate 4: fetch and cache the issue JSON.
GATE_LOG_DIR="/tmp/maestro-${issue_number}-$(date +%s)"
mkdir -p "$GATE_LOG_DIR"
echo "gate log dir: $GATE_LOG_DIR"

if ! gh issue view "$issue_number" \
  --json title,body,labels,assignees,milestone,state,comments \
  > "$GATE_LOG_DIR/issue.json" 2>"$GATE_LOG_DIR/gh-error.log"; then
  echo "implement-gates: failed to fetch issue #${issue_number}" >&2
  cat "$GATE_LOG_DIR/gh-error.log" >&2
  exit 1
fi

export GATE_LOG_DIR

# Gate 5: issue must not be CLOSED.
issue_state=$(python3 -c "import json; print(json.load(open('$GATE_LOG_DIR/issue.json'))['state'])")
if [ "$issue_state" = "CLOSED" ]; then
  echo "implement-gates: Issue #${issue_number} is CLOSED. Re-open or pick a different issue." >&2
  exit 1
fi

# Gate 6: working tree must be clean, or user must confirm stash.
if [ -n "$(git status --porcelain)" ]; then
  echo "implement-gates: Working tree has uncommitted changes"
  git status --short
  echo ""
  echo "(S)tash and continue, (A)bort"
  read -r choice
  case "$choice" in
    S|s)
      git stash push -m "auto-stash before /implement #${issue_number}"
      echo "implement-gates: stashed as 'auto-stash before /implement #${issue_number}'"
      ;;
    *)
      echo "implement-gates: aborting on dirty tree"
      exit 6
      ;;
  esac
fi
