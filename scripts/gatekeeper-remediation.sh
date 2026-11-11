#!/usr/bin/env bash
# Apply gatekeeper DOR remediation to a GitHub issue (the --auto-comment path).
#
# Consumes a parsed gatekeeper report (see
# .maestro/hooks/parse_gatekeeper_report.py) and performs the only two side
# effects the FAIL path is allowed: posting remediation.comment_body as an
# issue comment and adding remediation.labels_to_add as labels.
#
# Fails closed: nothing reaches `gh` until the comment body and every label
# pass validation here — independent of the parser's checks — so a crafted
# report can never turn report strings into gh flags or arbitrary labels
# (issue #1038, CWE-88).
#
# Usage: scripts/gatekeeper-remediation.sh <issue-number> <report.json>
# Exit codes:
#   0 — comment posted and labels applied
#   1 — validation failure (nothing posted)
#   2 — usage error

set -euo pipefail

COMMENT_BODY_MAX_LEN=10000
ALLOWED_LABELS=("needs-info" "blocked" "needs-contract")

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <issue-number> <gatekeeper-report.json>" >&2
  exit 2
fi

issue="$1"
report="$2"

if [ ! -s "$report" ]; then
  echo "gatekeeper-remediation: report missing or empty: $report" >&2
  exit 1
fi

comment_body="$(jq -r '.remediation.comment_body // ""' "$report")"

if [ -z "$comment_body" ]; then
  echo "gatekeeper-remediation: remediation.comment_body is empty" >&2
  exit 1
fi

if [ "${#comment_body}" -gt "$COMMENT_BODY_MAX_LEN" ]; then
  echo "gatekeeper-remediation: remediation.comment_body exceeds ${COMMENT_BODY_MAX_LEN} characters" >&2
  exit 1
fi

# Pass 1 — validate every label before touching GitHub, so a bad report
# cannot leave partial side effects behind.
while IFS= read -r -d '' label; do
  case "$label" in
    "${ALLOWED_LABELS[0]}"|"${ALLOWED_LABELS[1]}"|"${ALLOWED_LABELS[2]}") ;;
    *)
      echo "gatekeeper-remediation: refusing label outside allowlist (${ALLOWED_LABELS[*]}): $label" >&2
      exit 1
      ;;
  esac
done < <(jq -j '.remediation.labels_to_add[]? | (. + "\u0000")' "$report")

gh issue comment "$issue" --body "$comment_body"

# Pass 2 — NUL-delimited loop: report strings never undergo word splitting
# or globbing, and `--` keeps leading dashes from parsing as gh flags.
while IFS= read -r -d '' label; do
  gh issue edit "$issue" --add-label -- "$label"
done < <(jq -j '.remediation.labels_to_add[]? | (. + "\u0000")' "$report")
