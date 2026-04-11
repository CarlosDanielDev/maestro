mod budget;
mod ci_polling;
mod completion_pipeline;
mod completion_summary;
mod context_overflow;
mod data_handler;
mod event_handler;
pub(crate) mod helpers;
mod issue_completion;
mod plugins;
mod pr_retry;
mod review;
mod session_lifecycle;
mod session_spawners;
pub mod types;
mod work_assigner;

pub use types::*;

use crate::budget::BudgetEnforcer;
use crate::config::Config;
use crate::continuous::ContinuousModeState;
use crate::github::ci::PendingPrCheck;
use crate::github::client::GitHubClient;
use crate::models::ModelRouter;
use crate::notifications::dispatcher::NotificationDispatcher;
use crate::plugins::runner::PluginRunner;
use crate::session::context_monitor::{ContextMonitor, ProductionContextMonitor};
use crate::session::fork::ForkPolicy;
use crate::session::health::{HealthCheck, HealthMonitor};
use crate::session::logger::SessionLogger;
use crate::session::manager::SessionEvent;
use crate::session::pool::SessionPool;
use crate::session::types::Session;
use crate::session::worktree::WorktreeManager;
use crate::state::progress::ProgressTracker;
use crate::state::store::StateStore;
use crate::state::types::MaestroState;
use crate::tui::activity_log::{ActivityLog, LogLevel};
use crate::tui::panels::PanelView;
use crate::tui::theme::Theme;
use crate::work::assigner::WorkAssigner;
use chrono::Utc;
use std::time::Instant;
use tokio::sync::mpsc;

pub struct App {
    pub pool: SessionPool,
    pub activity_log: ActivityLog,
    pub panel_view: PanelView,
    pub state: MaestroState,
    pub store: StateStore,
    pub running: bool,
    pub total_cost: f64,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    pub work_assigner: Option<WorkAssigner>,
    pub github_client: Option<Box<dyn GitHubClient>>,
    pub config: Option<Config>,
    pub(crate) pending_issue_completions: Vec<PendingIssueCompletion>,
    pub(crate) pending_hooks: Vec<PendingHook>,
    pub health_monitor: Box<dyn HealthCheck>,
    pub budget_enforcer: Option<BudgetEnforcer>,
    pub model_router: Option<ModelRouter>,
    pub progress_tracker: ProgressTracker,
    pub notifications: NotificationDispatcher,
    pub tui_mode: TuiMode,
    pub session_logger: SessionLogger,
    pub pending_pr_checks: Vec<PendingPrCheck>,
    pub(crate) last_ci_poll: Instant,
    pub(crate) last_work_tick: Instant,
    pub plugin_runner: Option<PluginRunner>,
    pub show_help: bool,
    pub help_scroll: u16,
    pub context_monitor: Box<dyn ContextMonitor>,
    pub fork_policy: Option<ForkPolicy>,
    pub home_screen: Option<crate::tui::screens::HomeScreen>,
    pub issue_browser_screen: Option<crate::tui::screens::IssueBrowserScreen>,
    pub milestone_screen: Option<crate::tui::screens::MilestoneScreen>,
    pub prompt_input_screen: Option<crate::tui::screens::PromptInputScreen>,
    pub pending_commands: Vec<TuiCommand>,
    pub pending_session_launches: Vec<Session>,
    pub data_tx: mpsc::UnboundedSender<TuiDataEvent>,
    pub data_rx: mpsc::UnboundedReceiver<TuiDataEvent>,
    pub theme: Theme,
    pub preview_theme: Option<Theme>,
    pub once_mode: bool,
    pub completion_summary: Option<CompletionSummaryData>,
    pub continuous_mode: Option<ContinuousModeState>,
    pub upgrade_state: crate::updater::UpgradeState,
    pub spinner_tick: usize,
    pub completion_summary_dismissed: bool,
    pub gh_auth_ok: bool,
    pub pending_prs: Vec<crate::github::types::PendingPr>,
    pub flags: crate::flags::store::FeatureFlags,
    pub queue_confirmation_screen: Option<crate::tui::screens::QueueConfirmationScreen>,
    pub ci_check_details: std::collections::HashMap<u64, Vec<crate::github::ci::CheckRunDetail>>,
    pub queue_executor: Option<crate::work::executor::QueueExecutor>,
    pub queue_launch_configs: Option<Vec<crate::tui::screens::SessionConfig>>,
    pub hollow_retry_screen: Option<crate::tui::screens::HollowRetryScreen>,
    pub sanitize_screen: Option<crate::sanitize::screen::SanitizeScreen>,
    pub settings_screen: Option<crate::tui::screens::SettingsScreen>,
    pub prompt_history: crate::state::prompt_history::PromptHistoryStore,
    pub session_switcher: Option<crate::tui::session_switcher::SessionSwitcher>,
    pub adapt_screen: Option<crate::tui::screens::adapt::AdaptScreen>,
    pub pr_review_screen: Option<crate::tui::screens::pr_review::PrReviewScreen>,
    pub release_notes_screen: Option<crate::tui::screens::ReleaseNotesScreen>,
}

