#!/usr/bin/env bash
# Pre-check hook for /implement.
# Argument: $1 = issue number.
# Outputs in $GATE_LOG_DIR:
#   issue.json        — full gh issue payload fetched in Gate 4
#   issue-summary.md  — condensed DOR-section summary for downstream agents
#   dor-lint.json     — created later by /implement Step 4 fast-path lint
#
# Optional flags (parsed after the issue number):
#   --dirty-tree-action=<stash|abort|ask>
#       stash : auto-stash uncommitted changes and continue
#       abort : exit 6 immediately on dirty tree
#       ask   : interactive prompt (legacy; only safe with a real TTY)
#       Default: ask if stdin is a TTY, otherwise abort with a clear message.

set -euo pipefail

hook_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$hook_dir/../.." && pwd)"

issue_number="${1:-}"
shift || true

dirty_tree_action="auto"
include_untracked=0

while [ $# -gt 0 ]; do
  case "$1" in
    --dirty-tree-action=stash) dirty_tree_action="stash" ;;
    --dirty-tree-action=abort) dirty_tree_action="abort" ;;
    --dirty-tree-action=ask)   dirty_tree_action="ask" ;;
    --dirty-tree-action=*)
      echo "implement-gates: unknown --dirty-tree-action value: ${1#*=}" >&2
      exit 1
      ;;
    --include-untracked)    include_untracked=1 ;;
    --no-include-untracked) include_untracked=0 ;;
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

if ! "$repo_root/scripts/condense-issue.sh" "$GATE_LOG_DIR/issue.json" \
  > "$GATE_LOG_DIR/issue-summary.md"; then
  echo "implement-gates: failed to condense issue #${issue_number}" >&2
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
    # the list so the user sees them accumulating.
    local list count
    list=$(git stash list)
    count=$(printf '%s\n' "$list" | grep -c '^stash@' || true)
    echo "implement-gates: most recent stashes (top 5 of ${count}):"
    printf '%s\n' "$list" | head -5
  }

  do_stash() {
    # G2: honest stash. git stash push exits 0 even on "No local changes to
    # save" (untracked-only working tree). Capture its output, branch on the
    # real result, and tell the truth.
    local stash_args=("push" "-m" "auto-stash before /implement #${issue_number}")
    if [ "$include_untracked" = "1" ]; then
      stash_args=("push" "-u" "-m" "auto-stash before /implement #${issue_number} (untracked included)")
    fi

    local stash_output
    stash_output=$(git stash "${stash_args[@]}" 2>&1) || {
      echo "implement-gates: git stash failed:" >&2
      printf '%s\n' "$stash_output" >&2
      exit 6
    }

    if printf '%s' "$stash_output" | grep -q "No local changes to save"; then
      local untracked
      untracked=$(git status --porcelain | grep '^??' | cut -c4-)
      echo "implement-gates: WARN: auto-stash skipped — no tracked changes to stash."
      if [ -n "$untracked" ]; then
        echo "implement-gates: untracked files retained on disk:"
        printf '%s\n' "$untracked" | sed 's/^/  - /'
        if [ "$include_untracked" = "0" ]; then
          echo "implement-gates: pass --include-untracked to stash these too (default keeps them in tree)."
        fi
      fi
    else
      printf '%s\n' "$stash_output" | sed 's/^/implement-gates: /'
      surface_stash_list
    fi
  }

  case "$resolved_action" in
    stash)
      do_stash
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
          do_stash
          ;;
        *)
          echo "implement-gates: aborting on dirty tree"
          exit 6
          ;;
      esac
      ;;
  esac
fi

# Gate 6.5: CHANGELOG-induced snapshot drift.
#
# The landing screen embeds CHANGELOG.md via include_str! and renders trend
# bars from .entries.iter().take(24). Any edit to CHANGELOG.md can shift the
# landing_welcome_* snapshots. Gate 7 would catch this with a generic
# baseline failure; this gate surfaces it sooner with an actionable message.
# Scope: any modification to CHANGELOG.md in the diff between HEAD and main.
# Exit 0 silently when CHANGELOG.md is untouched (no false positives).
# shellcheck source=./lib-changelog.sh
. "$(dirname "$0")/lib-changelog.sh"
changelog_diff_base="$(resolve_changelog_diff_base)"
if [ -n "$changelog_diff_base" ]; then
  if changelog_changed_vs_base "$changelog_diff_base"; then
    echo "implement-gates: CHANGELOG.md modified vs ${changelog_diff_base} — verifying landing snapshots"
    if ! cargo test --lib tui::snapshot_tests::landing -- --include-ignored \
         > "$GATE_LOG_DIR/landing-snapshot.log" 2>&1; then
      echo "implement-gates: snapshot drift detected — run cargo insta accept or revert CHANGELOG" >&2
      echo "implement-gates: See $GATE_LOG_DIR/landing-snapshot.log" >&2
      exit 3
    fi
    echo "implement-gates: landing snapshots clean"
  fi
else
  echo "implement-gates: WARN: neither main nor origin/main is reachable — skipping CHANGELOG drift gate" >&2
fi

# Gate 7: baseline cargo test must be green.
if ! cargo test --quiet > "$GATE_LOG_DIR/baseline.log" 2>&1; then
  echo "implement-gates: BASELINE NOT GREEN — existing tests are failing before /implement ran." >&2
  echo "implement-gates: The RED gate would pass for the wrong reason. Fix baseline first." >&2
  echo "implement-gates: See $GATE_LOG_DIR/baseline.log" >&2
  exit 2
fi

# Gate 8 (optional): preflight bridge.
if [ -x .maestro/hooks/preflight.sh ]; then
  set +e
  bash .maestro/hooks/preflight.sh
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
# Linux. /implement Step 2 walks the same resolution chain on read.
# shellcheck disable=SC1091
source "$hook_dir/sentinel-path.sh"
echo -n "$GATE_LOG_DIR" > "$SENTINEL_PATH"
echo "sentinel: $SENTINEL_PATH"
