//! Token / USD cost estimate for the wizard's plan preview.
//!
//! See `docs/superpowers/specs/2026-05-05-orchestration-wizard-design.md` §7.
//! The displayed value is labelled "≈ $X.XX (rough estimate, ±50%)" — values
//! here are intentionally approximate. Per-token rates are snapshotted from
//! provider pricing pages and refreshed on maestro release, never at runtime.

#![allow(dead_code)]

use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::TeamRole;
use std::collections::HashMap;
use std::sync::OnceLock;

/// L2 system-prompt token allowance — see `build_system_prompt`.
pub const L2_SYSTEM_PROMPT_TOKENS: usize = 200;

/// Average issue-context tokens per provider (input excluding system prompt).
pub const AVG_ISSUE_CONTEXT_TOKENS_PER_PROVIDER: usize = 800;

/// Maximum recovery attempts per role per issue (spec §8 budget).
pub const RECOVERY_BUDGET: usize = 3;

/// Tokens per recovery iteration per role (spec §7 formula coefficient).
pub const RECOVERY_TOKENS_PER_ROLE: usize = 300;

/// Token-rate table keyed by canonical agent_id. USD per token.
///
/// Sources (snapshotted; refresh on quarterly review):
/// - claude:   anthropic.com/pricing      — Sonnet blended ~ $6 / 1M
/// - codex:    openai.com/api/pricing/    — gpt blended    ~ $8 / 1M
/// - qwen:     dashscope coder-32b        ~ $1.4 / 1M blended
/// - opencode: openrouter mid-tier OSS    ~ $1   / 1M blended
/// - minimax:  minimax.io M2.7            ~ $1.5 / 1M blended
/// - ollama:   local inference            $0
fn rates() -> &'static HashMap<&'static str, f64> {
    static RATES: OnceLock<HashMap<&'static str, f64>> = OnceLock::new();
    RATES.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("claude", 6.0e-6);
        m.insert("codex", 8.0e-6);
        m.insert("qwen", 1.4e-6);
        m.insert("opencode", 1.0e-6);
        m.insert("minimax", 1.5e-6);
        m.insert("ollama", 0.0);
        m
    })
}

/// Estimate the total tokens consumed by running `team` against `num_issues`,
/// where `avg_role_prompt_tokens` is the average size of a role-specific
/// system prompt (from `[modes.*]`).
///
/// Formula (spec §7):
/// ```text
/// per_issue = L2_SYSTEM_PROMPT_TOKENS
///           + (avg_role_prompt_tokens * num_required_roles)
///           + AVG_ISSUE_CONTEXT_TOKENS_PER_PROVIDER
///           + (RECOVERY_TOKENS_PER_ROLE * num_required_roles * RECOVERY_BUDGET)
/// ```
pub fn estimate_tokens(
    team: &ResolvedTeam,
    num_issues: usize,
    avg_role_prompt_tokens: usize,
) -> u64 {
    let num_required_roles = team.primitive.required_roles().len();
    let per_issue = L2_SYSTEM_PROMPT_TOKENS
        + (avg_role_prompt_tokens * num_required_roles)
        + AVG_ISSUE_CONTEXT_TOKENS_PER_PROVIDER
        + (RECOVERY_TOKENS_PER_ROLE * num_required_roles * RECOVERY_BUDGET);

    (num_issues as u64) * (per_issue as u64)
}

/// Estimate USD cost for the plan. L2 always routes to Claude; subagent
/// tokens are split equally across the team's required roles and routed to
/// each role's bound agent.
pub fn estimate_cost_usd(
    team: &ResolvedTeam,
    num_issues: usize,
    avg_role_prompt_tokens: usize,
) -> f64 {
    let rates = rates();
    let claude_rate = *rates.get("claude").unwrap_or(&0.0);
    let l2_cost = (num_issues as f64) * (L2_SYSTEM_PROMPT_TOKENS as f64) * claude_rate;

    let required = team.primitive.required_roles();
    if required.is_empty() {
        return l2_cost;
    }

    let denom = required.len();
    let subagent_tokens_per_role_per_issue = avg_role_prompt_tokens
        + (AVG_ISSUE_CONTEXT_TOKENS_PER_PROVIDER / denom)
        + (RECOVERY_TOKENS_PER_ROLE * RECOVERY_BUDGET);

    let mut subagent_cost = 0.0_f64;
    for role in required {
        let agent = primary_agent_for_role(team, *role).unwrap_or("claude");
        let rate = *rates.get(agent).unwrap_or(&0.0);
        subagent_cost += (num_issues as f64) * (subagent_tokens_per_role_per_issue as f64) * rate;
    }

    l2_cost + subagent_cost
}

