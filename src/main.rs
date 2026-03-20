mod config;
mod github;
mod session;
mod state;
mod tui;
mod work;

use clap::{Parser, Subcommand};
use config::Config;
use github::client::{GhCliClient, GitHubClient};
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

        /// Max concurrent sessions (overrides config)
        #[arg(long)]
        max_concurrent: Option<usize>,
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
        Some(Commands::Status) => cmd_status(),
        Some(Commands::Cost) => cmd_cost(),
        Some(Commands::Queue) => cmd_queue().await,
        Some(Commands::Add { issue_number }) => cmd_add(issue_number).await,
        Some(Commands::Run {
            prompt,
            issue,
            milestone,
            model,
            max_concurrent,
        }) => cmd_run(prompt, issue, milestone, model, max_concurrent).await,
        // Default: launch TUI with no sessions (dashboard mode)
        None => cmd_dashboard().await,
    }
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
cache_ttl_secs = 300

[notifications]
desktop = true
slack = false
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
        let blockers = item.blockers();
        let blocked_str = if blockers.is_empty() {
            "-".to_string()
        } else {
            blockers
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
        let ready_str = if item.is_ready(&[]) {
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

async fn cmd_run(
    prompt: Option<String>,
    issue: Option<String>,
    milestone: Option<String>,
    model: Option<String>,
    max_concurrent_override: Option<usize>,
) -> anyhow::Result<()> {
    let config = Config::find_and_load()?;
    let model = model.unwrap_or(config.sessions.default_model.clone());
    let max_concurrent = max_concurrent_override.unwrap_or(config.sessions.max_concurrent);

    let store = StateStore::new(StateStore::default_path());
    let repo_root = std::env::current_dir()?;
    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root));

    let mut app = App::new(
        store,
        max_concurrent,
        worktree_mgr,
        config.sessions.permission_mode.clone(),
        config.sessions.allowed_tools.clone(),
    );

    // Determine what to run
    if let Some(prompt_text) = prompt {
        let session = Session::new(
            prompt_text,
            model,
            config.sessions.default_mode.clone(),
            None,
        );
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
        app.config = Some(config.clone());
    } else if let Some(issue_str) = issue {
        let client = GhCliClient::new();

        // Parse comma-separated issue numbers and fetch full issue data
        for num_str in issue_str.split(',') {
            let num: u64 = num_str
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid issue number: {}", num_str.trim()))?;

            let gh_issue = client.get_issue(num).await?;
            let mut session = Session::new(
                gh_issue.unattended_prompt(),
                model.clone(),
                config.sessions.default_mode.clone(),
                Some(num),
            );
            session.issue_title = Some(gh_issue.title.clone());

            // Cache issue data
            app.state.issue_cache.insert(num, gh_issue);

            app.add_session(session).await?;
        }

        // Set github client + config so label lifecycle and auto-PR work
        app.github_client = Some(Box::new(client));
        app.config = Some(config.clone());
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
        app.config = Some(config.clone());
    }

    // Launch TUI
    tui::run(app).await
}

async fn cmd_dashboard() -> anyhow::Result<()> {
    let store = StateStore::new(StateStore::default_path());
    let repo_root = std::env::current_dir()?;
    let worktree_mgr = Box::new(GitWorktreeManager::new(repo_root));
    let app = App::new(
        store,
        3,
        worktree_mgr,
        "bypassPermissions".into(),
        Vec::new(),
    );
    tui::run(app).await
}
