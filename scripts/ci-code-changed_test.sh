#!/usr/bin/env bash
# Self-check for scripts/ci-code-changed.sh, run by the CI `changes` job.
# Builds throwaway repos and asserts the verdict for each diff shape.
set -euo pipefail

script="$(cd "$(dirname "$0")" && pwd)/ci-code-changed.sh"
fails=0

# check <name> <expected> <file>... — commits the files on top of a base
# commit, one commit per file, then diffs base..HEAD like the PR event does.
check() {
  local name="$1" want="$2"
  shift 2
  local repo
  repo="$(mktemp -d)"
  git -C "$repo" init -q
  git -C "$repo" -c user.name=t -c user.email=t@t commit -q --allow-empty -m base
  local base
  base="$(git -C "$repo" rev-parse HEAD)"
  for f in "$@"; do
    mkdir -p "$repo/$(dirname "$f")"
    echo x >>"$repo/$f"
    git -C "$repo" add "$f"
    git -C "$repo" -c user.name=t -c user.email=t@t commit -q -m "$f"
  done
  local got
  got="$(cd "$repo" && bash "$script" "$base")"
  if [ "$got" = "code=$want" ]; then
    echo "ok   $name"
  else
    echo "FAIL $name: want code=$want, got $got"
    fails=$((fails + 1))
  fi
  rm -r "$repo"
}

check "readme only" false README.md
check "docs dir + assets" false docs/guides/x.md docs/assets/demo.gif
check "docs + code in one PR" true README.md src/main.rs
check "docs first, code in a later push" true docs/a.md docs/b.md src/lib.rs
check "code first, docs in a later push" true src/lib.rs README.md
check "workflow only" true .github/workflows/ci.yml
check "CHANGELOG (embedded)" true CHANGELOG.md
check "generated config doc" true docs/configuration.md
check "templates md" true .maestro/templates/commands/x.md
check "claude md" true .claude/CLAUDE.md
check "test fixture md" true tests/fixtures/x.md

for base in "" 0000000000000000000000000000000000000000 deadbeef; do
  got="$(bash "$script" "$base")"
  if [ "$got" = "code=true" ]; then echo "ok   bad base '$base'"; else
    echo "FAIL bad base '$base': got $got"
    fails=$((fails + 1))
  fi
done

[ "$fails" -eq 0 ] || { echo "$fails case(s) failed"; exit 1; }
