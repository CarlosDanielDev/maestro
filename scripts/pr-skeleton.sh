#!/usr/bin/env bash
# Emit a draft Pull Request body for /pushup.
#
# Usage:
#   scripts/pr-skeleton.sh <issue-number> <commit-subject>
#
# The caller should pass the commit subject separately to `gh pr create --title`.
# This script computes that title input for validation only; the title is not
# embedded in the PR body.

set -euo pipefail

issue_number="${1:-}"
commit_subject="${2:-}"

if [ -z "$issue_number" ]; then
  echo "pr-skeleton: issue number required" >&2
  exit 1
fi

if [ -z "$commit_subject" ]; then
  echo "pr-skeleton: commit subject required" >&2
  exit 1
fi

title="$commit_subject"

body="$(cat <<EOF
## Summary

- <PLACEHOLDER bullet 1>
- <PLACEHOLDER bullet 2>
- <PLACEHOLDER bullet 3 — delete if not needed>

Closes #$issue_number

## Test plan

- [ ] cargo test --quiet
- [ ] cargo clippy -- -D warnings -A dead_code
- [ ] cargo fmt --check
- [ ] manual verification: <PLACEHOLDER>
EOF
)"

printf '%s\n' "$body"

if [ -n "${GATE_LOG_DIR:-}" ]; then
  mkdir -p "$GATE_LOG_DIR"
  printf '%s\n' "$body" > "$GATE_LOG_DIR/pr-draft.md"
fi

test -n "$title"
