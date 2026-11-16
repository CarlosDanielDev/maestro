#!/usr/bin/env bash
#
# check-design-drift.sh
#
# Fails when a diff touches the TUI (`crates/maestro-tui/src/tui/`) and says
# nothing about design: neither `docs/tui-design-system.md` nor
# `docs/design-system/decisions.md` changed, and the PR body has no
# `Design: no change` line. See docs/design-system/README.md (#1238).
#
# Inputs:
#   DESIGN_DRIFT_BASE  base ref for `git diff <base>...HEAD` (default: origin/main)
#   PR_BODY            pull-request body (empty locally, so a local run only
#                      checks the doc-change half)
#
# Exit codes:
#   0 — no TUI change, or the design was addressed
#   1 — TUI changed with no doc change and no `Design: no change` line
#   2 — the diff could not be computed (unknown base ref, not a git repo)

set -euo pipefail

BASE="${DESIGN_DRIFT_BASE:-origin/main}"
TUI_PREFIX="crates/maestro-tui/src/tui/"

if ! changed="$(git diff --name-only "$BASE...HEAD" 2>&1)"; then
  echo "ERROR: cannot diff $BASE...HEAD: $changed" >&2
  exit 2
fi

if ! grep -q "^$TUI_PREFIX" <<<"$changed"; then
  echo "design drift check: OK (no change under $TUI_PREFIX)"
  exit 0
fi

if grep -qxE 'docs/tui-design-system\.md|docs/design-system/decisions\.md' <<<"$changed"; then
  echo "design drift check: OK (design docs updated)"
  exit 0
fi

if grep -qE '^[[:space:]]*Design: no change[[:space:]]*$' <<<"${PR_BODY:-}"; then
  echo "design drift check: OK (PR body says 'Design: no change')"
  exit 0
fi

echo "DESIGN-DRIFT: this diff changes $TUI_PREFIX but not the design docs." >&2
echo "Do one of these in the same PR:" >&2
echo "  - update docs/tui-design-system.md for anything new (widget, token, key)" >&2
echo "  - add an entry to docs/design-system/decisions.md for a deviation or learning" >&2
echo "  - add a line 'Design: no change' to the PR body" >&2
exit 1
