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
check "golden-rules copies (read by a Rust test)" true AGENTS.md GEMINI.md .codex/AGENTS.md
check "script under docs/assets (read by a Rust test)" true docs/assets/capture.sh
check "media under docs/assets" false docs/assets/demo.gif docs/assets/og.png docs/assets/hero.svg docs/assets/demo.webm

for base in "" 0000000000000000000000000000000000000000 deadbeef; do
  got="$(bash "$script" "$base")"
  if [ "$got" = "code=true" ]; then echo "ok   bad base '$base'"; else
    echo "FAIL bad base '$base': got $got"
    fails=$((fails + 1))
  fi
done

# Guard: no tracked docs-only file may be named by code or tests. A test
# that reads one would be skipped on the very diff that breaks it (#1213
# broke tests/capture_script.rs this way). README.md is exempt: tests only
# create their own temp README.md, none reads the repo's.
repo_root="$(cd "$(dirname "$script")/.." && pwd)"
while IFS= read -r f; do
  [ "$f" = README.md ] && continue
  [ "$(cd "$repo_root" && bash "$script" --classify "$f")" = docs ] || continue
  if hits="$(cd "$repo_root" && git grep -l -F "\"$f\"" -- src tests build.rs 2>/dev/null)"; then
    echo "FAIL docs-only file referenced by code/tests: $f <- $(echo "$hits" | tr '\n' ' ')"
    fails=$((fails + 1))
  fi
done < <(cd "$repo_root" && git ls-files)
echo "ok   no docs-only file is referenced by code/tests"

[ "$fails" -eq 0 ] || { echo "$fails case(s) failed"; exit 1; }
