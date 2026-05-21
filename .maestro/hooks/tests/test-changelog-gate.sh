#!/usr/bin/env bash
# Tests for lib-changelog.sh — CHANGELOG-drift detection helpers.
# Run with: bash .maestro/hooks/tests/test-changelog-gate.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LIB="$SCRIPT_DIR/../lib-changelog.sh"

if [ ! -f "$LIB" ]; then
  echo "FAIL: $LIB does not exist"
  exit 1
fi

pass=0
fail=0

run_case() {
  local name="$1"
  local actual="$2"
  local expected="$3"
  if [ "$actual" = "$expected" ]; then
    echo "PASS: $name"
    pass=$((pass + 1))
  else
    echo "FAIL: $name — expected '$expected', got '$actual'"
    fail=$((fail + 1))
  fi
}

# Build a throwaway git repo with `main` and a `feature` branch.
# Echoes the temp dir path.
make_repo() {
  local dir
  dir=$(mktemp -d)
  (
    cd "$dir"
    git init -q -b main
    git config user.email "test@example.com"
    git config user.name "test"
    printf "# Changelog\n\n## [Unreleased]\n" > CHANGELOG.md
    printf "fn main() {}\n" > src.rs
    git add . > /dev/null
    git commit -q -m "base"
    git checkout -q -b feature
  )
  echo "$dir"
}

# Case 1: CHANGELOG edited on feature branch -> detected.
repo=$(make_repo)
(
  cd "$repo"
  printf "\n- new bullet (#1)\n" >> CHANGELOG.md
  git add CHANGELOG.md > /dev/null
  git commit -q -m "edit changelog"
)
actual=$(
  cd "$repo"
  # shellcheck source=../lib-changelog.sh
  source "$LIB"
  if changelog_changed_vs_base "main"; then echo CHANGED; else echo CLEAN; fi
)
run_case "changelog_edited_on_feature_detected" "$actual" "CHANGED"
rm -rf "$repo"

# Case 2: only source files edited -> not detected.
repo=$(make_repo)
(
  cd "$repo"
  printf "fn other() {}\n" >> src.rs
  git add src.rs > /dev/null
  git commit -q -m "edit src"
)
actual=$(
  cd "$repo"
  source "$LIB"
  if changelog_changed_vs_base "main"; then echo CHANGED; else echo CLEAN; fi
)
run_case "source_only_edit_not_detected" "$actual" "CLEAN"
rm -rf "$repo"

# Case 3: file containing CHANGELOG.md as a substring -> not detected (anchored grep).
repo=$(make_repo)
(
  cd "$repo"
  mkdir -p docs
  printf "archive\n" > docs/CHANGELOG-archive.md
  git add docs/CHANGELOG-archive.md > /dev/null
  git commit -q -m "add archive"
)
actual=$(
  cd "$repo"
  source "$LIB"
  if changelog_changed_vs_base "main"; then echo CHANGED; else echo CLEAN; fi
)
run_case "archive_with_similar_name_not_detected" "$actual" "CLEAN"
rm -rf "$repo"

# Case 4: no diff at all -> not detected.
repo=$(make_repo)
actual=$(
  cd "$repo"
  source "$LIB"
  if changelog_changed_vs_base "main"; then echo CHANGED; else echo CLEAN; fi
)
run_case "no_diff_not_detected" "$actual" "CLEAN"
rm -rf "$repo"

# Case 5: neither main nor origin/main exists -> resolve returns empty.
repo=$(mktemp -d)
(
  cd "$repo"
  git init -q -b trunk
  git config user.email "test@example.com"
  git config user.name "test"
  printf "x\n" > a.txt
  git add . > /dev/null
  git commit -q -m base
)
actual=$(
  cd "$repo"
  source "$LIB"
  resolve_changelog_diff_base
)
run_case "no_main_resolves_empty" "$actual" ""
rm -rf "$repo"

# Case 6: only main exists -> resolve returns "main".
repo=$(make_repo)
actual=$(
  cd "$repo"
  source "$LIB"
  resolve_changelog_diff_base
)
run_case "main_resolves_to_main" "$actual" "main"
rm -rf "$repo"

echo ""
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
