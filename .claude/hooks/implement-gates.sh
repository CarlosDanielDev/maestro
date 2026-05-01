#!/usr/bin/env bash
# Pre-check hook for /implement.
# Argument: $1 = issue number.
#
# Optional flags (parsed after the issue number):
#   --dirty-tree-action=<stash|abort|ask>
#       stash : auto-stash uncommitted changes and continue
#       abort : exit 6 immediately on dirty tree
#       ask   : interactive prompt (legacy; only safe with a real TTY)
#       Default: ask if stdin is a TTY, otherwise abort with a clear message.

set -euo pipefail

issue_number="${1:-}"
shift || true

dirty_tree_action="auto"

while [ $# -gt 0 ]; do
  case "$1" in
    --dirty-tree-action=stash) dirty_tree_action="stash" ;;
    --dirty-tree-action=abort) dirty_tree_action="abort" ;;
    --dirty-tree-action=ask)   dirty_tree_action="ask" ;;
    --dirty-tree-action=*)
      echo "implement-gates: unknown --dirty-tree-action value: ${1#*=}" >&2
      exit 1
      ;;
    *)
      echo "implement-gates: unknown argument: $1" >&2
      exit 1
      ;;
  esac
  shift
done

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

# Gate 6: working tree must be clean, or caller must specify how to handle it.
if [ -n "$(git status --porcelain)" ]; then
  echo "implement-gates: Working tree has uncommitted changes"
  git status --short
  echo ""

  resolved_action="$dirty_tree_action"
  if [ "$resolved_action" = "auto" ]; then
    if [ -t 0 ]; then
      resolved_action="ask"
    else
      echo "implement-gates: Dirty tree detected and stdin is not a TTY." >&2
      echo "implement-gates: Pass --dirty-tree-action=stash to auto-stash, or" >&2
      echo "implement-gates: --dirty-tree-action=abort to fail fast." >&2
      echo "implement-gates: aborting on dirty tree (no TTY, no flag)" >&2
      exit 6
    fi
  fi

  surface_stash_list() {
    # CI loops can pile up auto-stashes invisibly; surface the top of
    # the list so the user sees them accumulating (#545 P3).
    local count
    count=$(git stash list | wc -l | tr -d ' ')
    echo "implement-gates: most recent stashes (top 5 of ${count}):"
    git stash list | head -5
  }

  case "$resolved_action" in
    stash)
      git stash push -m "auto-stash before /implement #${issue_number}"
      echo "implement-gates: stashed as 'auto-stash before /implement #${issue_number}'"
      surface_stash_list
      ;;
    abort)
      echo "implement-gates: aborting on dirty tree (--dirty-tree-action=abort)"
      exit 6
      ;;
    ask)
      if [ ! -t 0 ]; then
        echo "implement-gates: --dirty-tree-action=ask requires a TTY; stdin is not interactive." >&2
        exit 6
      fi
      echo "(S)tash and continue, (A)bort"
      read -r choice
      case "$choice" in
        S|s)
          git stash push -m "auto-stash before /implement #${issue_number}"
          echo "implement-gates: stashed as 'auto-stash before /implement #${issue_number}'"
          surface_stash_list
          ;;
        *)
          echo "implement-gates: aborting on dirty tree"
          exit 6
          ;;
      esac
      ;;
  esac
fi

# Gate 7: baseline cargo test must be green.
if ! cargo test --quiet > "$GATE_LOG_DIR/baseline.log" 2>&1; then
  echo "implement-gates: BASELINE NOT GREEN — existing tests are failing before /implement ran." >&2
  echo "implement-gates: The RED gate would pass for the wrong reason. Fix baseline first." >&2
  echo "implement-gates: See $GATE_LOG_DIR/baseline.log" >&2
  exit 2
fi

# Gate 8 (optional): preflight bridge.
if [ -x .claude/hooks/preflight.sh ]; then
  set +e
  bash .claude/hooks/preflight.sh
  preflight_exit=$?
  set -e
  if [ $preflight_exit -ne 0 ]; then
    echo "implement-gates: Pre-flight CI checks failed. Fix before starting a new branch." >&2
    exit $preflight_exit
  fi
fi

# Sentinel: persist GATE_LOG_DIR so subsequent Bash tool calls can recover it
# without re-exporting (each Bash call is a fresh shell). Overwritten on next
# run. Path resolved by sentinel-path.sh into $XDG_RUNTIME_DIR or
# $HOME/.cache/maestro to avoid the /tmp symlink-attack vector on multi-user
# Linux (#545 P3). The legacy /tmp/maestro-current-gate-dir is also written
# for back-compat with consumers (e.g. /implement Step 2) that have not yet
# learned to walk the new resolution chain.
# shellcheck disable=SC1091
source "$(dirname "$0")/sentinel-path.sh"
echo -n "$GATE_LOG_DIR" > "$SENTINEL_PATH"
echo -n "$GATE_LOG_DIR" > /tmp/maestro-current-gate-dir
echo "sentinel: $SENTINEL_PATH"
