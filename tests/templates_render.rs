//! Byte-identical regression guard for the Claude provider's template render.
//!
//! `.maestro/templates/` is the source of truth for command specs.
//! `.claude/commands/*.md` files are generated artifacts. Each `#[test]` below
//! renders one canonical command through `ClaudeRules` and asserts the output
//! is byte-identical to the committed baseline. Drift in either direction
//! fails CI.

use std::path::Path;

use maestro::agent_provider::{
    AgentProvider, ClaudeProvider, CodexProvider, MinimaxProvider, OllamaProvider, QwenProvider,
};
use maestro::commands::sync_templates::banner::with_banner;
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

fn render_with_banner(command: &str) -> String {
    with_banner(&render(command), command)
}

#[test]
fn renders_plan_feature_byte_identical() {
    let rendered = render_with_banner("plan-feature");
    assert_byte_identical(
        "plan-feature",
        &rendered,
        Path::new(".claude/commands/plan-feature.md"),
    );
}

#[test]
fn renders_pushup_byte_identical() {
    let rendered = render_with_banner("pushup");
    assert_byte_identical("pushup", &rendered, Path::new(".claude/commands/pushup.md"));
}

#[test]
fn renders_implement_byte_identical() {
    let rendered = render_with_banner("implement");
    assert_byte_identical(
        "implement",
        &rendered,
        Path::new(".claude/commands/implement.md"),
    );
}

#[test]
fn renders_simplify_byte_identical() {
    let rendered = render_with_banner("simplify");
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

// ──────────────────────────────────────────────────────────────────────────
// Codex provider — semantic assertions (no committed baselines because
// Codex has no `.codex/commands/` analogue; see CodexRules::target_dir).
// ──────────────────────────────────────────────────────────────────────────

fn render_codex(command: &str) -> String {
    render_for_provider(&CodexProvider::new("codex"), command)
        .unwrap_or_else(|e| panic!("render_for_provider failed for codex `{command}`: {e}"))
}

#[test]
fn codex_renders_all_four_canonical_commands_with_zero_unresolved_placeholders() {
    for command in ["implement", "pushup", "plan-feature", "simplify"] {
        let rendered = render_codex(command);
        assert!(
            !rendered.contains("{{") && !rendered.contains("}}"),
            "codex render of `{command}` contains unexpanded `{{{{...}}}}`:\n{rendered}"
        );
    }
}

#[test]
fn codex_implement_render_contains_inline_subtask_heading() {
    let rendered = render_codex("implement");
    assert!(
        rendered.contains("## Sub-task:"),
        "expected `## Sub-task:` in codex implement render; got:\n{}",
        &rendered[..rendered.len().min(500)]
    );
    assert!(
        !rendered.contains("Use the Task tool"),
        "codex render leaked Claude phrasing"
    );
}

#[test]
fn codex_simplify_render_inlines_skill_body() {
    let rendered = render_codex("simplify");
    assert!(
        rendered.contains("project-patterns"),
        "expected inlined skill body in codex simplify render"
    );
    assert!(
        !rendered.contains(".claude/skills/project-patterns/SKILL.md)"),
        "codex render leaked Claude's skill_link phrasing"
    );
}

#[test]
fn codex_hook_gate_uses_provider_neutral_dot_maestro_path() {
    let rendered = render_codex("implement");
    assert!(
        rendered.contains("bash .maestro/hooks/implement-gates.sh"),
        "expected provider-neutral hook path from HOOK_GATE expansion"
    );
    assert!(
        !rendered.contains("bash .claude/hooks/"),
        "codex render must not emit legacy .claude/hooks/ path (#759)"
    );
}

#[test]
fn claude_hook_gate_uses_dot_maestro_path() {
    // Regression guard for #759: ClaudeRules::hook_gate must emit `.maestro/hooks/`,
    // matching the Codex provider. Integration-level check; the unit tests in
    // `src/templates/provider_rules/claude.rs` verify the method in isolation.
    let rendered = render("implement");
    assert!(
        rendered.contains("bash .maestro/hooks/implement-gates.sh"),
        "expected .maestro/hooks/ path from Claude HOOK_GATE expansion"
    );
    assert!(
        !rendered.contains("bash .claude/hooks/"),
        "claude render must not emit legacy .claude/hooks/ path (#759)"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// HTTP-generic providers (Qwen, Ollama, MiniMax) — semantic assertions only.
// No committed baselines: target_dir() is None, output is runtime-injected
// (cached on disk by `maestro sync-templates` in #707, appended at session
// spawn in #708).
// ──────────────────────────────────────────────────────────────────────────

fn render_via(provider: &dyn AgentProvider, command: &str) -> String {
    render_for_provider(provider, command).unwrap_or_else(|e| {
        panic!(
            "render_for_provider failed for `{command}` via `{}`: {e}",
            provider.id()
        )
    })
}

fn ollama_provider() -> OllamaProvider {
    OllamaProvider::new("ollama", "http://localhost:11434", "llama3", 10, None)
        .expect("ollama provider builds")
}

fn minimax_provider() -> MinimaxProvider {
    MinimaxProvider::new(
        "minimax",
        "https://api.minimax.io/v1",
        "MiniMax-M2.7",
        10,
        Some("MINIMAX_API_KEY".to_string()),
    )
    .expect("minimax provider builds")
}

#[test]
fn qwen_renders_all_four_canonical_commands_with_zero_unresolved_placeholders() {
    let provider = QwenProvider::new("qwen");
    for command in ["implement", "pushup", "plan-feature", "simplify"] {
        let rendered = render_via(&provider, command);
        assert!(
            !rendered.contains("{{") && !rendered.contains("}}"),
            "qwen render of `{command}` contains unexpanded `{{{{...}}}}`:\n{rendered}"
        );
    }
}

#[test]
fn ollama_renders_all_four_canonical_commands_with_zero_unresolved_placeholders() {
    let provider = ollama_provider();
    for command in ["implement", "pushup", "plan-feature", "simplify"] {
        let rendered = render_via(&provider, command);
        assert!(
            !rendered.contains("{{") && !rendered.contains("}}"),
            "ollama render of `{command}` contains unexpanded `{{{{...}}}}`:\n{rendered}"
        );
    }
}

#[test]
fn minimax_renders_all_four_canonical_commands_with_zero_unresolved_placeholders() {
    let provider = minimax_provider();
    for command in ["implement", "pushup", "plan-feature", "simplify"] {
        let rendered = render_via(&provider, command);
        assert!(
            !rendered.contains("{{") && !rendered.contains("}}"),
            "minimax render of `{command}` contains unexpanded `{{{{...}}}}`:\n{rendered}"
        );
    }
}

#[test]
fn http_generic_hook_gate_renders_instruction_text() {
    let provider = QwenProvider::new("qwen");
    let rendered = render_via(&provider, "implement");
    assert!(
        rendered.contains("the orchestrator MUST run:"),
        "expected instruction-text hook_gate phrasing in HTTP-generic render"
    );
    assert!(
        rendered.contains(".maestro/hooks/implement-gates.sh"),
        "expected hook script reference embedded in instruction text"
    );
}

#[test]
fn http_generic_skill_link_inlines_skill_body() {
    let provider = QwenProvider::new("qwen");
    let rendered = render_via(&provider, "simplify");
    assert!(
        !rendered.contains(".claude/skills/project-patterns/SKILL.md)"),
        "HTTP-generic render leaked Claude's skill_link phrasing"
    );
    assert!(
        rendered.contains("project-patterns"),
        "expected inlined skill body in HTTP-generic simplify render"
    );
}
