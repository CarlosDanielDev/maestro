#!/usr/bin/env bats
#
# Tests for .claude/hooks/implement-gates.sh
#
# Each test:
#   1. Creates a scratch git repo (via init-test-repo.sh).
#   2. Sets up PATH with fake-gh.sh and fake-cargo.sh in front.
#   3. Invokes the hook with environment overrides as needed.
#   4. Asserts exit code and relevant stdout/stderr.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HOOK="$REPO_ROOT/.claude/hooks/implement-gates.sh"
  FIXTURES="$REPO_ROOT/tests/hooks/fixtures"

  # Make the fakes visible as `gh` and `cargo` on PATH.
  SHIM_DIR="$(mktemp -d)"
  ln -s "$FIXTURES/fake-gh.sh" "$SHIM_DIR/gh"
  ln -s "$FIXTURES/fake-cargo.sh" "$SHIM_DIR/cargo"
  PATH="$SHIM_DIR:$PATH"
  export PATH

  # Scratch git repo.
  TEST_REPO="$("$FIXTURES/init-test-repo.sh")"
  cd "$TEST_REPO"
}

teardown() {
  cd /
  rm -rf "$TEST_REPO" "$SHIM_DIR"
}

# --- tests defined below, one per task ---

@test "exits 1 when not in a git repo" {
  cd "$(mktemp -d -t non-repo-XXXXXX)"
  run bash "$HOOK" 123
  [ "$status" -eq 1 ]
  [[ "$output" == *"not inside a git repository"* ]]
}

@test "exits 1 when gh CLI is not installed" {
  # Use a clean PATH without the shim dir.
  export PATH="/bin:/usr/bin"
  run bash "$HOOK" 123
  [ "$status" -eq 1 ]
  [[ "$output" == *"gh CLI not installed"* ]]
  [[ "$output" == *"brew install gh"* ]]
}

@test "exits 1 when gh is not authenticated" {
  export FAKE_GH_AUTH_STATUS=unauthed
  run bash "$HOOK" 123
  [ "$status" -eq 1 ]
  [[ "$output" == *"gh not authenticated"* ]]
  [[ "$output" == *"gh auth login"* ]]
}

@test "caches issue JSON to GATE_LOG_DIR/issue.json" {
  run bash "$HOOK" 123
  [[ "$output" == *"/tmp/maestro-123-"* ]]
  log_dir=$(echo "$output" | grep -oE '/tmp/maestro-123-[0-9]+' | head -1)
  [ -f "$log_dir/issue.json" ]
  python3 -c "import json; json.load(open('$log_dir/issue.json'))"
}

@test "exits 1 when issue is CLOSED" {
  export FAKE_GH_ISSUE_STATE=CLOSED
  run bash "$HOOK" 123
  [ "$status" -eq 1 ]
  [[ "$output" == *"Issue #123 is CLOSED"* ]]
}

@test "exits 6 on dirty tree when user chooses (A)bort" {
  echo "dirty" > new-file.txt
  run bash -c "echo 'A' | bash '$HOOK' 123"
  [ "$status" -eq 6 ]
  [[ "$output" == *"Working tree has uncommitted changes"* ]]
}
