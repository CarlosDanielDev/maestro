#!/usr/bin/env bats
#
# Tests for scripts/check-design-drift.sh (#1238).
# Each test builds a throwaway git repo: `main` holds the base, a branch adds
# the fixture diff, and the script compares the branch against `main`.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/check-design-drift.sh"
  TEST_REPO="$(mktemp -d -t design-drift-test-XXXXXX)"
  cd "$TEST_REPO"
  git init --quiet -b main
  git config user.email "test@example.com"
  git config user.name "test"
  mkdir -p crates/maestro-tui/src/tui docs/design-system src
  echo "base" > crates/maestro-tui/src/tui/app.rs
  echo "base" > docs/tui-design-system.md
  echo "base" > docs/design-system/decisions.md
  echo "base" > src/main.rs
  git add -A
  git commit --quiet -m base
  git checkout --quiet -b feature
  export DESIGN_DRIFT_BASE=main
  unset PR_BODY
}

teardown() {
  cd /
  rm -rf "$TEST_REPO"
}

change() {
  for f in "$@"; do echo "changed" >> "$f"; done
  git add -A
  git commit --quiet -m change
}

@test "TUI change without a doc change or PR-body line fails" {
  change crates/maestro-tui/src/tui/app.rs
  run bash "$SCRIPT"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Design: no change"* ]]
}

@test "TUI change with a PR body that has no marker line fails" {
  change crates/maestro-tui/src/tui/app.rs
  PR_BODY=$'Some text\nDesign: changed a lot' run bash "$SCRIPT"
  [ "$status" -eq 1 ]
}

@test "TUI change with docs/tui-design-system.md changed passes" {
  change crates/maestro-tui/src/tui/app.rs docs/tui-design-system.md
  run bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "TUI change with docs/design-system/decisions.md changed passes" {
  change crates/maestro-tui/src/tui/app.rs docs/design-system/decisions.md
  run bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "TUI change with a 'Design: no change' line in the PR body passes" {
  change crates/maestro-tui/src/tui/app.rs
  # GitHub sends PR bodies with CRLF line endings.
  PR_BODY=$'Summary\r\nDesign: no change\r\nMore' run bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "diff that does not touch the TUI passes" {
  change src/main.rs
  run bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "unknown base ref is an error, not a pass" {
  change crates/maestro-tui/src/tui/app.rs
  DESIGN_DRIFT_BASE=no-such-ref run bash "$SCRIPT"
  [ "$status" -eq 2 ]
}
