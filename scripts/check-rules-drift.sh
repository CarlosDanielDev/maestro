#!/usr/bin/env bash
#
# check-rules-drift.sh
#
# Verifies every provider entry point embeds the canonical Golden Rules
# block verbatim. The canonical file is `.maestro/templates/core/golden-rules.md`.
# Each provider file must contain a single `<!-- BEGIN GOLDEN-RULES ... -->`
# / `<!-- END GOLDEN-RULES -->` pair surrounding the canonical content.
#
# Exit codes:
#   0 — all provider files match the canonical
#   1 — at least one provider drifted or is missing the block

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CANONICAL="$ROOT_DIR/.maestro/templates/core/golden-rules.md"

if [[ ! -f "$CANONICAL" ]]; then
  echo "ERROR: canonical file not found at $CANONICAL" >&2
  exit 1
fi

PROVIDERS=(
  ".claude/CLAUDE.md"
  ".codex/AGENTS.md"
  "AGENTS.md"
  "GEMINI.md"
)

extract_block() {
  local file="$1"
  awk '
    /<!-- BEGIN GOLDEN-RULES/ { flag = 1; next }
    /<!-- END GOLDEN-RULES/   { flag = 0 }
    flag                       { print }
  ' "$file"
}

canonical_content="$(cat "$CANONICAL")"
fail=0

for rel in "${PROVIDERS[@]}"; do
  file="$ROOT_DIR/$rel"
  if [[ ! -f "$file" ]]; then
    echo "MISSING: $rel does not exist" >&2
    fail=1
    continue
  fi
  embedded="$(extract_block "$file")"
  if [[ -z "$embedded" ]]; then
    echo "MISSING-BLOCK: $rel has no GOLDEN-RULES block" >&2
    fail=1
    continue
  fi
  if [[ "$embedded" != "$canonical_content" ]]; then
    echo "DRIFT: $rel does not match $CANONICAL" >&2
    diff <(echo "$canonical_content") <(echo "$embedded") | head -40 >&2 || true
    fail=1
  fi
done

if [[ $fail -eq 0 ]]; then
  echo "golden-rules drift check: OK (${#PROVIDERS[@]} provider entry points in sync)"
fi
exit $fail
