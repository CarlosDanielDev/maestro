mod budget;
mod config;
mod doctor;
mod gates;
mod git;
mod github;
mod models;
mod modes;
mod notifications;
mod plugins;
mod prompts;
mod provider;
mod review;
mod session;
mod state;
mod tui;
mod util;
mod work;

use clap::{Parser, Subcommand};
use config::{Config, NotificationsConfig};
use github::client::{GhCliClient, GitHubClient};
use notifications::dispatcher::NotificationDispatcher;
use session::types::Session;
use session::worktree::GitWorktreeManager;
use state::store::StateStore;
use tui::app::App;
use work::assigner::WorkAssigner;
use work::types::WorkItem;

#[derive(Parser)]
#[command(
    name = "maestro",
    version,
    about = "Multi-session Claude Code orchestrator"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run sessions from GitHub issues or a prompt
    Run {
        /// Prompt to send to Claude
        #[arg(short, long)]
        prompt: Option<String>,

        /// GitHub issue number(s), comma-separated
        #[arg(short, long)]
        issue: Option<String>,

        /// Milestone to fetch all issues from
        #[arg(short = 'M', long)]
        milestone: Option<String>,

        /// Model to use (opus, sonnet, haiku)
        #[arg(short, long)]
        model: Option<String>,

        /// Session mode (orchestrator, vibe, review, or custom)
        #[arg(long)]
        mode: Option<String>,

        /// Max concurrent sessions (overrides config)
        #[arg(long)]
        max_concurrent: Option<usize>,

        /// Resume from previous state after a crash
        #[arg(long)]
        resume: bool,
    },
    /// Show queued/pending issues from GitHub
    Queue,
    /// Add an issue to the work queue manually
    Add {
        /// Issue number to add
        issue_number: u64,
    },
    /// Show current state without TUI
    Status,
    /// Show spending report
    Cost,
    /// Initialize maestro.toml in current directory
    Init,
    /// Clean orphaned worktrees left by crashed sessions
    Clean {
        /// Show what would be cleaned without actually doing it
        #[arg(long)]
        dry_run: bool,
    },
    /// Show session transcript logs
    Logs {
        /// Show full log for a specific session ID
        #[arg(long)]
        session: Option<String>,
        /// Export as JSON
        #[arg(long)]
        export: Option<String>,
    },
    /// Resume interrupted sessions from saved state
    Resume {
        /// Resume a specific session by ID
        #[arg(long)]
        session: Option<String>,
    },
    /// Test Slack webhook configuration
    TestSlack,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for (bash, zsh, fish)
        shell: String,
    },
    /// Check environment setup and required tools
    Doctor,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init logging (to file, not terminal — TUI owns stdout)
    tracing_subscriber::fmt()
        .with_env_filter("maestro=debug")
        .with_writer(|| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("maestro.log")
                .unwrap_or_else(|_| {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .open("/dev/null")
                        .unwrap()
                })
        })
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => cmd_init(),
        Some(Commands::Clean { dry_run }) => cmd_clean(dry_run),
        Some(Commands::Logs { session, export }) => cmd_logs(session, export),
        Some(Commands::Resume { session }) => cmd_resume(session).await,
        Some(Commands::TestSlack) => cmd_test_slack().await,
        Some(Commands::Completions { shell }) => cmd_completions(&shell),
        Some(Commands::Doctor) => cmd_doctor(),
        Some(Commands::Status) => cmd_status(),
        Some(Commands::Cost) => cmd_cost(),
        Some(Commands::Queue) => cmd_queue().await,
        Some(Commands::Add { issue_number }) => cmd_add(issue_number).await,
        Some(Commands::Run {
            prompt,
            issue,
            milestone,
            model,
            mode,
            max_concurrent,
            resume,
        }) => {
            cmd_run(
                prompt,
                issue,
                milestone,
                model,
                mode,
                max_concurrent,
                resume,
            )
            .await
        }
        // Default: launch TUI with no sessions (dashboard mode)
        None => cmd_dashboard().await,
    }
}

