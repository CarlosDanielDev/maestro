//! `maestro team` subcommand handlers.
//!
//! See spec `docs/superpowers/specs/2026-05-05-orchestration-wizard-design.md`
//! and issue #665. The CLI surface itself is in `src/cli_team.rs` (kept
//! self-contained so `build.rs` can include it for man/completion generation);
//! this module wires the surface to the orchestration loader, scheduler, and
//! state store.
//!
//! ## Layout
//!
//! - `dispatch(action)` is the entry point called from `main.rs`.
//! - Per-subcommand helpers (`list_teams`, `new_preset`, `launch_headless`,
//!   `manage_list`, `explain`) are library-callable so tests can drive them
//!   with a tempdir-injected `Loader` rather than the platform default.
//! - `SchedulerRunner` is the seam used by `--yes` headless launches; the
//!   production impl wires real L1 dispatch, while tests inject
//!   `MockSchedulerRunner` (mirrors the `AgentProviderFactory::with_default_provider`
//!   pattern from #663 in `src/orchestration/dispatch.rs`).

#![allow(dead_code)]

#[path = "team_launch.rs"]
mod team_launch;

pub use team_launch::{LaunchOpts, SchedulerRunner, launch_headless};

use crate::cli::{TeamSubcommand, TeamTier};
use crate::orchestration::loader::{Loader, write_preset_file};
use crate::orchestration::team::{ResolvedTeam, SourceTier, TeamConfig};
use crate::orchestration::types::{Primitive, TeamRole};
use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use team_launch::{ProductionSchedulerRunner, print_launch_outcome};

/// Entry point for `maestro team ...`. Routes each subcommand to its handler.
pub async fn dispatch(action: TeamSubcommand) -> Result<()> {
    let loader = Loader::default_for_cwd();
    match action {
        TeamSubcommand::List { json } => cmd_list(&loader, json),
        TeamSubcommand::New {
            name,
            extends,
            tier,
            implementer,
            reviewer,
            docs,
        } => cmd_new(NewPresetOpts {
            name,
            extends,
            tier,
            implementer,
            reviewer,
            docs,
        }),
        TeamSubcommand::Launch {
            preset,
            issue,
            issues,
            yes,
            max_parallel,
        } => {
            if !yes {
                // Interactive launch lives behind the TUI wizard (#664).
                // From the CLI without `--yes`, we redirect rather than
                // open a TUI session — keeps `team launch` predictable
                // for scripting.
                return Err(anyhow!(
                    "interactive launch lives in the TUI wizard — pass --yes for the headless plan run, or open the wizard with `maestro` then `[t]`"
                ));
            }
            let opts = LaunchOpts {
                preset,
                issue,
                issues,
                max_parallel,
            };
            let runner = Arc::new(ProductionSchedulerRunner) as Arc<dyn SchedulerRunner>;
            let outcome = launch_headless(&loader, opts, runner).await?;
            print_launch_outcome(&outcome);
            if outcome.failed.is_empty() {
                Ok(())
            } else {
                Err(anyhow!(
                    "{} issue(s) did not reach Succeeded",
                    outcome.failed.len()
                ))
            }
        }
        TeamSubcommand::Manage { list } => cmd_manage(&loader, list),
        TeamSubcommand::Explain { name, json } => cmd_explain(&loader, &name, json),
    }
}

// -- list ----------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct TeamSummary {
    pub name: String,
    pub primitive: Primitive,
    pub source_tier: TierLabel,
    pub min_agents: Vec<String>,
    pub roles: Vec<TeamRole>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TierLabel {
    BuiltIn,
    User,
    Project,
}

impl From<SourceTier> for TierLabel {
    fn from(t: SourceTier) -> Self {
        match t {
            SourceTier::BuiltIn => TierLabel::BuiltIn,
            SourceTier::User => TierLabel::User,
            SourceTier::Project => TierLabel::Project,
        }
    }
}

