use crate::commands::setup::{setup_app_from_config_with_bypass, startup_cleanup};
use crate::config::Config;
use crate::provider::github::client::{GhCliClient, GitHubClient};
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
        if let Err(e) = crate::doctor::validate_preflight(&report) {
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

    let model = model.unwrap_or(config.sessions.default_model.clone());
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
        let session = Session::new(
            prompt_text,
            model,
            session_mode.clone(),
            None,
            role_override,
        )
        .with_image_paths(images.clone());
        app.add_session(session).await?;
    } else if let Some(milestone_name) = milestone {
        let client = GhCliClient::from_config_repo(Some(config.project.repo.clone()));
        let issues = client.list_issues_by_milestone(&milestone_name).await?;
        if issues.is_empty() {
            anyhow::bail!("No open issues found in milestone '{}'", milestone_name);
        }

        let items: Vec<WorkItem> = issues.into_iter().map(WorkItem::from_issue).collect();
        let assigner = WorkAssigner::new(items);

        app.work_assignment_service = Some(WorkAssignmentService::new(assigner));
        app.github_client = Some(Box::new(client));

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
        let client = GhCliClient::from_config_repo(Some(config.project.repo.clone()));

        for num_str in issue_str.split(',') {
            let num: u64 = num_str
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid issue number: {}", num_str.trim()))?;

            let gh_issue = client.get_issue(num).await?;
            let issue_mode = crate::modes::mode_from_labels(&gh_issue.labels)
                .unwrap_or_else(|| session_mode.clone());
            let prompt = crate::prompts::PromptBuilder::build_issue_prompt_with_images(
                &gh_issue, &config, &images,
            );
            let mut session =
                Session::new(prompt, model.clone(), issue_mode, Some(num), role_override)
                    .with_image_paths(images.clone());
            session.issue_title = Some(gh_issue.title.clone());

            app.state.issue_cache.insert(num, gh_issue);

            app.add_session(session).await?;
        }

        app.github_client = Some(Box::new(client));
    } else {
        let client = GhCliClient::from_config_repo(Some(config.project.repo.clone()));
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
        app.github_client = Some(Box::new(client));
    }

    app.once_mode = once;
    app.no_splash = no_splash;

    crate::tui::run(app).await
}
