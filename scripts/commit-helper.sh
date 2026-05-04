#!/usr/bin/env bash
# Emit a draft Conventional Commit message for /pushup.
#
# Usage:
#   scripts/commit-helper.sh <issue-number>
#
# Label-to-prefix precedence, high to low; first match wins:
#   bug                         -> fix
#   security                    -> fix
#   tech-debt or refactor       -> refactor
#   testing                     -> test
#   documentation or type:docs  -> docs
#   enhancement or type:feature -> feat
#   default                     -> chore

set -euo pipefail

issue_number="${1:-}"

if [ -z "$issue_number" ]; then
  echo "commit-helper: issue number required" >&2
  exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "commit-helper: gh CLI is required" >&2
  exit 1
fi

gh_stderr="$(mktemp)"
if ! issue_json="$(gh issue view "$issue_number" --json labels 2>"$gh_stderr")"; then
  err="$(cat "$gh_stderr")"
  rm -f "$gh_stderr"
  if [ -n "$err" ]; then
    echo "commit-helper: failed to fetch issue #$issue_number: $err" >&2
  else
    echo "commit-helper: failed to fetch issue #$issue_number" >&2
  fi
  exit 1
fi
rm -f "$gh_stderr"

prefix="$(
  ISSUE_JSON="$issue_json" python3 - <<'PY'
import json
import os
import sys

try:
    issue = json.loads(os.environ["ISSUE_JSON"])
except json.JSONDecodeError:
    print("chore")
    raise SystemExit

labels = issue.get("labels") or []
names = set()
for label in labels:
    if isinstance(label, dict):
        name = label.get("name")
    else:
        name = str(label)
    if name:
        names.add(str(name).lower())

if "bug" in names:
    print("fix")
elif "security" in names:
    print("fix")
elif "tech-debt" in names or "refactor" in names:
    print("refactor")
elif "testing" in names:
    print("test")
elif "documentation" in names or "type:docs" in names:
    print("docs")
elif "enhancement" in names or "type:feature" in names:
    print("feat")
else:
    print("chore")
PY
)"

render_draft() {
  printf '%s: <PLACEHOLDER subject — fill me>\n\nCloses #%s\n' "$prefix" "$issue_number"
}

render_draft

if [ -n "${GATE_LOG_DIR:-}" ]; then
  mkdir -p "$GATE_LOG_DIR"
  render_draft > "$GATE_LOG_DIR/commit-draft.txt"
fi