pub fn list_teams(loader: &Loader) -> Result<Vec<TeamSummary>> {
    let resolved = loader.resolve()?;
    let mut out: Vec<TeamSummary> = resolved
        .into_iter()
        .map(|(name, t)| TeamSummary {
            name,
            primitive: t.primitive,
            source_tier: t.source_tier.into(),
            min_agents: t.min_agents,
            roles: {
                let mut roles: Vec<TeamRole> = t.bindings.keys().copied().collect();
                roles.sort_by_key(|r| format!("{r:?}"));
                roles
            },
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn list_teams_json(loader: &Loader) -> Result<String> {
    let summaries = list_teams(loader)?;
    serde_json::to_string_pretty(&summaries).context("serializing team list to JSON")
}

fn cmd_list(loader: &Loader, json: bool) -> Result<()> {
    if json {
        println!("{}", list_teams_json(loader)?);
        return Ok(());
    }
    let summaries = list_teams(loader)?;
    println!("{:<24} {:<12} {:<14} ROLES", "NAME", "TIER", "PRIMITIVE");
    for s in &summaries {
        let tier = match s.source_tier {
            TierLabel::BuiltIn => "built-in",
            TierLabel::User => "user",
            TierLabel::Project => "project",
        };
        let primitive = s.primitive.label();
        let roles = s
            .roles
            .iter()
            .map(|r| format!("{r:?}").to_lowercase())
            .collect::<Vec<_>>()
            .join(",");
        println!("{:<24} {:<12} {:<14} {}", s.name, tier, primitive, roles);
    }
    Ok(())
}

// -- new -----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NewPresetOpts {
    pub name: String,
    pub extends: String,
    pub tier: TeamTier,
    pub implementer: Option<String>,
    pub reviewer: Option<String>,
    pub docs: Option<String>,
}

/// Build a `TeamConfig` from new-preset options, applying any role overrides.
fn build_preset_config(opts: &NewPresetOpts) -> TeamConfig {
    let mut bindings: HashMap<String, toml::Value> = HashMap::new();
    if let Some(a) = &opts.implementer {
        bindings.insert("implementer".into(), toml::Value::String(a.clone()));
    }
    if let Some(a) = &opts.reviewer {
        bindings.insert("reviewer".into(), toml::Value::String(a.clone()));
    }
    if let Some(a) = &opts.docs {
        bindings.insert("docs".into(), toml::Value::String(a.clone()));
    }
    TeamConfig {
        extends: opts.extends.clone(),
        primitive: None,
        min_agents: None,
        bindings,
        role_overrides: HashMap::new(),
    }
}

/// Write a new preset to the chosen tier. `user_dir_override` and
/// `project_root_override` let tests redirect the destination away from the
/// platform default; passing `None` falls back to `Loader::user_tier_default`
/// or `std::env::current_dir()`. Name validation lives in `loader::write_preset_file`
/// (and `Loader::write_*_preset`), so all paths reject illegal names uniformly.
pub fn write_new_preset(
    opts: &NewPresetOpts,
    user_dir_override: Option<&Path>,
    project_root_override: Option<&Path>,
) -> Result<PathBuf> {
    let cfg = build_preset_config(opts);
    match opts.tier {
        TeamTier::User => match user_dir_override {
            Some(dir) => write_preset_file(dir, &opts.name, &cfg),
            None => Loader::write_user_preset(&opts.name, &cfg),
        },
        TeamTier::Project => {
            let root = match project_root_override {
                Some(p) => p.to_path_buf(),
                None => std::env::current_dir().context("determining project root")?,
            };
            Loader::write_project_preset(&root, &opts.name, &cfg)
        }
    }
}

fn cmd_new(opts: NewPresetOpts) -> Result<()> {
    let path = write_new_preset(&opts, None, None)?;
    println!(
        "Wrote {} preset → {}",
        tier_label(opts.tier),
        path.display()
    );
    Ok(())
}

fn tier_label(tier: TeamTier) -> &'static str {
    match tier {
        TeamTier::User => "user",
        TeamTier::Project => "project",
    }
}

// -- manage --------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UserPresetEntry {
    pub name: String,
    pub path: PathBuf,
}

