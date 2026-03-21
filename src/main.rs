mod config;
mod session;
mod state;
mod tui;

use clap::{Parser, Subcommand};
use config::Config;
use session::types::Session;
use session::worktree::GitWorktreeManager;
use state::store::StateStore;
use tui::app::App;

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
    /// Run a session with a prompt or issue number
    Run {
        /// Prompt to send to Claude
        #[arg(short, long)]
        prompt: Option<String>,

        /// GitHub issue number(s), comma-separated
        #[arg(short, long)]
        issue: Option<String>,

        /// Model to use (opus, sonnet, haiku)
        #[arg(short, long)]
        model: Option<String>,

        /// Max concurrent sessions (overrides config)
        #[arg(long)]
        max_concurrent: Option<usize>,
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
        Some(Commands::Run {
            prompt,
            issue,
            model,
            max_concurrent,
        }) => cmd_run(prompt, issue, model, max_concurrent).await,
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

async fn cmd_run(
    prompt: Option<String>,
    issue: Option<String>,
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
    } else if let Some(issue_str) = issue {
        // Parse comma-separated issue numbers
        for num_str in issue_str.split(',') {
            let num: u64 = num_str
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid issue number: {}", num_str.trim()))?;
            let prompt = format!(
                "Work on GitHub issue #{}. Read the issue details and implement the required changes.",
                num
            );
            let session = Session::new(
                prompt,
                model.clone(),
                config.sessions.default_mode.clone(),
                Some(num),
            );
            app.add_session(session).await?;
        }
    } else {
        anyhow::bail!("Provide --prompt or --issue. See `maestro run --help`.");
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
