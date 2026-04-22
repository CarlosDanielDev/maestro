#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/check-layers.sh"
  MANIFEST="$REPO_ROOT/scripts/architecture-layers.yml"
  FIXTURES="$REPO_ROOT/tests/scripts/fixtures"
  # Each test runs in a clean tempdir with a faked src/ layout.
  TEST_ROOT="$(mktemp -d -t layers-test-XXXXXX)"
  cd "$TEST_ROOT"
  mkdir -p src/session src/state src/tui
}

teardown() {
  cd /
  rm -rf "$TEST_ROOT"
}

@test "same-layer import passes (domain → domain)" {
  mkdir -p src/session
  cp "$FIXTURES/layers-samelayer-ok.rs" src/session/module_a.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 0 ]
}

@test "lower-layer import passes (ui → domain)" {
  mkdir -p src/tui
  cp "$FIXTURES/layers-lowerlayer-ok.rs" src/tui/screen.rs
  # Stub the target.
  mkdir -p src/session
  echo "pub struct SessionManager;" > src/session/manager.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 0 ]
}

@test "higher-layer import fails (domain → ui)" {
  mkdir -p src/session src/tui
  cp "$FIXTURES/layers-higherlayer-fail.rs" src/session/naughty.rs
  echo "pub struct ThemeConfig;" > src/tui/theme.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  # session→tui matches both the layer-ordering rule and the explicit forbidden pair;
  # the script emits FORBIDDEN (more specific) so we accept either keyword.
  [[ "$output" == *"VIOLATION"* || "$output" == *"FORBIDDEN"* ]]
}

@test "forbidden pair fails (state → tui)" {
  mkdir -p src/state src/tui
  cp "$FIXTURES/layers-forbiddenpair-fail.rs" src/state/store.rs
  echo "pub struct SerializableColor;" > src/tui/theme.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FORBIDDEN"* ]]
}

@test "known-debt entry with future deadline is tolerated" {
  mkdir -p src/state src/tui
  cp "$FIXTURES/layers-forbiddenpair-fail.rs" src/state/store.rs
  echo "pub struct SerializableColor;" > src/tui/theme.rs
  cat > layers-debt.txt <<'EOF'
src/state/store.rs → src/tui/theme.rs # deadline: 2099-12-31, owner: @test, ticket: #TEST, plan: resolve later
EOF
  DEBT_FILE_OVERRIDE="$PWD/layers-debt.txt" run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 0 ]
}

@test "known-debt entry with past deadline fails with DEADLINE PAST" {
  mkdir -p src/state src/tui
  cp "$FIXTURES/layers-forbiddenpair-fail.rs" src/state/store.rs
  echo "pub struct SerializableColor;" > src/tui/theme.rs
  cat > layers-debt.txt <<'EOF'
src/state/store.rs → src/tui/theme.rs # deadline: 2000-01-01, owner: @test, ticket: #TEST, plan: overdue
EOF
  DEBT_FILE_OVERRIDE="$PWD/layers-debt.txt" run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"DEADLINE PAST"* ]]
}
