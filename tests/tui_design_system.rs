//! Guards for `docs/tui-design-system.md` (the TUI reuse guide).
//!
//! 1. Doc drift: every code name and file path the doc cites in inline
//!    backticks must still exist, so the guide can't silently rot.
//! 2. Theme-only colors: screens must not hardcode `Color::*` (except
//!    `Color::Reset`).
//! 3. No new hand-rolled text input: use `widgets::input::Input`.
//! 4. Every widget module is named in the doc.
//!
//! Existing debt in 2 and 3 is allowlisted; the lists may only shrink.

use std::fs;
use std::path::{Path, PathBuf};

const DOC: &str = "docs/tui-design-system.md";

/// Screen files that still hardcode colors. Remove an entry once fixed;
/// the test fails if an entry is stale, so the list can only shrink.
const RAW_COLOR_ALLOWLIST: &[&str] = &[
    "crates/maestro-tui/src/tui/screens/issue_wizard/draw_diff.rs",
    "crates/maestro-tui/src/tui/screens/pr_review/concerns.rs",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn rel(path: &Path) -> String {
    path.strip_prefix(root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Inline backtick spans outside fenced code blocks.
fn inline_code_spans(doc: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut in_fence = false;
    for line in doc.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut parts = line.split('`');
        parts.next();
        while let (Some(code), Some(_)) = (parts.next(), parts.next()) {
            spans.push(code.to_string());
        }
    }
    spans
}

/// A token is "code-like" when it has an underscore or an uppercase letter
/// and is 4+ chars: catches `Toggle`, `with_step`, `MAESTRO_ASCII_ICONS`,
/// skips plain words like `label` or `bool`.
fn code_like_tokens(span: &str) -> Vec<String> {
    span.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| t.len() >= 4)
        .filter(|t| !t.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .filter(|t| t.contains('_') || t.chars().any(|c| c.is_ascii_uppercase()))
        .map(str::to_string)
        .collect()
}

fn is_path(span: &str) -> bool {
    !span.contains(char::is_whitespace)
        && !span.contains('<')
        && !span.starts_with('*')
        && (span.ends_with(".rs") || span.ends_with(".md") || span.ends_with('/'))
}

#[test]
fn design_system_doc_names_exist_in_code() {
    let doc = fs::read_to_string(root().join(DOC)).expect("design-system doc exists");

    let mut files = Vec::new();
    rs_files(&root().join("src"), &mut files);
    // Core modules moved into the workspace crates (#1305).
    rs_files(&root().join("crates"), &mut files);
    rs_files(&root().join("tests"), &mut files);
    let src: String = files
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok())
        .collect::<Vec<_>>()
        .join("\n");
    let known_paths: Vec<String> = files.iter().map(|p| rel(p)).collect();

    let mut missing = Vec::new();
    for span in inline_code_spans(&doc) {
        // Shell commands and placeholders are not code names.
        if span.starts_with("cargo ") || span.starts_with("INSTA_UPDATE") || span.contains('<') {
            continue;
        }
        if is_path(&span) {
            let needle = span.trim_end_matches('/');
            let found = known_paths
                .iter()
                .any(|p| p.ends_with(needle) || p.contains(&format!("{needle}/")))
                || root().join(needle).exists();
            if !found {
                missing.push(format!("path `{span}`"));
            }
            continue;
        }
        for token in code_like_tokens(&span) {
            if !src.contains(&token) {
                missing.push(format!("`{token}` (from `{span}`)"));
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "{DOC} cites names that no longer exist in src/, crates/ or tests/. Update the doc:\n  {}",
        missing.join("\n  ")
    );
}

/// `(repo-relative path, source before the first #[cfg(test)])` for every
/// non-test `.rs` file under `dir`. Inline test modules may break the rules
/// on purpose (raw colors in assertions, fake key input).
fn non_test_sources(dir: &str) -> Vec<(String, String)> {
    let mut files = Vec::new();
    rs_files(&root().join(dir), &mut files);
    files
        .iter()
        .map(|p| (rel(p), fs::read_to_string(p).unwrap_or_default()))
        .filter(|(name, _)| !name.ends_with("tests.rs") && !name.contains("/tests/"))
        .map(|(name, text)| {
            let body = text.split("#[cfg(test)]").next().unwrap_or("").to_string();
            (name, body)
        })
        .collect()
}

/// Ratchet: new offenders fail, and so do allowlist entries that are no
/// longer offenders — the allowlist can only shrink.
fn assert_ratchet(offenders: &[String], allowlist: &[&str], list_name: &str, rule: &str) {
    let new: Vec<&str> = offenders
        .iter()
        .map(String::as_str)
        .filter(|f| !allowlist.contains(f))
        .collect();
    assert!(new.is_empty(), "{rule}\n  {}", new.join("\n  "));

    let stale: Vec<&&str> = allowlist
        .iter()
        .filter(|f| !offenders.iter().any(|o| o == **f))
        .collect();
    assert!(
        stale.is_empty(),
        "Fixed — remove these from {list_name}: {stale:?}"
    );
}

#[test]
fn screens_use_theme_colors_only() {
    let offenders: Vec<String> = non_test_sources("crates/maestro-tui/src/tui/screens")
        .into_iter()
        .filter(|(_, body)| body.matches("Color::").count() > body.matches("Color::Reset").count())
        .map(|(name, _)| name)
        .collect();
    assert_ratchet(
        &offenders,
        RAW_COLOR_ALLOWLIST,
        "RAW_COLOR_ALLOWLIST",
        "Hardcoded `Color::` in screens — use `&Theme` tokens (docs/tui-design-system.md §1):",
    );
}

/// Files that still hand-roll text input (`KeyCode::Char(c)` pushed onto a
/// `String`). Migrate to `widgets::input::Input` (#964–#974), then remove.
const HAND_ROLLED_INPUT_ALLOWLIST: &[&str] = &[
    "crates/maestro-tui/src/tui/screens/adapt/mod.rs",
    "crates/maestro-tui/src/tui/screens/bypass_warning.rs",
    "crates/maestro-tui/src/tui/screens/interaction/diff_review.rs",
    "crates/maestro-tui/src/tui/screens/issue_browser/draw.rs",
    "crates/maestro-tui/src/tui/screens/pr_review/mod.rs",
    "crates/maestro-tui/src/tui/screens/prd/input.rs",
    "crates/maestro-tui/src/tui/screens/roadmap_dispatch.rs",
    "crates/maestro-tui/src/tui/screens/settings/input.rs",
    "crates/maestro-tui/src/tui/screens/settings/schema_tab/modals/add_entry.rs",
    "crates/maestro-tui/src/tui/screens/team_wizard/compose.rs",
    "crates/maestro-tui/src/tui/screens/team_wizard/launch.rs",
    "crates/maestro-tui/src/tui/widgets/list_editor.rs",
    "crates/maestro-tui/src/tui/widgets/text_input.rs",
];

#[test]
fn no_new_hand_rolled_text_input() {
    // ponytail: same-file text heuristic. Misses input fed through a helper
    // method (e.g. `help.rs` `push_char`); upgrade to a syn-based check if
    // those start to slip through review.
    let offenders: Vec<String> = non_test_sources("crates/maestro-tui/src/tui")
        .into_iter()
        .filter(|(name, _)| !name.starts_with("crates/maestro-tui/src/tui/widgets/input/"))
        .filter(|(_, body)| {
            let binds_char =
                body.contains("KeyCode::Char(c)") || body.contains("KeyCode::Char(ch)");
            let edits_string = [".push(c)", ".push(ch)", ", c)", ", ch)"]
                .iter()
                .any(|p| body.contains(p) && (!p.starts_with(',') || body.contains(".insert(")));
            binds_char && edits_string
        })
        .map(|(name, _)| name)
        .collect();
    assert_ratchet(
        &offenders,
        HAND_ROLLED_INPUT_ALLOWLIST,
        "HAND_ROLLED_INPUT_ALLOWLIST",
        "Hand-rolled text input — use `widgets::input::Input` (docs/tui-design-system.md §2 \"Text input\"):",
    );
}

/// Key hints live only in the footer (`ds::key_hints`, #1258): screens list
/// them in `Screen::footer_hints` instead of drawing their own hint row.
#[test]
fn screens_draw_no_keybinds_bar() {
    let offenders: Vec<String> = non_test_sources("crates/maestro-tui/src/tui/screens")
        .into_iter()
        .filter(|(_, body)| body.contains("draw_keybinds_bar"))
        .map(|(name, _)| name)
        .collect();
    assert_ratchet(
        &offenders,
        &[],
        "the keybinds-bar allowlist",
        "Hint row drawn inside a screen — return the hints from `Screen::footer_hints` (docs/tui-design-system.md \"Shell rows\"):",
    );
}

/// Every module declared in `crates/maestro-tui/src/tui/widgets/mod.rs` must be cited in the
/// doc's inline code, so a new widget can't land unseen. Counts as cited: a
/// code token starting with its CamelCase type (`CiMonitor` → `CiMonitorWidget`),
/// or `<module>::` / `widgets/<module>`. Plain prose words ("badge",
/// "spinner") don't count.
#[test]
fn every_widget_is_documented() {
    let doc = fs::read_to_string(root().join(DOC)).expect("design-system doc exists");
    let spans = inline_code_spans(&doc);
    let widgets_mod = fs::read_to_string(root().join("crates/maestro-tui/src/tui/widgets/mod.rs"))
        .expect("widgets/mod.rs exists");
    let mut missing: Vec<String> = widgets_mod
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l
                .strip_prefix("pub mod ")
                .or_else(|| l.strip_prefix("mod "))?;
            let name = rest.strip_suffix(';')?;
            (!name.ends_with("_tests")).then(|| name.to_string())
        })
        .filter(|stem| {
            let camel: String = stem
                .split('_')
                .map(|w| {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                        .unwrap_or_default()
                })
                .collect();
            let cited = spans.iter().any(|span| {
                span.contains(&format!("{stem}::"))
                    || span.contains(&format!("widgets/{stem}"))
                    || span
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .any(|t| t.starts_with(&camel))
            });
            !cited
        })
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "Widgets missing from {DOC} §2 — document them (name + when to use): {missing:?}"
    );
}