pub fn manage_list(loader: &Loader) -> Result<Vec<UserPresetEntry>> {
    let raw = loader.load_raw()?;
    let mut out: Vec<UserPresetEntry> = raw
        .values()
        .filter(|t| t.source_tier == SourceTier::User)
        .filter_map(|t| {
            t.source_path.as_ref().map(|p| UserPresetEntry {
                name: t.name.clone(),
                path: p.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn cmd_manage(loader: &Loader, list: bool) -> Result<()> {
    if !list {
        return Err(anyhow!(
            "interactive `team manage` not yet implemented — pass --list for now"
        ));
    }
    let entries = manage_list(loader)?;
    if entries.is_empty() {
        println!("(no user-tier presets)");
        return Ok(());
    }
    for entry in entries {
        println!("{:<24} {}", entry.name, entry.path.display());
    }
    Ok(())
}

// -- explain -------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct TeamExplanation {
    pub name: String,
    pub source_tier: TierLabel,
    pub primitive: Primitive,
    pub min_agents: Vec<String>,
    pub bindings: Vec<RoleBindingSummary>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoleBindingSummary {
    pub role: String,
    pub agent: String,
    pub mode: Option<String>,
    pub model_override: Option<String>,
    pub prompt_addendum: Option<String>,
    pub fallback_agent: Option<String>,
}

pub fn explain(loader: &Loader, name: &str) -> Result<TeamExplanation> {
    let resolved = loader.resolve()?;
    let team = resolved
        .get(name)
        .ok_or_else(|| anyhow!("team {name:?} not found"))?;
    Ok(team_to_explanation(team))
}

fn team_to_explanation(team: &ResolvedTeam) -> TeamExplanation {
    let mut bindings: Vec<RoleBindingSummary> = team
        .bindings
        .iter()
        .map(|(role, b)| RoleBindingSummary {
            role: format!("{role:?}").to_lowercase(),
            agent: b.agent.clone(),
            mode: b.mode.clone(),
            model_override: b.model_override.clone(),
            prompt_addendum: b.prompt_addendum.clone(),
            fallback_agent: b.fallback_agent.clone(),
        })
        .collect();
    bindings.sort_by(|a, b| a.role.cmp(&b.role));
    TeamExplanation {
        name: team.name.clone(),
        source_tier: team.source_tier.into(),
        primitive: team.primitive,
        min_agents: team.min_agents.clone(),
        bindings,
    }
}

pub fn explain_json(loader: &Loader, name: &str) -> Result<String> {
    serde_json::to_string_pretty(&explain(loader, name)?).context("serializing explanation to JSON")
}

fn cmd_explain(loader: &Loader, name: &str, json: bool) -> Result<()> {
    if json {
        println!("{}", explain_json(loader, name)?);
        return Ok(());
    }
    let exp = explain(loader, name)?;
    let tier = match exp.source_tier {
        TierLabel::BuiltIn => "built-in",
        TierLabel::User => "user",
        TierLabel::Project => "project",
    };
    let primitive = exp.primitive.label();
    println!("Team: {} ({tier})", exp.name);
    println!("  Primitive: {primitive}");
    println!("  min_agents: {:?}", exp.min_agents);
    for b in &exp.bindings {
        println!("  {:<14} agent = {}", b.role, b.agent);
        if let Some(m) = &b.mode {
            println!("  {:<14} mode  = {m}", "");
        }
        if let Some(m) = &b.model_override {
            println!("  {:<14} model = {m}", "");
        }
        if let Some(p) = &b.prompt_addendum {
            println!("  {:<14} addn  = {p}", "");
        }
        if let Some(f) = &b.fallback_agent {
            println!("  {:<14} fbk   = {f}", "");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "team_tests.rs"]
mod tests;
