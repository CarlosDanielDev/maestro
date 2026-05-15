//! Regenerates Claude provider command baselines under `.claude/commands/`
//! from the canonical sources in `.maestro/templates/commands/`.
//!
//! Run from the repo root:
//!
//! ```text
//! cargo run --example regen_templates
//! ```
//!
//! The `tests/templates_render.rs::renders_*_byte_identical` tests are the
//! post-condition: if the example writes bytes that differ from what the
//! render engine produces, those tests fail on the next `cargo test` run.

use anyhow::Context;
use maestro::agent_provider::{AgentProvider, ClaudeProvider};
use maestro::templates::render_for_provider;

const COMMANDS: &[&str] = &["implement", "pushup", "plan-feature", "simplify"];

fn main() -> anyhow::Result<()> {
    let provider = ClaudeProvider::default();
    let Some(target_dir) = provider.template_rules().target_dir() else {
        anyhow::bail!(
            "provider `{}` has no target_dir; nothing to regenerate",
            provider.id()
        );
    };
    std::fs::create_dir_all(target_dir)
        .with_context(|| format!("create_dir_all `{}`", target_dir.display()))?;
    for command in COMMANDS {
        let rendered = render_for_provider(&provider, command)
            .with_context(|| format!("render `{command}`"))?;
        let path = target_dir.join(format!("{command}.md"));
        std::fs::write(&path, &rendered).with_context(|| format!("write `{}`", path.display()))?;
        println!("wrote {} ({} bytes)", path.display(), rendered.len());
    }
    Ok(())
}
