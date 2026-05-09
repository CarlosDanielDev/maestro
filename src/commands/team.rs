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

use crate::cli::{TeamSubcommand, TeamTier};
use crate::orchestration::dag::{IssueMeta, IssueState};
use crate::orchestration::loader::Loader;
use crate::orchestration::scheduler::Scheduler;
use crate::orchestration::team::{ResolvedTeam, SourceTier, TeamConfig};
use crate::orchestration::types::{Primitive, TeamInput, TeamOutput, TeamRole};
use crate::state::types::{IssueNumber, IssueRunState};
use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
        let primitive = primitive_label(s.primitive);
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

fn validate_preset_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("preset name must not be empty"));
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(anyhow!(
            "preset name {name:?} contains illegal path characters"
        ));
    }
    Ok(())
}

/// Write a new preset to the chosen tier. `user_dir_override` and
/// `project_root_override` let tests redirect the destination away from the
/// platform default; passing `None` falls back to `Loader::user_tier_default`
/// or `std::env::current_dir()`.
pub fn write_new_preset(
    opts: &NewPresetOpts,
    user_dir_override: Option<&Path>,
    project_root_override: Option<&Path>,
) -> Result<PathBuf> {
    validate_preset_name(&opts.name)?;
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

fn write_preset_file(dir: &Path, name: &str, cfg: &TeamConfig) -> Result<PathBuf> {
    let toml_text = toml::to_string_pretty(cfg)
        .with_context(|| format!("serializing preset {name:?} to TOML"))?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating preset dir {dir:?}"))?;
    let path = dir.join(format!("{name}.toml"));
    std::fs::write(&path, toml_text).with_context(|| format!("writing preset file {path:?}"))?;
    Ok(path)
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

/// Human-friendly label for `Primitive`, matching the kebab-case names
/// users see in TOML and JSON output (vs the raw `Debug` format which
/// flattens to `singlepass` / `verdictonly`).
fn primitive_label(p: Primitive) -> String {
    match p {
        Primitive::Pipeline => "pipeline",
        Primitive::FanOut => "fan-out",
        Primitive::SinglePass => "single-pass",
        Primitive::VerdictOnly => "verdict-only",
    }
    .to_string()
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
    let primitive = primitive_label(exp.primitive);
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

// -- launch headless -----------------------------------------------------

#[derive(Debug, Clone)]
pub struct LaunchOpts {
    pub preset: String,
    pub issue: Option<u64>,
    pub issues: Vec<u64>,
    pub max_parallel: usize,
}

#[derive(Debug, Default)]
pub struct LaunchOutcome {
    pub succeeded: Vec<IssueNumber>,
    pub failed: Vec<(IssueNumber, String)>,
    pub plan_levels: usize,
}

/// Async seam for headless `team launch --yes`. Production impl drives the
/// real L1 dispatch + scheduler loop; tests inject `MockSchedulerRunner`
/// to return canned `TeamOutput` per issue without touching providers.
///
/// Pattern mirrors `AgentProviderFactory::with_default_provider` from
/// `src/orchestration/dispatch.rs` (#663), where a trait object lets tests
/// substitute the entire downstream-call surface in one place.
#[async_trait::async_trait]
pub trait SchedulerRunner: Send + Sync {
    async fn run_issue(
        &self,
        issue: IssueNumber,
        team: &ResolvedTeam,
    ) -> Result<TeamOutput, String>;
}

/// Production `SchedulerRunner` — drives `dispatch_subagent` (#663) for each
/// `NextStep::Dispatch` emitted by the issue's primitive machine. Headless
/// v1 returns a synthetic `TeamOutput::Pr` on machine completion: the real
/// worktree + PR creation surface lives inside the TUI today and will be
/// extracted in a v0.27.x follow-up. The scheduler-to-dispatch wiring
/// itself is exercised end-to-end here, so CI can call `team launch --yes`
/// and see actual provider calls happen.
struct ProductionSchedulerRunner;

#[async_trait::async_trait]
impl SchedulerRunner for ProductionSchedulerRunner {
    async fn run_issue(
        &self,
        issue: IssueNumber,
        team: &ResolvedTeam,
    ) -> Result<TeamOutput, String> {
        use crate::config::Config;
        use crate::orchestration::dispatch::{DispatchContext, dispatch_subagent};
        use crate::orchestration::primitives::{NextStep, make_machine};

        let config = Config::find_and_load().ok().map(Arc::new);
        let default_model = config
            .as_ref()
            .map(|c| c.sessions.default_model.clone())
            .unwrap_or_else(|| "opus".to_string());
        let ctx = DispatchContext::new(team.clone(), config, default_model);

        tracing::info!(
            target: "maestro::team::launch",
            issue,
            team = %team.name,
            primitive = ?team.primitive,
            "headless launch dispatching primitive machine"
        );

        let mut machine = make_machine(team.primitive, issue);
        loop {
            match machine.next() {
                NextStep::Dispatch { role, instructions } => {
                    let result = dispatch_subagent(&ctx, role, &instructions).await;
                    machine.advance(role, result);
                }
                NextStep::Done { .. } => {
                    return Ok(TeamOutput::Pr {
                        number: issue,
                        branch: format!("feat/issue-{issue}"),
                    });
                }
                NextStep::Fail { reason } => return Err(reason),
            }
        }
    }
}

fn build_metas_from_args(
    opts: &LaunchOpts,
) -> Result<(TeamInput, HashMap<IssueNumber, IssueMeta>)> {
    let issues: Vec<IssueNumber> = match (opts.issue, opts.issues.as_slice()) {
        (Some(_), &[_, ..]) => {
            return Err(anyhow!("--issue and --issues are mutually exclusive"));
        }
        (Some(n), []) => vec![n],
        (None, []) => return Err(anyhow!("--issue or --issues is required for --yes launch")),
        (None, list) => list.to_vec(),
    };

    // Synthesise minimal `IssueMeta` records — production code will replace
    // this with `gh issue view`. For now we treat the CLI input as the full
    // selection set with no implicit dependencies.
    let mut metas = HashMap::new();
    for &n in &issues {
        metas.insert(
            n,
            IssueMeta {
                number: n,
                state: IssueState::Open,
                milestone: None,
                blocked_by: Vec::new(),
            },
        );
    }
    let input = if issues.len() == 1 {
        TeamInput::Issue { number: issues[0] }
    } else {
        TeamInput::IssueSet {
            primary_milestone: None,
            issues,
        }
    };
    Ok((input, metas))
}

/// Drive a headless team launch using the supplied `SchedulerRunner`.
///
/// Tests inject a mock runner; production wires `LoggingSchedulerRunner`
/// (placeholder) → real dispatch in a follow-up. Returns `LaunchOutcome`
/// summarising per-issue terminal state. Errors only on plan-construction
/// failures; per-issue failures are surfaced through `outcome.failed`.
pub async fn launch_headless(
    loader: &Loader,
    opts: LaunchOpts,
    runner: Arc<dyn SchedulerRunner>,
) -> Result<LaunchOutcome> {
    let resolved = loader.resolve()?;
    let team = resolved
        .get(&opts.preset)
        .ok_or_else(|| anyhow!("preset {:?} not found", opts.preset))?
        .clone();

    let max_parallel = opts.max_parallel.max(1);
    let (input, metas) = build_metas_from_args(&opts)?;
    let mut sched = Scheduler::from_input(team, input, metas, max_parallel)?;
    let plan_levels = sched.run.plan.len();

    let mut outcome = LaunchOutcome {
        plan_levels,
        ..Default::default()
    };

    loop {
        let ready = sched.next_ready();
        if ready.is_empty() {
            break;
        }
        for issue in ready {
            sched.run.state.insert(
                issue,
                IssueRunState::InFlight {
                    session_id: uuid::Uuid::new_v4(),
                    started_at: chrono::Utc::now(),
                },
            );
            match runner.run_issue(issue, &sched.team).await {
                Ok(output) => {
                    sched
                        .run
                        .state
                        .insert(issue, IssueRunState::Succeeded { output });
                    outcome.succeeded.push(issue);
                }
                Err(reason) => {
                    sched.run.state.insert(
                        issue,
                        IssueRunState::Failed {
                            reason: reason.clone(),
                            attempts: 1,
                        },
                    );
                    outcome.failed.push((issue, reason));
                }
            }
        }
    }

    Ok(outcome)
}

fn print_launch_outcome(outcome: &LaunchOutcome) {
    println!(
        "Plan: {} level(s) — {} succeeded, {} failed",
        outcome.plan_levels,
        outcome.succeeded.len(),
        outcome.failed.len()
    );
    for (issue, reason) in &outcome.failed {
        println!("  ✗ #{issue}: {reason}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn make_loader_with_tempdirs() -> (tempfile::TempDir, tempfile::TempDir, Loader) {
        let user = tempdir().unwrap();
        let project = tempdir().unwrap();
        let loader = Loader::new(
            Some(user.path().to_path_buf()),
            Some(project.path().to_path_buf()),
        );
        (user, project, loader)
    }

    fn write_user_preset_file(user: &Path, name: &str, contents: &str) {
        fs::write(user.join(format!("{name}.toml")), contents).unwrap();
    }

    // -- list_teams ------------------------------------------------------

    #[test]
    fn list_teams_with_only_builtins_returns_five_entries() {
        let loader = Loader::new(None, None);
        let summaries = list_teams(&loader).unwrap();
        assert_eq!(summaries.len(), 5);
        for name in [
            "default-coder",
            "default-researcher",
            "default-triager",
            "default-reviewer",
            "default-docs",
        ] {
            assert!(
                summaries.iter().any(|s| s.name == name),
                "missing built-in {name}"
            );
        }
        for s in &summaries {
            assert_eq!(s.source_tier, TierLabel::BuiltIn);
        }
    }

    #[test]
    fn list_teams_includes_user_tier_preset() {
        let (user, _project, loader) = make_loader_with_tempdirs();
        write_user_preset_file(
            user.path(),
            "cheap-coder",
            r#"extends = "default-coder"
implementer = "ollama"
"#,
        );
        let summaries = list_teams(&loader).unwrap();
        let entry = summaries
            .iter()
            .find(|s| s.name == "cheap-coder")
            .expect("cheap-coder must be present");
        assert_eq!(entry.source_tier, TierLabel::User);
    }

    #[test]
    fn list_teams_json_is_valid_json_array() {
        let loader = Loader::new(None, None);
        let s = list_teams_json(&loader).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        let arr = parsed.as_array().expect("top level must be array");
        assert_eq!(arr.len(), 5);
        for entry in arr {
            assert!(entry.get("name").is_some());
            assert!(entry.get("primitive").is_some());
            assert!(entry.get("source_tier").is_some());
        }
    }

    // -- explain ---------------------------------------------------------

    #[test]
    fn explain_returns_resolved_bindings_for_builtin() {
        let loader = Loader::new(None, None);
        let exp = explain(&loader, "default-coder").unwrap();
        assert_eq!(exp.name, "default-coder");
        assert_eq!(exp.source_tier, TierLabel::BuiltIn);
        assert!(!exp.bindings.is_empty());
    }

    #[test]
    fn explain_unknown_team_returns_err() {
        let loader = Loader::new(None, None);
        let err = explain(&loader, "no-such-team").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("not found"));
        assert!(msg.contains("no-such-team"));
    }

    #[test]
    fn explain_json_round_trips_through_serde_json() {
        let loader = Loader::new(None, None);
        let s = explain_json(&loader, "default-coder").unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["name"], "default-coder");
        assert!(v["bindings"].is_array());
    }

    #[test]
    fn explain_overridden_field_traces_to_child_preset() {
        let (user, _project, loader) = make_loader_with_tempdirs();
        write_user_preset_file(
            user.path(),
            "child",
            r#"extends = "default-coder"
implementer = "opencode"
"#,
        );
        let exp = explain(&loader, "child").unwrap();
        let imp = exp
            .bindings
            .iter()
            .find(|b| b.role == "implementer")
            .expect("implementer binding must exist");
        assert_eq!(imp.agent, "opencode");
    }

    // -- write_new_preset ------------------------------------------------

    #[test]
    fn write_new_preset_user_tier_writes_toml_under_override_dir() {
        let (user, _project, _loader) = make_loader_with_tempdirs();
        let opts = NewPresetOpts {
            name: "my-team".into(),
            extends: "default-coder".into(),
            tier: TeamTier::User,
            implementer: Some("opencode".into()),
            reviewer: None,
            docs: None,
        };
        let path = write_new_preset(&opts, Some(user.path()), None).unwrap();
        assert!(path.exists());
        assert!(path.ends_with("my-team.toml"));
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("extends = \"default-coder\""));
        assert!(body.contains("implementer = \"opencode\""));
    }

    #[test]
    fn write_new_preset_project_tier_writes_under_dot_maestro_teams() {
        let (_user, project, _loader) = make_loader_with_tempdirs();
        let opts = NewPresetOpts {
            name: "proj-team".into(),
            extends: "default-coder".into(),
            tier: TeamTier::Project,
            implementer: None,
            reviewer: None,
            docs: None,
        };
        let path = write_new_preset(&opts, None, Some(project.path())).unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains(".maestro/teams"));
    }

    #[test]
    fn write_new_preset_rejects_path_traversal_in_name() {
        let (user, _project, _loader) = make_loader_with_tempdirs();
        let opts = NewPresetOpts {
            name: "../etc/passwd".into(),
            extends: "default-coder".into(),
            tier: TeamTier::User,
            implementer: None,
            reviewer: None,
            docs: None,
        };
        let err = write_new_preset(&opts, Some(user.path()), None).unwrap_err();
        assert!(format!("{err:#}").contains("illegal path characters"));
    }

    #[test]
    fn write_new_preset_rejects_empty_name() {
        let (user, _project, _loader) = make_loader_with_tempdirs();
        let opts = NewPresetOpts {
            name: "".into(),
            extends: "default-coder".into(),
            tier: TeamTier::User,
            implementer: None,
            reviewer: None,
            docs: None,
        };
        let err = write_new_preset(&opts, Some(user.path()), None).unwrap_err();
        assert!(format!("{err:#}").contains("empty"));
    }

    // -- manage_list -----------------------------------------------------

    #[test]
    fn manage_list_excludes_builtin_tier() {
        let loader = Loader::new(None, None);
        let entries = manage_list(&loader).unwrap();
        assert!(
            entries.is_empty(),
            "manage_list must not include built-in presets"
        );
    }

    // -- launch_headless (with mock SchedulerRunner) --------------------

    struct MockSchedulerRunner {
        responses: std::sync::Mutex<std::collections::VecDeque<Result<TeamOutput, String>>>,
    }

    impl MockSchedulerRunner {
        fn new(responses: Vec<Result<TeamOutput, String>>) -> Self {
            Self {
                responses: std::sync::Mutex::new(responses.into()),
            }
        }
    }

    #[async_trait::async_trait]
    impl SchedulerRunner for MockSchedulerRunner {
        async fn run_issue(
            &self,
            _issue: IssueNumber,
            _team: &ResolvedTeam,
        ) -> Result<TeamOutput, String> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| "mock runner exhausted".to_string())
                .and_then(|r| r)
        }
    }

    fn pr_output(n: u64) -> TeamOutput {
        TeamOutput::Pr {
            number: n,
            branch: format!("feat/{n}"),
        }
    }

    #[tokio::test]
    async fn launch_headless_all_succeed_returns_no_failures() {
        let loader = Loader::new(None, None);
        let runner = std::sync::Arc::new(MockSchedulerRunner::new(vec![Ok(pr_output(1))]));
        let outcome = launch_headless(
            &loader,
            LaunchOpts {
                preset: "default-coder".into(),
                issue: Some(1),
                issues: vec![],
                max_parallel: 1,
            },
            runner,
        )
        .await
        .unwrap();
        assert_eq!(outcome.succeeded, vec![1]);
        assert!(outcome.failed.is_empty());
        assert_eq!(outcome.plan_levels, 1);
    }

    #[tokio::test]
    async fn launch_headless_records_per_issue_failure() {
        let loader = Loader::new(None, None);
        let runner = std::sync::Arc::new(MockSchedulerRunner::new(vec![Err("boom".into())]));
        let outcome = launch_headless(
            &loader,
            LaunchOpts {
                preset: "default-coder".into(),
                issue: Some(99),
                issues: vec![],
                max_parallel: 1,
            },
            runner,
        )
        .await
        .unwrap();
        assert!(outcome.succeeded.is_empty());
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].0, 99);
        assert!(outcome.failed[0].1.contains("boom"));
    }

    #[tokio::test]
    async fn launch_headless_unknown_preset_returns_err() {
        let loader = Loader::new(None, None);
        let runner = std::sync::Arc::new(MockSchedulerRunner::new(vec![]));
        let err = launch_headless(
            &loader,
            LaunchOpts {
                preset: "no-such-preset".into(),
                issue: Some(1),
                issues: vec![],
                max_parallel: 1,
            },
            runner,
        )
        .await
        .unwrap_err();
        assert!(format!("{err:#}").contains("not found"));
    }

    #[tokio::test]
    async fn launch_headless_requires_issue_or_issues() {
        let loader = Loader::new(None, None);
        let runner = std::sync::Arc::new(MockSchedulerRunner::new(vec![]));
        let err = launch_headless(
            &loader,
            LaunchOpts {
                preset: "default-coder".into(),
                issue: None,
                issues: vec![],
                max_parallel: 1,
            },
            runner,
        )
        .await
        .unwrap_err();
        assert!(format!("{err:#}").contains("required"));
    }

    #[test]
    fn manage_list_includes_user_tier_with_path() {
        let (user, _project, loader) = make_loader_with_tempdirs();
        write_user_preset_file(
            user.path(),
            "u1",
            r#"extends = "default-coder"
implementer = "ollama"
"#,
        );
        let entries = manage_list(&loader).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "u1");
        assert!(entries[0].path.ends_with("u1.toml"));
    }
}