fn build_notification_dispatcher(cfg: &NotificationsConfig) -> NotificationDispatcher {
    let mut dispatcher = NotificationDispatcher::new(cfg.desktop);
    if cfg.slack {
        if let Some(ref url) = cfg.slack_webhook_url {
            dispatcher = dispatcher.with_slack(url.clone(), cfg.slack_rate_limit_per_min);
            tracing::info!("Slack notifications enabled");
        } else {
            tracing::warn!("notifications.slack = true but no slack_webhook_url configured");
        }
    }
    dispatcher
}

fn cmd_init() -> anyhow::Result<()> {
    let path = std::path::PathBuf::from("maestro.toml");
    if path.exists() {
        println!("maestro.toml already exists.");
        return Ok(());
    }

    let content = r#"[project]
repo = ""
base_branch = "main"

[sessions]
max_concurrent = 3
stall_timeout_secs = 300
default_model = "opus"
default_mode = "orchestrator"
permission_mode = "bypassPermissions"  # Options: default, acceptEdits, bypassPermissions, dontAsk, plan, auto
allowed_tools = []                      # Empty = all tools. Example: ["Bash", "Read", "Write", "Edit"]

[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80

[github]
issue_filter_labels = ["maestro:ready"]
auto_pr = true
auto_merge = false                      # Set to true to auto-merge PRs after CI + review pass
merge_method = "squash"                 # Options: merge, squash, rebase
cache_ttl_secs = 300

[gates]
enabled = true
test_command = "cargo test"
ci_poll_interval_secs = 30
ci_max_wait_secs = 1800

[notifications]
desktop = true
slack = false
# slack_webhook_url = "https://hooks.slack.com/services/T.../B.../xxx"
# slack_rate_limit_per_min = 10

[review]
enabled = false
command = "gh pr review {pr_number} --comment --body 'Automated review by Maestro'"
# reviewers = [
#   { name = "claude", command = "claude --print 'review PR #{pr_number}'", required = true },
#   { name = "codex", command = "codex review {pr_number}", required = false },
# ]

[concurrency]
heavy_task_labels = []                  # Labels that mark a task as resource-intensive
heavy_task_limit = 2                    # Max concurrent heavy tasks

[monitoring]
work_tick_interval_secs = 10

# Plugin hooks — shell commands triggered on lifecycle events
# [[plugins]]
# name = "notify-team"
# on = "session_completed"             # Hook points: session_started, session_completed, tests_passed,
#                                      #   tests_failed, budget_threshold, file_conflict, pr_created
# run = "curl -X POST https://slack.webhook/..."
# timeout_secs = 30                    # Optional per-plugin timeout

# Custom modes — define system prompt and allowed tools
# [modes.review]
# system_prompt = "You are a code reviewer. Review the PR and leave comments."
# allowed_tools = ["Read", "Grep", "Glob"]
# permission_mode = "plan"
"#;

    std::fs::write(&path, content)?;
    println!("Created maestro.toml");
    Ok(())
}

fn cmd_status() -> anyhow::Result<()> {
    let store = StateStore::new(StateStore::default_path());
    let state = store.load()?;

    if state.sessions.is_empty() {
        println!("No sessions recorded.");
        return Ok(());
    }

    println!(
        "Sessions: {} total, {} active",
        state.sessions.len(),
        state.active_sessions().len()
    );
    println!("Total cost: ${:.2}", state.total_cost_usd);
    println!();

    for session in &state.sessions {
        let label = match session.issue_number {
            Some(n) => format!("#{}", n),
            None => session.id.to_string()[..8].to_string(),
        };
        println!(
            "  {} {} {} ${:.2} {}",
            session.status.symbol(),
            label,
            session.status.label(),
            session.cost_usd,
            session.elapsed_display(),
        );
    }

    Ok(())
}

fn cmd_cost() -> anyhow::Result<()> {
    let store = StateStore::new(StateStore::default_path());
    let state = store.load()?;

    println!("=== Maestro Spending Report ===");
    println!("Total: ${:.2}", state.total_cost_usd);
    println!();

    for session in &state.sessions {
        let label = match session.issue_number {
            Some(n) => format!("#{:<6}", n),
            None => session.id.to_string()[..8].to_string(),
        };
        println!(
            "  {} ${:.2} ({})",
            label,
            session.cost_usd,
            session.status.label(),
        );
    }

    Ok(())
}

async fn cmd_queue() -> anyhow::Result<()> {
    let config = Config::find_and_load()?;
    let client = GhCliClient::new();
    let label_refs: Vec<&str> = config
        .github
        .issue_filter_labels
        .iter()
        .map(|s| s.as_str())
        .collect();
    let issues = client.list_issues(&label_refs).await?;

    if issues.is_empty() {
        println!(
            "No issues found with labels: {:?}",
            config.github.issue_filter_labels
        );
        return Ok(());
    }

    let items: Vec<WorkItem> = issues.into_iter().map(WorkItem::from_issue).collect();
    let assigner = WorkAssigner::new(items);

    println!(
        "{:<10} {:<8} {:<50} {:<10} {:<15}",
        "Priority", "Issue", "Title", "Status", "Blocked By"
    );
    println!("{}", "-".repeat(93));

    for item in assigner.all_items() {
        let blocked_str = if item.blocked_by.is_empty() {
            "-".to_string()
        } else {
            item.blocked_by
                .iter()
                .map(|n| format!("#{}", n))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let title: String = if item.title().chars().count() > 48 {
            let truncated: String = item.title().chars().take(45).collect();
            format!("{}...", truncated)
        } else {
            item.title().to_string()
        };
        let no_completed = std::collections::HashSet::new();
        let ready_str = if item.is_ready(&no_completed) {
            "Ready"
        } else {
            "Blocked"
        };
        println!(
            "{:<10} #{:<7} {:<50} {:<10} {}",
            format!("{:?}", item.priority),
            item.number(),
            title,
            ready_str,
            blocked_str
        );
    }

    let counts = assigner.count_by_status();
    println!(
        "\nTotal: {} issues ({} ready, {} blocked)",
        assigner.total(),
        counts.pending,
        assigner.total() - counts.pending
    );

    Ok(())
}

async fn cmd_add(issue_number: u64) -> anyhow::Result<()> {
    let client = GhCliClient::new();
    // Add the maestro:ready label to the issue
    client.add_label(issue_number, "maestro:ready").await?;
    println!("Added 'maestro:ready' label to issue #{}", issue_number);
    Ok(())
}

fn cmd_logs(session: Option<String>, export: Option<String>) -> anyhow::Result<()> {
    let logger = session::logger::SessionLogger::new(session::logger::SessionLogger::default_dir());

    if let Some(session_id_str) = session {
        let session_id: uuid::Uuid = session_id_str
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid session ID: {}", session_id_str))?;
        let content = logger.read_log(session_id)?;

        if export.as_deref() == Some("json") {
            let lines: Vec<&str> = content.lines().collect();
            let json = serde_json::json!({
                "session_id": session_id_str,
                "lines": lines,
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        } else {
            println!("{}", content);
        }
    } else {
        let logs = logger.list_logs()?;
        if logs.is_empty() {
            println!("No session logs found.");
            return Ok(());
        }

        if export.as_deref() == Some("json") {
            let entries: Vec<serde_json::Value> = logs
                .iter()
                .map(|l| {
                    serde_json::json!({
                        "session_id": l.session_id,
                        "size_bytes": l.size_bytes,
                        "path": l.path.display().to_string(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&entries)?);
        } else {
            println!("{:<40} {:>10}", "Session ID", "Size");
            println!("{}", "-".repeat(52));
            for log in &logs {
                println!(
                    "{:<40} {:>10}",
                    log.session_id,
                    format_bytes(log.size_bytes)
                );
            }
            println!("\n{} log(s) found.", logs.len());
        }
    }
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn cmd_clean(dry_run: bool) -> anyhow::Result<()> {
    let repo_root = std::env::current_dir()?;
    let mgr = session::cleanup::CleanupManager::new(&repo_root);
    let orphans = mgr.scan_orphans()?;

    if orphans.is_empty() {
        println!("No orphaned worktrees found.");
        return Ok(());
    }

    println!("Found {} orphaned worktree(s):", orphans.len());
    for orphan in &orphans {
        println!("  {} ({})", orphan.name, orphan.path.display());
    }

    if dry_run {
        println!("\nDry run — no changes made.");
    } else {
        let removed = mgr.remove_orphans(&orphans)?;
        println!("\nRemoved {} worktree(s).", removed);
    }

    Ok(())
}

async fn cmd_resume(session_filter: Option<String>) -> anyhow::Result<()> {
    let config = Config::find_and_load()?;
    let store = StateStore::new(StateStore::default_path());
    let state = store.load()?;
    let repo_root = std::env::current_dir()?;

    // Find incomplete sessions
    let incomplete: Vec<&Session> = state
        .sessions
        .iter()
        .filter(|s| {
            matches!(
                s.status,
                session::types::SessionStatus::Running
                    | session::types::SessionStatus::Spawning
                    | session::types::SessionStatus::Queued
                    | session::types::SessionStatus::Stalled
                    | session::types::SessionStatus::Errored
                    | session::types::SessionStatus::Retrying
            )
        })
        .filter(|s| {
            if let Some(ref filter) = session_filter {
                s.id.to_string().starts_with(filter)
                    || s.issue_number
                        .map(|n| n.to_string() == *filter)
                        .unwrap_or(false)
            } else {
                true
            }
        })
        .collect();

    if incomplete.is_empty() {
        println!("No incomplete sessions to resume.");
        return Ok(());
    }

    println!("Resuming {} incomplete session(s)...", incomplete.len());

    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root));
    let max_concurrent = config.sessions.max_concurrent;

    let mut app = App::new(
        store,
        max_concurrent,
        worktree_mgr,
        config.sessions.permission_mode.clone(),
        config.sessions.allowed_tools.clone(),
    );

    // Wire up from config
    app.budget_enforcer = Some(crate::budget::BudgetEnforcer::new(
        config.budget.per_session_usd,
        config.budget.total_usd,
        config.budget.alert_threshold_pct,
    ));
    app.model_router = Some(crate::models::ModelRouter::new(
        config.models.routing.clone(),
        config.sessions.default_model.clone(),
    ));
    app.notifications = build_notification_dispatcher(&config.notifications);
    if !config.plugins.is_empty() {
        app.plugin_runner = Some(crate::plugins::runner::PluginRunner::new(
            config.plugins.clone(),
            30,
        ));
    }

    // Re-enqueue incomplete sessions with their original prompts
    for s in &incomplete {
        let mut new_session = Session::new(
            s.prompt.clone(),
            s.model.clone(),
            s.mode.clone(),
            s.issue_number,
        );
        new_session.issue_title = s.issue_title.clone();
        new_session.retry_count = s.retry_count;
        app.add_session(new_session).await?;
    }

    // Set up GitHub client for label management
    let client = GhCliClient::new();
    app.github_client = Some(Box::new(client));
    app.configure(config);

    tui::run(app).await
}

async fn cmd_test_slack() -> anyhow::Result<()> {
    let config = Config::find_and_load()?;
    let mut dispatcher = build_notification_dispatcher(&config.notifications);

    if !dispatcher.has_slack() {
        anyhow::bail!(
            "Slack is not configured. Set notifications.slack = true and slack_webhook_url in maestro.toml"
        );
    }

    println!("Sending test message to Slack webhook...");
    match dispatcher.test_slack().await {
        Ok(true) => {
            println!("Slack webhook test successful!");
            Ok(())
        }
        Ok(false) => {
            anyhow::bail!("Test was rate-limited. Try again later.")
        }
        Err(e) => {
            anyhow::bail!("Slack webhook test failed: {}", e)
        }
    }
}

fn cmd_completions(shell: &str) -> anyhow::Result<()> {
    use clap::CommandFactory;
    use clap_complete::{Shell, generate};

    let shell = match shell.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        other => anyhow::bail!("Unsupported shell: {}. Use bash, zsh, or fish.", other),
    };

    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "maestro", &mut std::io::stdout());
    Ok(())
}

fn cmd_doctor() -> anyhow::Result<()> {
    let config = Config::find_and_load().ok();
    let report = doctor::run_all_checks(config.as_ref());
    doctor::print_report(&report);

    if report.has_failures() {
        std::process::exit(1);
    }
    Ok(())
}

async fn cmd_run(
    prompt: Option<String>,
    issue: Option<String>,
    milestone: Option<String>,
    model: Option<String>,
    mode: Option<String>,
    max_concurrent_override: Option<usize>,
    resume: bool,
) -> anyhow::Result<()> {
    let config = Config::find_and_load()?;
    let model = model.unwrap_or(config.sessions.default_model.clone());
    let session_mode = mode.unwrap_or(config.sessions.default_mode.clone());
    let max_concurrent = max_concurrent_override.unwrap_or(config.sessions.max_concurrent);

    let store = StateStore::new(StateStore::default_path());
    let repo_root = std::env::current_dir()?;

    // Startup cleanup: remove orphaned worktrees (non-blocking)
    {
        let cleanup_mgr = session::cleanup::CleanupManager::new(&repo_root);
        if let Ok(orphans) = cleanup_mgr.scan_orphans()
            && !orphans.is_empty()
        {
            tracing::info!("Cleaning {} orphaned worktrees on startup", orphans.len());
            let _ = cleanup_mgr.remove_orphans(&orphans);
        }
    }

    // Startup log cleanup: remove logs older than 30 days
    {
        let logger =
            session::logger::SessionLogger::new(session::logger::SessionLogger::default_dir());
        if let Ok(removed) = logger.cleanup_old_logs(30)
            && removed > 0
        {
            tracing::info!("Cleaned {} old session logs", removed);
        }
    }
    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root));

    let mut app = App::new(
        store,
        max_concurrent,
        worktree_mgr,
        config.sessions.permission_mode.clone(),
        config.sessions.allowed_tools.clone(),
    );

    // Wire up budget enforcer from config
    app.budget_enforcer = Some(crate::budget::BudgetEnforcer::new(
        config.budget.per_session_usd,
        config.budget.total_usd,
        config.budget.alert_threshold_pct,
    ));

    // Wire up model router from config
    app.model_router = Some(crate::models::ModelRouter::new(
        config.models.routing.clone(),
        config.sessions.default_model.clone(),
    ));

    // Wire up notification dispatcher from config
    app.notifications = build_notification_dispatcher(&config.notifications);

    // Wire up plugin runner from config
    if !config.plugins.is_empty() {
        app.plugin_runner = Some(crate::plugins::runner::PluginRunner::new(
            config.plugins.clone(),
            30, // default timeout
        ));
    }

    // Resume from previous state if requested
    if resume {
        let mut recovered = 0;
        for session in &mut app.state.sessions {
            if matches!(
                session.status,
                session::types::SessionStatus::Running | session::types::SessionStatus::Spawning
            ) {
                session.status = session::types::SessionStatus::Errored;
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

    // Determine what to run
    if let Some(prompt_text) = prompt {
        let session = Session::new(prompt_text, model, session_mode.clone(), None);
        app.add_session(session).await?;
    } else if let Some(milestone_name) = milestone {
        // Fetch all issues in the milestone
        let client = GhCliClient::new();
        let issues = client.list_issues_by_milestone(&milestone_name).await?;
        if issues.is_empty() {
            anyhow::bail!("No open issues found in milestone '{}'", milestone_name);
        }

        let items: Vec<WorkItem> = issues.into_iter().map(WorkItem::from_issue).collect();
        let assigner = WorkAssigner::new(items);

        // Store assigner and config in app for work management
        app.work_assigner = Some(assigner);
        app.github_client = Some(Box::new(client));
        app.configure(config.clone());
    } else if let Some(issue_str) = issue {
        let client = GhCliClient::new();

        // Parse comma-separated issue numbers and fetch full issue data
        for num_str in issue_str.split(',') {
            let num: u64 = num_str
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid issue number: {}", num_str.trim()))?;

            let gh_issue = client.get_issue(num).await?;
            // Use mode from label (maestro:mode:X) or CLI --mode or config default
            let issue_mode = crate::modes::mode_from_labels(&gh_issue.labels)
                .unwrap_or_else(|| session_mode.clone());
            let mut session = Session::new(
                gh_issue.unattended_prompt(),
                model.clone(),
                issue_mode,
                Some(num),
            );
            session.issue_title = Some(gh_issue.title.clone());

            // Cache issue data
            app.state.issue_cache.insert(num, gh_issue);

            app.add_session(session).await?;
        }

        // Set github client + config so label lifecycle and auto-PR work
        app.github_client = Some(Box::new(client));
        app.configure(config.clone());
    } else {
        // No prompt, issue, or milestone — auto-fetch maestro:ready issues
        let client = GhCliClient::new();
        let label_refs: Vec<&str> = config
            .github
            .issue_filter_labels
            .iter()
            .map(|s| s.as_str())
            .collect();
        let issues = client.list_issues(&label_refs).await?;

        if issues.is_empty() {
            anyhow::bail!(
                "No issues found with labels {:?}. Use --prompt or --issue instead.",
                config.github.issue_filter_labels
            );
        }

        let items: Vec<WorkItem> = issues.into_iter().map(WorkItem::from_issue).collect();
        let assigner = WorkAssigner::new(items);

        app.work_assigner = Some(assigner);
        app.github_client = Some(Box::new(client));
        app.configure(config.clone());
    }

    // Launch TUI
    tui::run(app).await
}

async fn cmd_dashboard() -> anyhow::Result<()> {
    let store = StateStore::new(StateStore::default_path());
    let state = store.load().unwrap_or_default();
    let repo_root = std::env::current_dir()?;

    // Check for incomplete sessions and offer auto-resume
    let has_incomplete = state.sessions.iter().any(|s| {
        matches!(
            s.status,
            session::types::SessionStatus::Running
                | session::types::SessionStatus::Spawning
                | session::types::SessionStatus::Queued
                | session::types::SessionStatus::Stalled
                | session::types::SessionStatus::Retrying
        )
    });

    if has_incomplete {
        eprintln!(
            "Found incomplete sessions from previous run. Use `maestro resume` to continue them."
        );
    }

    // Load config for project info
    let config = Config::find_and_load().ok();

    // Build project info from config and git
    let repo_name = config
        .as_ref()
        .map(|c| c.project.repo.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            // Try to get repo name from git remote
            std::process::Command::new("git")
                .args(["remote", "get-url", "origin"])
                .output()
                .ok()
                .and_then(|o| {
                    let url = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    url.rsplit('/')
                        .next()
                        .map(|s| s.trim_end_matches(".git").to_string())
                })
                .unwrap_or_else(|| "unknown".to_string())
        });

    let branch = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let project_info = tui::screens::home::ProjectInfo {
        repo: repo_name,
        branch,
    };

    // Build recent sessions from saved state
    let recent_sessions: Vec<tui::screens::home::SessionSummary> = state
        .sessions
        .iter()
        .rev()
        .take(10)
        .map(|s| tui::screens::home::SessionSummary {
            issue_number: s.issue_number.unwrap_or(0),
            title: s.last_message.clone(),
            status: s.status.label().to_string(),
            cost_usd: s.cost_usd,
        })
        .collect();

    let max_concurrent = config
        .as_ref()
        .map(|c| c.sessions.max_concurrent)
        .unwrap_or(3);

    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root));
    let mut app = App::new(
        store,
        max_concurrent,
        worktree_mgr,
        "bypassPermissions".into(),
        Vec::new(),
    );

    // Run preflight checks on a blocking thread to avoid stalling async runtime
    let config_clone = config.clone();
    let doctor_warnings = tokio::task::spawn_blocking(move || {
        let report = doctor::run_all_checks(config_clone.as_ref());
        report
            .failed_checks()
            .iter()
            .map(|c| format!("{}: {}", c.name, c.message))
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();

    // Set up home screen and start in Dashboard mode
    app.home_screen = Some(tui::screens::HomeScreen::new(
        project_info,
        recent_sessions,
        doctor_warnings,
    ));
    app.tui_mode = tui::app::TuiMode::Dashboard;
    app.config = config;

    tui::run(app).await
}
