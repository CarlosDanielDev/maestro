//! Byte-identical regression guard for the Claude provider's template render.
//!
//! `.maestro/templates/` is the source of truth for command specs.
//! `.claude/commands/*.md` files are generated artifacts. Each `#[test]` below
//! renders one canonical command through `ClaudeRules` and asserts the output
//! is byte-identical to the committed baseline. Drift in either direction
//! fails CI.

use std::path::Path;

use maestro::agent_provider::ClaudeProvider;
use maestro::templates::render_for_provider;

fn assert_byte_identical(command: &str, rendered: &str, committed_path: &Path) {
    let baseline = std::fs::read_to_string(committed_path)
        .unwrap_or_else(|e| panic!("cannot read baseline {}: {e}", committed_path.display()));
    if rendered == baseline {
        return;
    }
    let rendered_lines: Vec<&str> = rendered.lines().collect();
    let baseline_lines: Vec<&str> = baseline.lines().collect();
    let max_len = rendered_lines.len().max(baseline_lines.len());
    let mut diff = String::new();
    for i in 0..max_len {
        let r = rendered_lines.get(i).copied();
        let b = baseline_lines.get(i).copied();
        match (r, b) {
            (Some(rv), Some(bv)) if rv != bv => {
                diff.push_str(&format!("L{:>4} - {bv}\n", i + 1));
                diff.push_str(&format!("L{:>4} + {rv}\n", i + 1));
            }
            (Some(rv), None) => diff.push_str(&format!("L{:>4} + {rv}\n", i + 1)),
            (None, Some(bv)) => diff.push_str(&format!("L{:>4} - {bv}\n", i + 1)),
            _ => {}
        }
    }
    panic!(
        "Rendered `{command}` differs from baseline {}.\n\
         Baseline is a generated artifact — regenerate from canonical render.\n\
         Diff (- baseline, + rendered):\n{diff}",
        committed_path.display()
    );
}

fn render(command: &str) -> String {
    render_for_provider(&ClaudeProvider::default(), command)
        .unwrap_or_else(|e| panic!("render_for_provider failed for `{command}`: {e}"))
}

#[test]
fn renders_plan_feature_byte_identical() {
    let rendered = render("plan-feature");
    assert_byte_identical(
        "plan-feature",
        &rendered,
        Path::new(".claude/commands/plan-feature.md"),
    );
}

#[test]
fn renders_pushup_byte_identical() {
    let rendered = render("pushup");
    assert_byte_identical("pushup", &rendered, Path::new(".claude/commands/pushup.md"));
}

#[test]
fn renders_implement_byte_identical() {
    let rendered = render("implement");
    assert_byte_identical(
        "implement",
        &rendered,
        Path::new(".claude/commands/implement.md"),
    );
}

#[test]
fn renders_simplify_byte_identical() {
    let rendered = render("simplify");
    assert_byte_identical(
        "simplify",
        &rendered,
        Path::new(".claude/commands/simplify.md"),
    );
}

#[test]
fn simplify_render_contains_no_unresolved_placeholders() {
    let rendered = render("simplify");
    assert!(
        !rendered.contains("{{"),
        "simplify render contains unexpanded `{{{{...}}}}`:\n{rendered}"
    );
}

/// Regeneration helper for the #703 cutover. Not part of the default test
/// run — invoke explicitly:
///
/// ```text
/// cargo test --test templates_render -- --ignored regenerate_claude_baselines --nocapture
/// ```
///
/// After this writes the baselines, the byte-identical tests above pass.
#[test]
#[ignore = "manual regeneration of generated artifacts under .claude/commands/"]
fn regenerate_claude_baselines() {
    for command in ["implement", "pushup", "plan-feature", "simplify"] {
        let rendered = render(command);
        let path = format!(".claude/commands/{command}.md");
        std::fs::write(&path, &rendered).unwrap_or_else(|e| panic!("write `{path}`: {e}"));
        println!("wrote {path} ({} bytes)", rendered.len());
    }
}
