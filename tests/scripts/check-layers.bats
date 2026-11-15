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

# --- maestro-core crate boundary (#1305) ---------------------------------

@test "maestro-core importing the TUI crate fails" {
  mkdir -p crates/maestro-core/src
  echo "use maestro_tui::App;" > crates/maestro-core/src/naughty.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"CORE-FRONTEND: crates/maestro-core/src/naughty.rs"* ]]
}

@test "maestro-core naming a frontend module fails" {
  mkdir -p crates/maestro-core/src/config
  echo "use crate::tui::theme::ThemeConfig;" > crates/maestro-core/src/config/tui.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"CORE-FRONTEND"* ]]
}

@test "maestro-core using ratatui widgets or crossterm fails" {
  mkdir -p crates/maestro-core/src
  echo "use ratatui::widgets::Block;" > crates/maestro-core/src/a.rs
  echo "use crossterm::event::KeyCode;" > crates/maestro-core/src/b.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"crates/maestro-core/src/a.rs"* ]]
  [[ "$output" == *"crates/maestro-core/src/b.rs"* ]]
}

@test "maestro-core using any ratatui path fails, Color included" {
  mkdir -p crates/maestro-core/src/config
  echo "use ratatui::style::Color;" > crates/maestro-core/src/config/theme.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"CORE-FRONTEND: crates/maestro-core/src/config/theme.rs"* ]]
}

@test "maestro-core aliasing ratatui fails (use / extern crate ... as)" {
  mkdir -p crates/maestro-core/src
  echo "use ratatui as rt;" > crates/maestro-core/src/a.rs
  echo "extern crate crossterm as ct;" > crates/maestro-core/src/b.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"crates/maestro-core/src/a.rs"* ]]
  [[ "$output" == *"crates/maestro-core/src/b.rs"* ]]
}

@test "maestro-core manifest depending on ratatui fails" {
  mkdir -p crates/maestro-core/src
  printf '[package]\nname = "maestro-core"\n[dependencies]\nratatui = { version = "0.29", default-features = false }\n' > crates/maestro-core/Cargo.toml
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"CORE-FRONTEND: crates/maestro-core/Cargo.toml:4"* ]]
}

@test "maestro-core clean source, data strings and comments pass" {
  mkdir -p crates/maestro-core/src
  printf '[package]\nname = "maestro-core"\n[dependencies]\nserde = "1"\n' > crates/maestro-core/Cargo.toml
  printf '// ratatui::widgets is the TUI'"'"'s job\nconst KNOWN: &[&str] = &["ratatui", "crossterm"];\n' > crates/maestro-core/src/scanner.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 0 ]
}
