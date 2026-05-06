use crate::commands::setup::{setup_app_from_config_with_bypass, startup_cleanup};
use crate::config::Config;
use crate::provider::{create_provider, github::client::RepoProvider};
use crate::session::types::Session;
use crate::session::worktree::GitWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::app::work_assigner::WorkAssignmentService;
use crate::work::assigner::WorkAssigner;
use crate::work::types::WorkItem;

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
) -> anyhow::Result<()> {
    let loaded = Config::find_and_load_with_path()?;
    let config = loaded.config.clone();
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
        let report =
            tokio::task::spawn_blocking(move || crate::doctor::run_all_checks(Some(&config_ref)))
                .await?;
        let validation_config = config.clone();
        let validation = tokio::task::spawn_blocking(move || {
            crate::doctor::validate_provider_setup(&validation_config)
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

    let mut app = setup_app_from_config_with_bypass(
        loaded,
        store,
        worktree_mgr,
        effective_max_concurrent,
        bypass_review,
    );
    app.pool.set_provider(selected_provider);
    if matches!(
        resolved_agent.config.kind,
        crate::config::AgentKind::Claude
            | crate::config::AgentKind::Codex
            | crate::config::AgentKind::Qwen
    ) {
        app.pool.set_permission_mode(
            resolved_agent
                .config
                .permission_mode
                .clone()
                .unwrap_or_else(|| config.sessions.permission_mode.clone()),
        );
        app.pool
            .set_allowed_tools(resolved_agent.config.allowed_tools.clone());
    }
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
        app.github_client = Some(client);

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

        for num_str in issue_str.split(',') {
            let num: u64 = num_str
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid issue number: {}", num_str.trim()))?;

            let gh_issue = client.get_issue(num).await?;
            let (issue_mode, mode_config) = crate::modes::resolve_mode_for_labels(
                &gh_issue.labels,
                &session_mode,
                Some(&config),
            );
            let prompt = crate::prompts::PromptBuilder::build_issue_prompt_with_images(
                &gh_issue, &config, &images,
            );
            let mut session =
                Session::new(prompt, model.clone(), issue_mode, Some(num), role_override)
                    .with_mode_config(mode_config)
                    .with_image_paths(images.clone());
            session.issue_title = Some(gh_issue.title.clone());

            app.state.issue_cache.insert(num, gh_issue);

            app.add_session(session).await?;
        }

        app.github_client = Some(client);
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
        app.github_client = Some(client);
    }

    app.once_mode = once;
    app.no_splash = no_splash;

    crate::tui::run(app).await
}

fn provider_for_agent(
    resolved: &crate::config::ResolvedAgentConfig,
) -> anyhow::Result<std::sync::Arc<dyn crate::agent_provider::AgentProvider>> {
    match resolved.config.kind {
        crate::config::AgentKind::Claude => {
            let command = resolved.config.command.as_deref().unwrap_or("claude");
            Ok(std::sync::Arc::new(
                crate::agent_provider::ClaudeProvider::new(command),
            ))
        }
        crate::config::AgentKind::Qwen => {
            let command = resolved.config.command.as_deref().unwrap_or("qwen");
            Ok(std::sync::Arc::new(
                crate::agent_provider::QwenProvider::with_config(
                    command,
                    resolved.config.extra_args.clone(),
                    resolved.config.env.clone(),
                ),
            ))
        }
        crate::config::AgentKind::Codex => {
            let command = resolved.config.command.as_deref().unwrap_or("codex");
            Ok(std::sync::Arc::new(
                crate::agent_provider::CodexProvider::with_config(
                    command,
                    resolved.config.sandbox.clone(),
                    resolved.config.ephemeral,
                    resolved.config.profile.clone(),
                    resolved.config.config_overrides.clone(),
                    resolved.config.extra_args.clone(),
                    resolved.config.env.clone(),
                    resolved.config.json,
                ),
            ))
        }
        other => anyhow::bail!(
            "agent `{}` uses `{}` provider, but that provider runtime is not implemented yet",
            resolved.id,
            other.as_str()
        ),
    }
}
