#!/usr/bin/env bash
# Test fixture: PATH-shim for `gh` CLI.
#
# Environment variables consumed:
#   FAKE_GH_AUTH_STATUS   — "authed" (default) or "unauthed"
#   FAKE_GH_ISSUE_STATE   — "OPEN" (default) or "CLOSED"
#   FAKE_GH_ISSUE_BODY    — body to return (default: minimal issue)
#   FAKE_GH_RETURN_CODE   — exit code for the subcommand (default: 0)
#
# Supports: gh auth status, gh issue view

set -euo pipefail

cmd="${1:-}"
sub="${2:-}"

if [ "$cmd" = "auth" ] && [ "$sub" = "status" ]; then
  if [ "${FAKE_GH_AUTH_STATUS:-authed}" = "authed" ]; then
    echo "github.com"
    echo "  ✓ Logged in to github.com as test-user"
    exit 0
  else
    echo "You are not logged into any GitHub hosts." >&2
    exit 1
  fi
fi

if [ "$cmd" = "issue" ] && [ "$sub" = "view" ]; then
  cat <<EOF
{
  "title": "test issue",
  "body": "${FAKE_GH_ISSUE_BODY:-## Overview\nTest\n## Expected Behavior\nTest\n## Acceptance Criteria\n- [ ] Test\n## Files to Modify\n- src/lib.rs\n## Test Hints\n- None\n## Blocked By\n- None\n## Definition of Done\n- [ ] Tests pass}",
  "labels": [{"name": "type:feature"}],
  "assignees": [],
  "milestone": null,
  "state": "${FAKE_GH_ISSUE_STATE:-OPEN}",
  "comments": []
}
EOF
  exit "${FAKE_GH_RETURN_CODE:-0}"
fi

echo "fake-gh: unsupported subcommand '$cmd $sub'" >&2
exit 2
