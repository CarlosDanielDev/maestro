use crate::commands::setup::{setup_app_for_agent, startup_cleanup};
use crate::config::Config;
use crate::provider::{create_provider, github::client::RepoProvider};
use crate::session::types::Session;
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::app::work_assigner::WorkAssignmentService;
use crate::work::assigner::WorkAssigner;
use crate::work::types::WorkItem;
use maestro_core::agent_provider_map::provider_for_agent;

#[path = "run_deps.rs"]
mod run_deps;

#[allow(clippy::too_many_arguments)]
pub async fn cmd_run(
    prompt: Option<String>,
    issue: Option<String>,
    milestone: Option<String>,
    model: Option<String>,
    agent: Option<String>,
    mode: Option<String>,
    max_concurrent_override: Option<usize>,
    resume: bool,
    skip_doctor: bool,
    images: Vec<std::path::PathBuf>,
    once: bool,
    continuous: bool,
    enable_flags: Vec<String>,
    disable_flags: Vec<String>,
    role_override: Option<crate::session::role::Role>,
    no_splash: bool,
    bypass_review: bool,
    force_quota: bool,
    ignore_deps: bool,
) -> anyhow::Result<()> {
    // `force_quota` is exposed to downstream provider gates via a process
    // -wide flag so it doesn't have to thread through every session spawn
    // path. MinimaxProvider reads the flag in its pre-spawn quota check.
    if force_quota {
        crate::agent_provider::minimax::set_force_quota();
    }
    let loaded = Config::find_and_load_with_path()?;
    let config = loaded.config.clone();

    // Workspace trust gate (#1045): `run` exists to execute the configured
    // commands — refuse outright when the repo config declares executable
    // values and the workspace is not trusted.
    crate::config::trust::refuse_untrusted_with_default_store(&loaded.path, &loaded.config)?;

    // Pre-cutoff nudge (#750): headless claude transport loses subscription
    // billing on 2026-06-15. Suppressible via MAESTRO_SILENCE_TRANSPORT_WARN=1.
    let silence_transport_warn = std::env::var("MAESTRO_SILENCE_TRANSPORT_WARN")
        .map(|v| v == "1")
        .unwrap_or(false);
    if let Some(msg) = crate::config::transport_warning::headless_cutoff_warning(
        &config.agents,
        &crate::config::transport_warning::SystemClock,
        silence_transport_warn,
    ) {
        tracing::warn!("{msg}");
    }

    let resolved_agent = config.resolve_agent(agent.as_deref())?;
    let selected_provider = provider_for_agent(&resolved_agent)?;

    let feature_flags = crate::flags::store::FeatureFlags::new(
        config.flags.entries.clone(),
        enable_flags,
        disable_flags,
    );

    if continuous && milestone.is_none() {
        tracing::warn!("--continuous has no effect without --milestone");
    }

    if !skip_doctor {
        let config_ref = config.clone();
        let report_agent = agent.clone();
        let report = tokio::task::spawn_blocking(move || {
            crate::doctor::run_all_checks_for_agent(Some(&config_ref), report_agent.as_deref())
        })
        .await?;
        let validation_config = config.clone();
        let validation_agent = agent.clone();
        let validation = tokio::task::spawn_blocking(move || {
            crate::doctor::validate_provider_setup_for_agent(
                &validation_config,
                validation_agent.as_deref(),
            )
        })
        .await?;
        if let Err(e) = validation {
            crate::doctor::print_report(&report);
            return Err(e.context("Fix the issues above or pass --skip-doctor to bypass"));
        }
        for check in report.failed_checks() {
            tracing::warn!("Doctor: {} — {}", check.name, check.message);
        }
    }

    for img in &images {
        crate::session::image::validate_image_path(img)?;
    }

    let model = model
        .or_else(|| resolved_agent.config.model.clone())
        .unwrap_or(config.sessions.default_model.clone());
    let session_mode = mode.unwrap_or(config.sessions.default_mode.clone());

    let store = StateStore::new(StateStore::default_path());
    let repo_root = std::env::current_dir()?;
    startup_cleanup(&repo_root);

    let issue_filter_labels = config.github.issue_filter_labels.clone();
    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root));

    let effective_max_concurrent = if continuous && milestone.is_some() {
        Some(1)
    } else {
        max_concurrent_override
    };

    let workspace_trusted =
        crate::config::trust::evaluate_with_default_store(&loaded.path, &loaded.config)
            == crate::config::trust::TrustDecision::Trusted;
    let mut app = setup_run_app(
        loaded,
        store,
        worktree_mgr,
        effective_max_concurrent,
        bypass_review,
        &resolved_agent,
        workspace_trusted,
    );
    app.pool.set_provider(selected_provider);
    app.flags = feature_flags;

    if resume {
        let mut recovered = 0;
        for session in &mut app.state.sessions {
            if matches!(
                session.status,
                crate::session::types::SessionStatus::Running
                    | crate::session::types::SessionStatus::Spawning
            ) {
                session.status = crate::session::types::SessionStatus::Errored;
                recovered += 1;
            }
        }
        if recovered > 0 {
            tracing::info!(
                "Resume: marked {} incomplete sessions as errored (retry will pick them up)",
                recovered
            );
        }
    }

    if let Some(prompt_text) = prompt {
        let mode_config = crate::modes::resolve_session_mode_config(&session_mode, Some(&config));
        let session = Session::new(
            prompt_text,
            model,
            session_mode.clone(),
            None,
            role_override,
        )
        .with_mode_config(mode_config)
        .with_agent_id(Some(resolved_agent.id.clone()))
        .with_image_paths(images.clone());
        app.add_session(session).await?;
    } else if let Some(milestone_name) = milestone {
        let provider_config = config.effective_provider_config();
        let client = create_provider(&provider_config)?;
        let issues = client.list_issues_by_milestone(&milestone_name).await?;
        if issues.is_empty() {
            anyhow::bail!("No open issues found in milestone '{}'", milestone_name);
        }

        let items: Vec<WorkItem> = issues.into_iter().map(WorkItem::from_issue).collect();
        let assigner = WorkAssigner::new(items);

        app.work_assignment_service = Some(WorkAssignmentService::new(assigner));
        app.github_client = Some(std::sync::Arc::from(client));

        if continuous && app.flags.is_enabled(crate::flags::Flag::ContinuousMode) {
            app.continuous_mode = Some(crate::continuous::ContinuousModeState::new());
            tracing::info!(
                "Continuous mode: processing milestone '{}' issues serially",
                milestone_name
            );
        } else if continuous {
            tracing::info!("--continuous ignored: Flag::ContinuousMode is disabled");
        }
    } else if let Some(issue_str) = issue {
        let provider_config = config.effective_provider_config();
        let client = create_provider(&provider_config)?;

        // #1141: every requested issue is fetched and dependency-gated before
        // anything is enqueued — a blocked issue later in the list must not
        // abandon agents already spawned for earlier ones. queue reports
        // blocked-by issues as Blocked; refuse unless explicitly overridden.
        let requested =
            run_deps::resolve_issue_list(client.as_ref(), &issue_str, ignore_deps).await?;
        // #1156: this path skips the QueueConfirmation review, so warn here.
        if let Some(warning) = run_deps::launch_scope_warning(&requested) {
            eprintln!("warning: {warning}");
            let level = crate::tui::activity_log::LogLevel::Warn;
            app.activity_log.push_simple("run".into(), warning, level);
        }

        for gh_issue in requested {
            let num = gh_issue.number;
            let (issue_mode, mode_config) = crate::modes::resolve_mode_for_labels(
                &gh_issue.labels,
                &session_mode,
                Some(&config),
            );
            let prompt = crate::prompts::PromptBuilder::build_issue_prompt_with_images(
                &gh_issue, &config, &images,
            );
            let user_text =
                crate::session::role::user_text_from_issue(&gh_issue.title, &gh_issue.body, None);
            let mut session = Session::new(prompt, model.clone(), issue_mode, Some(num), None)
                .with_role_from_user_text(role_override, &user_text)
                .with_mode_config(mode_config)
                .with_agent_id(Some(resolved_agent.id.clone()))
                .with_image_paths(images.clone());
            session.issue_title = Some(gh_issue.title.clone());

            app.state.issue_cache.insert(num, gh_issue);

            // Label lifecycle (#1140): apply the same ready -> in-progress
            // transition the work-assigner path applies on assignment.
            app.mark_issues_in_progress(client.as_ref(), &[num]).await;

            app.add_session(session).await?;
        }

        app.github_client = Some(std::sync::Arc::from(client));
    } else {
        let provider_config = config.effective_provider_config();
        let client = create_provider(&provider_config)?;
        let label_refs: Vec<&str> = issue_filter_labels.iter().map(|s| s.as_str()).collect();
        let issues = client.list_issues(&label_refs).await?;

        if issues.is_empty() {
            anyhow::bail!(
                "No issues found with labels {:?}. Use --prompt or --issue instead.",
                issue_filter_labels
            );
        }

        let items: Vec<WorkItem> = issues.into_iter().map(WorkItem::from_issue).collect();
        let assigner = WorkAssigner::new(items);

        app.work_assignment_service = Some(WorkAssignmentService::new(assigner));
        app.github_client = Some(std::sync::Arc::from(client));
    }

    app.once_mode = once;
    app.no_splash = no_splash;

    crate::tui::run(app).await
}

/// Build the run's App for its resolved agent (#1411). Setup sets the pool
/// mode and the ▲ BYPASS chip together from that agent, so nothing after
/// this may change the pool mode.
fn setup_run_app(
    loaded: crate::config::LoadedConfig,
    store: StateStore,
    worktree_mgr: Box<dyn crate::session::worktree::WorktreeManager + Send>,
    max_concurrent: Option<usize>,
    bypass_review: bool,
    resolved_agent: &crate::config::ResolvedAgentConfig,
    workspace_trusted: bool,
) -> crate::tui::app::App {
    let agent_has_permission_mode = matches!(
        resolved_agent.config.kind,
        crate::config::AgentKind::Claude
            | crate::config::AgentKind::Codex
            | crate::config::AgentKind::Qwen
            | crate::config::AgentKind::Opencode
    );
    let mut app = setup_app_for_agent(
        loaded,
        store,
        worktree_mgr,
        max_concurrent,
        bypass_review,
        workspace_trusted,
        agent_has_permission_mode.then_some(resolved_agent.id.as_str()),
    );
    app.selected_agent_id = resolved_agent.id.clone();
    if agent_has_permission_mode {
        app.pool
            .set_allowed_tools(resolved_agent.config.allowed_tools.clone());
    }
    app
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
