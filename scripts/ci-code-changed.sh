#!/usr/bin/env bash
# Prints `code=true` unless every file changed since $1 is docs-only.
# Used by ci.yml to skip the heavy Rust jobs on docs/site-only diffs.
# Fail-safe: a missing or unknown base prints code=true.
set -uo pipefail

is_docs_only() {
  case "$1" in
    # Markdown the binary embeds, the drift gates check, or a test reads.
    CHANGELOG.md | docs/configuration.md | docs/tui-design-system.md | AGENTS.md | GEMINI.md) return 1 ;;
    .maestro/* | .claude/* | .codex/* | template/* | tests/*) return 1 ;;
    # docs/assets: media only. Scripts there (capture.sh) are tested.
    docs/assets/*.gif | docs/assets/*.png | docs/assets/*.svg | docs/assets/*.webm) return 0 ;;
    docs/assets/*) return 1 ;;
    *.md) return 0 ;;
    *) return 1 ;;
  esac
}

# `--classify <path>` prints docs|code for one path (used by the self-check).
if [ "${1:-}" = "--classify" ]; then
  if is_docs_only "${2:-}"; then echo docs; else echo code; fi
  exit 0
fi

base="${1:-}"
if [ -z "$base" ] || [ "$base" = "0000000000000000000000000000000000000000" ] \
  || ! files="$(git diff --name-only "$base" HEAD 2>/dev/null)" || [ -z "$files" ]; then
  echo "code=true"
  exit 0
fi


while IFS= read -r f; do
  is_docs_only "$f" || { echo "code=true"; exit 0; }
done <<< "$files"
echo "code=false"
