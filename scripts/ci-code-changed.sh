#!/usr/bin/env bash
# Prints `code=true` unless every file changed since $1 is docs-only.
# Used by ci.yml to skip the heavy Rust jobs on docs/site-only diffs.
# Fail-safe: a missing or unknown base prints code=true.
set -uo pipefail

base="${1:-}"
if [ -z "$base" ] || [ "$base" = "0000000000000000000000000000000000000000" ] \
  || ! files="$(git diff --name-only "$base" HEAD 2>/dev/null)" || [ -z "$files" ]; then
  echo "code=true"
  exit 0
fi

is_docs_only() {
  case "$1" in
    # Markdown the binary embeds or the drift gates generate/check.
    CHANGELOG.md | docs/configuration.md | .maestro/* | .claude/* | template/* | tests/*) return 1 ;;
    *.md | docs/assets/*) return 0 ;;
    *) return 1 ;;
  esac
}

while IFS= read -r f; do
  is_docs_only "$f" || { echo "code=true"; exit 0; }
done <<< "$files"
echo "code=false"