impl App {
    pub fn new(
        store: StateStore,
        max_concurrent: usize,
        worktree_mgr: Box<dyn WorktreeManager + Send>,
        permission_mode: String,
        allowed_tools: Vec<String>,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (data_tx, data_rx) = mpsc::unbounded_channel();
        let state = store.load().unwrap_or_default();
        let mut pool = SessionPool::new(max_concurrent, worktree_mgr, event_tx.clone());
        pool.set_permission_mode(permission_mode);
        pool.set_allowed_tools(allowed_tools);
        Self {
            pool,
            activity_log: ActivityLog::new(500),
            panel_view: PanelView::new(),
            state,
            store,
            running: true,
            total_cost: 0.0,
            start_time: Utc::now(),
            event_rx,
            work_assigner: None,
            github_client: None,
            config: None,
            pending_issue_completions: Vec::new(),
            pending_hooks: Vec::new(),
            health_monitor: Box::new(HealthMonitor::new()),
            budget_enforcer: None,
            model_router: None,
            progress_tracker: ProgressTracker::new(),
            notifications: NotificationDispatcher::new(false),
            tui_mode: TuiMode::Overview,
            session_logger: SessionLogger::new(SessionLogger::default_dir()),
            pending_pr_checks: Vec::new(),
            last_ci_poll: Instant::now(),
            last_work_tick: Instant::now(),
            plugin_runner: None,
            show_help: false,
            help_scroll: 0,
            context_monitor: Box::new(ProductionContextMonitor::new()),
            fork_policy: None,
            home_screen: None,
            issue_browser_screen: None,
            milestone_screen: None,
            prompt_input_screen: None,
            pending_commands: Vec::new(),
            pending_session_launches: Vec::new(),
            data_tx,
            data_rx,
            theme: Theme::default(),
            preview_theme: None,
            once_mode: false,
            completion_summary: None,
            continuous_mode: None,
            upgrade_state: crate::updater::UpgradeState::Hidden,
            spinner_tick: 0,
            completion_summary_dismissed: false,
            gh_auth_ok: true,
            pending_prs: Vec::new(),
            flags: crate::flags::store::FeatureFlags::default(),
            queue_confirmation_screen: None,
            ci_check_details: std::collections::HashMap::new(),
            queue_executor: None,
            queue_launch_configs: None,
            hollow_retry_screen: None,
            sanitize_screen: None,
            settings_screen: None,
            prompt_history: crate::state::prompt_history::PromptHistoryStore::new(
                crate::state::prompt_history::PromptHistoryStore::default_path(),
                crate::config::default_max_prompt_history(),
            ),
            session_switcher: None,
            adapt_screen: None,
            pr_review_screen: None,
            release_notes_screen: None,
        }
    }

    pub fn configure(&mut self, config: Config) {
        self.fork_policy = Some(ForkPolicy::new(
            config.sessions.context_overflow.max_fork_depth,
        ));
        let guardrail = crate::prompts::resolve_guardrail(
            config.sessions.guardrail_prompt.as_deref(),
            &std::path::PathBuf::from("."),
        );
        self.pool.set_guardrail_prompt(guardrail);
        let mut theme = Theme::from_config(&config.tui.theme);
        theme.apply_capability(crate::tui::theme::ColorCapability::detect());
        self.theme = theme;
        self.prompt_history
            .set_max_entries(config.sessions.max_prompt_history);
        if let Err(e) = self.prompt_history.load() {
            self.activity_log.push_simple(
                "HISTORY".into(),
                format!("Failed to load prompt history: {}", e),
                LogLevel::Warn,
            );
        }
        self.config = Some(config);
    }

    /// Returns the preview theme if set, otherwise the base theme.
    pub fn active_theme(&self) -> &Theme {
        self.preview_theme.as_ref().unwrap_or(&self.theme)
    }

    fn check_gh_auth_error(&mut self, e: &anyhow::Error) -> bool {
        if crate::github::client::is_gh_auth_error(e) {
            self.gh_auth_ok = false;
            self.activity_log.push_simple(
                "AUTH".into(),
                "GitHub authentication lost. Run `gh auth login` to re-authenticate.".into(),
                LogLevel::Error,
            );
            true
        } else {
            false
        }
    }

    fn log_gh_auth_skip(&mut self, issue_number: u64, operation: &str) {
        self.activity_log.push_simple(
            format!("#{}", issue_number),
            format!(
                "Skipping {} — GitHub not authenticated. Run `gh auth login`",
                operation
            ),
            LogLevel::Warn,
        );
    }

    fn sync_state(&mut self) {
        self.state.sessions = self.pool.all_sessions().into_iter().cloned().collect();
        self.state.update_total_cost();
        self.total_cost = self.state.total_cost_usd;
        self.state.last_updated = Some(Utc::now());
        let _ = self.store.save(&self.state);
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
