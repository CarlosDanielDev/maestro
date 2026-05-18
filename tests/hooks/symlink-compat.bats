#!/usr/bin/env bats
#
# Tests for the backward-compat symlink .claude/hooks -> ../.maestro/hooks
# Issue #759: hooks relocated to .maestro/hooks; symlink preserves old paths
# for one release.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
}

@test "symlink .claude/hooks exists and is a symlink" {
  [ -L "$REPO_ROOT/.claude/hooks" ]
}

@test "symlink target resolves to ../.maestro/hooks" {
  target="$(readlink "$REPO_ROOT/.claude/hooks")"
  [ "$target" = "../.maestro/hooks" ]
}

@test "symlink resolves to a real directory" {
  [ -d "$REPO_ROOT/.claude/hooks" ]
}

@test "implement-gates.sh is reachable via symlink path" {
  [ -f "$REPO_ROOT/.claude/hooks/implement-gates.sh" ]
}

@test "notify.sh is reachable via symlink path" {
  [ -f "$REPO_ROOT/.claude/hooks/notify.sh" ]
}

@test "parse_gatekeeper_report.py is reachable via symlink path" {
  [ -f "$REPO_ROOT/.claude/hooks/parse_gatekeeper_report.py" ]
}
