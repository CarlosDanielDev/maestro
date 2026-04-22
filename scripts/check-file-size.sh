#!/bin/bash
# Enforce maximum file size for Rust source files.
# Files listed in scripts/allowlist-large-files.txt are exempt.

set -euo pipefail

MAX_LINES=400
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ALLOWLIST="$SCRIPT_DIR/allowlist-large-files.txt"

# Load allowlist — preserve both stripped paths and raw lines.
allowed=()
allowed_raw=()
if [[ -f "$ALLOWLIST" ]]; then
  while IFS= read -r line; do
    raw="$line"
    stripped="${line%%#*}"
    stripped="${stripped// /}"
    [[ -z "$stripped" ]] && continue
    allowed+=("$stripped")
    allowed_raw+=("$raw")
  done < "$ALLOWLIST"
fi

is_allowed() {
  local file="$1"
  for pattern in "${allowed[@]+"${allowed[@]}"}"; do
    [[ "$file" == "$pattern" ]] && return 0
  done
  return 1
}

violations=0

while IFS= read -r entry; do
  # Trim leading whitespace from wc output
  entry=$(echo "$entry" | sed 's/^[[:space:]]*//')
  lines="${entry%% *}"
  file="${entry##* }"
  rel="${file#$ROOT_DIR/}"

  if is_allowed "$rel"; then
    continue
  fi

  if (( lines > MAX_LINES )); then
    echo "VIOLATION: $rel ($lines lines, max $MAX_LINES)"
    violations=$((violations + 1))
  fi
done < <(find "$ROOT_DIR/src" -name '*.rs' -exec wc -l {} + | grep -v ' total$')

# Deadline enforcement — iterate raw entries to find `deadline: YYYY-MM-DD`.
today=$(date +%Y-%m-%d)
for entry in "${allowed_raw[@]+"${allowed_raw[@]}"}"; do
  deadline=$(echo "$entry" | grep -oE 'deadline: [0-9-]+' | sed 's/deadline: //' || true)
  if [[ -n "$deadline" && "$deadline" < "$today" ]]; then
    echo "DEADLINE PAST: $entry"
    violations=$((violations + 1))
  fi
done

if (( violations > 0 )); then
  echo ""
  echo "$violations file(s) exceed the $MAX_LINES-line limit."
  echo "Split large files or add them to scripts/allowlist-large-files.txt (temporary)."
  exit 1
fi

echo "All files within $MAX_LINES-line limit."
