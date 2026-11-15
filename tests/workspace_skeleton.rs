//! Guards the cargo workspace layout from issue #1304 (ADR 006).
//!
//! The three crates must stay workspace members, and their dependency
//! direction must follow ADR 006: core depends on no frontend, the TUI
//! depends on core only, the CLI depends on core only (#1307). The `maestro`
//! binary depends on both frontends and is the one place that opens the TUI.

use std::path::Path;

const CRATES: [&str; 3] = ["maestro-core", "maestro-tui", "maestro-cli"];

fn manifest(path: &Path) -> toml::Table {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    text.parse()
        .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()))
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn deps(table: &toml::Table) -> Vec<String> {
    table
        .get("dependencies")
        .and_then(|d| d.as_table())
        .map(|d| d.keys().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn workspace_lists_the_three_crates() {
    let root_manifest = manifest(&root().join("Cargo.toml"));
    let members: Vec<&str> = root_manifest["workspace"]["members"]
        .as_array()
        .expect("[workspace].members must be an array")
        .iter()
        .filter_map(|m| m.as_str())
        .collect();
    for name in CRATES {
        let path = format!("crates/{name}");
        assert!(
            members.contains(&path.as_str()),
            "{path} missing from {members:?}"
        );
        let crate_manifest = manifest(&root().join(&path).join("Cargo.toml"));
        assert_eq!(crate_manifest["package"]["name"].as_str(), Some(name));
    }
}

#[test]
fn crate_dependencies_follow_adr_006() {
    let deps_of = |name: &str| {
        deps(&manifest(
            &root().join("crates").join(name).join("Cargo.toml"),
        ))
    };
    let core = deps_of("maestro-core");
    assert!(
        !core
            .iter()
            .any(|d| d == "maestro-tui" || d == "maestro-cli"),
        "maestro-core must not depend on a frontend: {core:?}"
    );
    let tui = deps_of("maestro-tui");
    assert!(
        !tui.iter().any(|d| d == "maestro-cli"),
        "maestro-tui must not depend on maestro-cli: {tui:?}"
    );
}

#[test]
fn cli_crate_has_no_terminal_ui_dependency() {
    let manifest_path = root().join("crates/maestro-cli/Cargo.toml");
    let cli = deps(&manifest(&manifest_path));
    for banned in [
        "maestro-tui",
        "ratatui",
        "crossterm",
        "tui-textarea",
        "arboard",
    ] {
        assert!(
            !cli.iter().any(|d| d == banned),
            "maestro-cli must not depend on {banned} (#1307): {cli:?}"
        );
    }
}

#[test]
fn cli_code_lives_in_the_cli_crate() {
    for path in [
        "crates/maestro-cli/src/cli.rs",
        "crates/maestro-cli/src/commands/mod.rs",
    ] {
        assert!(root().join(path).is_file(), "{path} must exist (#1307)");
    }
    for path in ["src/cli.rs", "src/cli_team.rs"] {
        assert!(
            !root().join(path).exists(),
            "{path} must live in crates/maestro-cli (#1307)"
        );
    }
}

/// The manifest check above misses a crate reached some other way (a
/// re-export through core, a `#[path]` include); scan the source too.
#[test]
fn cli_crate_source_names_no_terminal_ui_crate() {
    let banned = regex::Regex::new(r"\b(crossterm|ratatui|maestro_tui|tui_textarea)\b")
        .expect("valid regex");
    let mut hits = Vec::new();
    let mut dirs = vec![root().join("crates/maestro-cli/src")];
    while let Some(dir) = dirs.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).expect("read source");
                for (n, line) in text.lines().enumerate() {
                    if banned.is_match(line) {
                        hits.push(format!("{}:{}: {line}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "maestro-cli must not name a terminal-UI crate (#1307):\n{}",
        hits.join("\n")
    );
}
