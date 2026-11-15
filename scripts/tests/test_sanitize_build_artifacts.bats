#!/usr/bin/env bats

# SUP-12 (#1044): paths with spaces must reach `rm` as one argument.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/sanitize-build-artifacts.sh"
  WORK="$(mktemp -d)"
  git -C "$WORK" init -q
}

teardown() {
  chmod -R u+w "$WORK"
  rm -r "$WORK"
}

@test "--snap-new removes a spaced-path draft and nothing else" {
  mkdir -p "$WORK/a b"
  touch "$WORK/a b/x.snap.new" "$WORK/a" "$WORK/b"
  cd "$WORK"
  run bash "$SCRIPT" --snap-new --yes
  [ "$status" -eq 0 ]
  [ ! -e "$WORK/a b/x.snap.new" ]
  [ -d "$WORK/a b" ]
  [ -e "$WORK/a" ]
  [ -e "$WORK/b" ]
}

@test "--incremental removes a spaced-path dir and spares its word-split twin" {
  mkdir -p "$WORK/target/a b/incremental/x" "$WORK/target/a"
  touch "$WORK/target/a/keep"
  cd "$WORK"
  run bash "$SCRIPT" --incremental --yes
  [ "$status" -eq 0 ]
  [ ! -e "$WORK/target/a b/incremental" ]
  [ -d "$WORK/target/a b" ]
  [ -e "$WORK/target/a/keep" ]
}