/// USD-per-token rate for a canonical agent_id. `None` if the agent isn't in
/// the rate table.
pub fn primary_rate_for_agent(agent_id: &str) -> Option<f64> {
    rates().get(agent_id).copied()
}

fn primary_agent_for_role(team: &ResolvedTeam, role: TeamRole) -> Option<&str> {
    team.bindings
        .get(&role)
        .map(|b| b.agent.as_str())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{ResolvedTeam, RoleBinding, SourceTier};
    use crate::orchestration::types::Primitive;
    use std::collections::HashMap;

    fn binding(agent: &str) -> RoleBinding {
        RoleBinding {
            agent: agent.into(),
            ..Default::default()
        }
    }

    fn pipeline_team_with(impl_a: &str, rev_a: &str, docs_a: &str) -> ResolvedTeam {
        let mut bindings = HashMap::new();
        bindings.insert(TeamRole::Implementer, binding(impl_a));
        bindings.insert(TeamRole::Reviewer, binding(rev_a));
        bindings.insert(TeamRole::Docs, binding(docs_a));
        ResolvedTeam {
            name: "test-pipeline".into(),
            primitive: Primitive::Pipeline,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::BuiltIn,
        }
    }

    fn pipeline_team() -> ResolvedTeam {
        pipeline_team_with("claude", "claude", "claude")
    }

    fn single_pass_team() -> ResolvedTeam {
        ResolvedTeam {
            name: "test-single".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into()],
            bindings: HashMap::new(),
            source_tier: SourceTier::BuiltIn,
        }
    }

    #[test]
    fn estimate_tokens_grows_linearly_with_num_issues() {
        let one = estimate_tokens(&pipeline_team(), 1, 100);
        let two = estimate_tokens(&pipeline_team(), 2, 100);
        assert_eq!(two, 2 * one);
    }

    #[test]
    fn estimate_tokens_zero_for_zero_issues() {
        assert_eq!(estimate_tokens(&pipeline_team(), 0, 100), 0);
    }

    #[test]
    fn estimate_tokens_pipeline_greater_than_single_pass() {
        let pipeline = estimate_tokens(&pipeline_team(), 3, 100);
        let single = estimate_tokens(&single_pass_team(), 3, 100);
        assert!(
            pipeline > single,
            "pipeline ({pipeline}) must exceed single-pass ({single})"
        );
    }

    #[test]
    fn estimate_cost_usd_ollama_subagents_only_l2_floor() {
        let team = pipeline_team_with("ollama", "ollama", "ollama");
        let cost = estimate_cost_usd(&team, 1, 100);
        assert!(cost > 0.0, "L2 Claude floor must be nonzero: {cost}");
        // 1 issue × 200 L2 tokens × $6e-6/token = $0.0012 — sub-cent per issue.
        // The threshold honours spec §7 "±50%" rough-estimate language.
        assert!(cost < 0.01, "ollama subagents ⇒ sub-cent per issue: {cost}");
    }

    #[test]
    fn estimate_cost_usd_mixed_team_positive() {
        let cost = estimate_cost_usd(&pipeline_team(), 3, 100);
        assert!(
            cost > 0.001,
            "all-Claude pipeline must exceed sub-cent: {cost}"
        );
    }

    #[test]
    fn estimate_cost_usd_zero_issues_zero_cost() {
        assert_eq!(estimate_cost_usd(&pipeline_team(), 0, 100), 0.0);
    }

    #[test]
    fn cost_constants_are_nonzero() {
        assert!(L2_SYSTEM_PROMPT_TOKENS > 0);
        assert!(AVG_ISSUE_CONTEXT_TOKENS_PER_PROVIDER > 0);
        assert!(RECOVERY_BUDGET > 0);
        assert!(RECOVERY_TOKENS_PER_ROLE > 0);
    }

    #[test]
    fn rates_table_includes_all_v0_25_providers() {
        for id in &["claude", "codex", "qwen", "opencode", "minimax", "ollama"] {
            assert!(
                primary_rate_for_agent(id).is_some(),
                "missing rate for provider: {id}"
            );
        }
        assert_eq!(primary_rate_for_agent("ollama"), Some(0.0));
    }
}
