mod auto_pr;
mod budget;
mod bypass;
mod ci_polling;
pub(crate) mod clipboard_action;
mod completion_git;
mod completion_pipeline;
mod completion_summary;
mod context_overflow;
pub(crate) mod data_handler;
mod event_handler;
mod gate_retry;
pub(crate) mod helpers;
mod issue_completion;
mod plugins;
mod pr_retry;
mod pushup_marker;
mod review;
mod session_lifecycle;
mod session_spawners;
mod settings_actions;
pub mod types;
pub mod work_assigner;

pub use types::*;

use crate::budget::BudgetEnforcer;
use crate::config::Config;
use crate::continuous::ContinuousModeState;
use crate::mascot::MascotAnimator;
use crate::mascot::animator::SystemClock;
use crate::models::ModelRouter;
use crate::notifications::desktop::{DesktopNotifier, OsascriptNotifier};
use crate::notifications::dispatcher::NotificationDispatcher;
use crate::plugins::runner::PluginRunner;
use crate::provider::github::client::GitHubClient;
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
use crate::tui::app::work_assigner::WorkAssignmentService;
use crate::tui::panels::PanelView;
use crate::tui::theme::Theme;
use chrono::Utc;
pub use ci_polling::CiPoller;
use std::time::Instant;
use tokio::sync::mpsc;

/// Single source of truth for the "GitHub auth missing — recover" hint.
/// Reused at every site that tells the user what to do once `gh auth
/// login` finishes. Without this, the same advice ships in multiple
/// phrasings and drifts over time.
pub(crate) const AUTH_RECOVERY_HINT: &str = "Run `gh auth login` then press Shift+P to retry.";

pub struct App {
    pub pool: SessionPool,
    pub activity_log: ActivityLog,
    pub panel_view: PanelView,
    pub state: MaestroState,
    pub store: StateStore,
    pub turboquant_adapter: Option<std::sync::Arc<crate::turboquant::adapter::TurboQuantAdapter>>,
    pub running: bool,
    pub total_cost: f64,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    pub work_assignment_service: Option<WorkAssignmentService>,
    pub github_client: Option<Box<dyn GitHubClient>>,
    pub config: Option<Config>,
    pub config_path: Option<std::path::PathBuf>,
    pub(crate) pending_issue_completions: Vec<PendingIssueCompletion>,
    pub(crate) pending_hooks: Vec<PendingHook>,
    pub health_monitor: Box<dyn HealthCheck>,
    /// Gate runner used by `App::retry_completion_gates` ([g] on the
    /// failed-gates recovery modal — issue #560). Production wires
    /// `GateRunner`; tests inject `CapturingGateRunner` via
    /// `with_gate_runner`. Kept on `App` instead of a free function so
    /// the `[g]` keybinding handler has a stable injection point.
    pub gate_runner: Box<dyn crate::gates::runner::GateCheck>,
    pub budget_enforcer: Option<BudgetEnforcer>,
    pub model_router: Option<ModelRouter>,
    pub progress_tracker: ProgressTracker,
    pub notifications: NotificationDispatcher,
    pub tui_mode: TuiMode,
    /// Navigation back-stack for consistent [Esc] behavior.
    pub nav_stack: NavigationStack,
    pub session_logger: SessionLogger,
    pub ci_poller: CiPoller,
    pub(crate) last_work_tick: Instant,
    pub plugin_runner: Option<PluginRunner>,
    pub help_state: Option<crate::tui::help::HelpOverlayState>,
    pub cached_mode_km: Option<crate::tui::navigation::keymap::ModeKeyMap>,
    pub cached_mode_km_key: (TuiMode, Option<crate::session::types::SessionStatus>, bool),
    pub context_monitor: Box<dyn ContextMonitor>,
    pub fork_policy: Option<ForkPolicy>,
    pub screen_state: ScreenState,
    pub session_config: crate::tui::app::types::SessionConfig,
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
    pub pending_prs: Vec<crate::provider::github::types::PendingPr>,
    pub flags: crate::flags::store::FeatureFlags,
    pub queue_executor: Option<crate::work::executor::QueueExecutor>,
    pub queue_launch_configs: Option<Vec<crate::tui::screens::SessionConfig>>,
    pub prompt_history: crate::state::prompt_history::PromptHistoryStore,
    pub tool_start_times: std::collections::HashMap<uuid::Uuid, (String, Instant)>,
    pub no_splash: bool,
    pub show_mascot: bool,
    pub mascot_style: crate::mascot::MascotStyle,
    pub mascot_animator: MascotAnimator,
    pub session_ui_state: std::collections::HashMap<uuid::Uuid, SessionUiState>,
    pub log_viewer_scroll: u16,
    pub log_viewer_cache: crate::tui::log_viewer::LogViewerCache,
    pub session_summary_state: Option<crate::tui::app::types::SessionSummaryState>,
    pub show_activity_log: bool,
    /// Marquee animation state for the top status bar (#417). Scrolls
    /// when the assembled spans exceed the viewport width.
    pub status_bar_marquee: crate::tui::marquee::MarqueeState,
    /// Cheap content fingerprint (total span char width) used to reset
    /// `status_bar_marquee` when the bar's identity changes (breadcrumb
    /// depth, agent count, TQ toggle, …).
    pub status_bar_marquee_fingerprint: usize,
    pub resource_monitor: Box<dyn crate::system::monitor::ResourceMonitor>,
    /// Bypass mode (#328): when true, the session pool runs Claude with
    /// `bypassPermissions` and review corrections auto-apply. Source-of-truth
    /// for the indicator widget and the CONFIRM-typing warning gate.
    pub bypass_active: bool,
    /// One-shot per session: have we already shown the full-screen warning?
    pub bypass_warning_acknowledged: bool,
    /// Live PRD (#321) loaded from `.maestro/prd.toml`; lazily populated by
    /// the PRD screen on first entry.
    pub prd: Option<crate::prd::model::Prd>,
    /// Last completed `/review` cycle (#327). Populated by data_handler;
    /// consumed by the PR-review screen on next render.
    pub pending_review_report: Option<crate::review::types::ReviewReport>,
    /// Cursor into `pending_review_report.concerns` for the panel UI.
    pub concerns_cursor: usize,
    /// PRD sources discovered during the last sync. Surfaced via the
    /// `[o]` Explore key on the PRD screen.
    pub prd_candidates: Vec<crate::prd::discover::DiscoveredPrd>,
    /// Pre-parsed `IngestedPrd` for each candidate (1:1 with
    /// `prd_candidates`). Populated once when candidates land so the
    /// explore renderer doesn't re-parse the markdown on every frame.
    pub prd_candidate_parsed: Vec<crate::prd::ingest::IngestedPrd>,
    /// Whether the PRD screen is currently showing the explore panel.
    pub prd_show_explore: bool,
    /// Cursor into `prd_candidates` while the explore panel is open.
    pub prd_explore_cursor: usize,
    /// Reads/writes `.claude/settings.json` for the caveman_mode toggle (#490).
    /// `None` when not yet wired (pre-`with_settings_store` or in tests).
    pub settings_store: Option<Box<dyn crate::settings::SettingsStore + Send>>,
    /// System clipboard adapter for the `c` Copy keybinding. Tests
    /// inject a `MockClipboard` via `with_clipboard`.
    pub(crate) clipboard: Box<dyn crate::tui::clipboard::Clipboard>,
    /// Shell launcher for the `[s] Shell into worktree` recovery key on
    /// the failed-gates modal (#560). Production wires `OsShellLauncher`;
    /// tests inject `CapturingShellLauncher` via `with_shell_launcher`.
    pub(crate) shell_launcher: Box<dyn crate::tui::shell_launcher::ShellLauncher>,
    /// Transient (~2 s) banner shown after a copy action.
    pub(crate) copy_toast: Option<crate::tui::app::clipboard_action::CopyToast>,
    /// Desktop notification dispatcher. Production wires `OsascriptNotifier`;
    /// tests inject `FakeNotifier` via `with_desktop_notifier`.
    pub(crate) desktop_notifier: std::sync::Arc<dyn DesktopNotifier>,
    /// Issue numbers for which auto-PR creation has already been attempted
    /// in this process. Closes the in-process double-fire path of #514's
    /// AC7.
    ///
    /// Idempotency is dual-layer:
    /// - **In-process** (this set): blocks the same App tick / duplicate
    ///   `Completed` event from firing `create_pr` twice in one run.
    /// - **Cross-restart** (the `list_prs_for_branch` preflight in
    ///   `auto_pr::run_auto_pr`): blocks a second run from creating a
    ///   duplicate after a crash that lost the in-memory state.
    ///
    /// Both windows are needed; neither alone closes the gap. The set is
    /// not persisted because the preflight covers the cross-restart case
    /// at the GitHub layer (the only authority that ultimately matters).
    /// Memory growth is bounded by `usize` issue numbers per maestro run
    /// (8 bytes each); single-user, dozens-of-sessions threat model.
    pub(crate) attempted_pr_issue_numbers: std::collections::HashSet<u64>,
    /// Git operations adapter (#520). Production wires `CliGitOps`;
    /// tests inject `MockGitOps` via `with_git_ops`. Used by the auto-PR
    /// pipeline's zero-commit gate.
    pub(crate) git_ops: Box<dyn crate::git::GitOps>,
    /// Override for `$HOME` in tests. Production reads `std::env::var("HOME")`.
    /// Used by `pushup_marker` to find `~/.maestro/last-pr-created`.
    pub(crate) home_dir_override: Option<std::path::PathBuf>,
    /// Last-seen mtime of `~/.maestro/last-pr-created`. Prevents re-firing
    /// `TuiCommand::PrCreated` on every tick when the marker hasn't moved.
    pub(crate) last_pr_marker_mtime: Option<std::time::SystemTime>,
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
        let (state, state_load_error) = match store.load() {
            Ok(state) => (state, None),
            Err(e) => (MaestroState::default(), Some(e.to_string())),
        };
        let mut pool = SessionPool::new(max_concurrent, worktree_mgr, event_tx);
        pool.set_permission_mode(permission_mode.clone());
        pool.set_allowed_tools(allowed_tools.clone());
        let session_config = crate::tui::app::types::SessionConfig::new(
            max_concurrent,
            permission_mode,
            allowed_tools,
        );
        // Recover any pending completions persisted from a prior run so the
        // auto-PR work is retried on next tick (#514). The AC4 preflight
        // (PR-already-exists check) prevents double-firing when the prior
        // run actually succeeded but crashed before clearing the entry.
        let recovered_completions = state.pending_completions.clone();
        // Recover the pending-PR retry queue from prior run so Shift+P
        // (#521) can resurrect them after a maestro restart. The schema has
        // always supported this; only the rehydration was missing.
        // Apply PENDING_PRS_REHYDRATE_CAP defensively: a corrupt or
        // maliciously-crafted state file with millions of entries would
        // otherwise OOM App::new.
        let original_pending_prs_count = state.pending_prs.len();
        let mut recovered_prs = state.pending_prs.clone();
        let pending_prs_truncated =
            original_pending_prs_count > crate::provider::github::types::PENDING_PRS_REHYDRATE_CAP;
        if pending_prs_truncated {
            recovered_prs.truncate(crate::provider::github::types::PENDING_PRS_REHYDRATE_CAP);
        }
        let recovered_prs_count = recovered_prs.len();
        let mut app = Self {
            pool,
            activity_log: ActivityLog::new(500),
            panel_view: PanelView::new(),
            state,
            store,
            turboquant_adapter: None,
            running: true,
            total_cost: 0.0,
            start_time: Utc::now(),
            event_rx,
            work_assignment_service: None,
            github_client: None,
            config: None,
            config_path: None,
            pending_issue_completions: recovered_completions,
            pending_hooks: Vec::new(),
            health_monitor: Box::new(HealthMonitor::new()),
            gate_runner: Box::new(crate::gates::runner::GateRunner),
            budget_enforcer: None,
            model_router: None,
            progress_tracker: ProgressTracker::new(),
            notifications: NotificationDispatcher::new(false),
            tui_mode: TuiMode::Overview,
            nav_stack: NavigationStack::default(),
            session_logger: SessionLogger::new(SessionLogger::default_dir()),
            ci_poller: CiPoller::default(),
            last_work_tick: Instant::now(),
            plugin_runner: None,
            help_state: None,
            cached_mode_km: None,
            cached_mode_km_key: (TuiMode::Overview, None, false),
            context_monitor: Box::new(ProductionContextMonitor::new()),
            fork_policy: None,
            screen_state: ScreenState::default(),
            session_config,
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
            pending_prs: recovered_prs,
            flags: crate::flags::store::FeatureFlags::default(),
            queue_executor: None,
            queue_launch_configs: None,
            prompt_history: crate::state::prompt_history::PromptHistoryStore::new(
                crate::state::prompt_history::PromptHistoryStore::default_path(),
                crate::config::default_max_prompt_history(),
            ),
            no_splash: false,
            show_mascot: true,
            mascot_style: crate::mascot::MascotStyle::default(),
            mascot_animator: MascotAnimator::new(&SystemClock),
            tool_start_times: std::collections::HashMap::new(),
            session_ui_state: std::collections::HashMap::new(),
            log_viewer_scroll: 0,
            log_viewer_cache: crate::tui::log_viewer::LogViewerCache::default(),
            session_summary_state: None,
            show_activity_log: true,
            resource_monitor: Box::new(crate::system::SysInfoMonitor::new(1000)),
            status_bar_marquee: crate::tui::marquee::MarqueeState::new(),
            status_bar_marquee_fingerprint: 0,
            bypass_active: false,
            bypass_warning_acknowledged: false,
            prd: None,
            pending_review_report: None,
            concerns_cursor: 0,
            prd_candidates: Vec::new(),
            prd_candidate_parsed: Vec::new(),
            prd_show_explore: false,
            prd_explore_cursor: 0,
            settings_store: None,
            clipboard: Box::new(crate::tui::clipboard::SystemClipboard),
            shell_launcher: Box::new(crate::tui::shell_launcher::OsShellLauncher),
            copy_toast: None,
            desktop_notifier: std::sync::Arc::new(OsascriptNotifier::new(false)),
            attempted_pr_issue_numbers: std::collections::HashSet::new(),
            git_ops: Box::new(crate::git::CliGitOps),
            home_dir_override: None,
            last_pr_marker_mtime: None,
        };
        if let Some(error) = state_load_error {
            app.activity_log.push_simple(
                "State".into(),
                format!("Failed to load persisted state: {}", error),
                LogLevel::Error,
            );
            app.notifications.notify(
                crate::notifications::types::InterruptLevel::Critical,
                "State load failed",
                &error,
            );
        }
        if pending_prs_truncated {
            app.activity_log.push_simple(
                "#orphan-prs".into(),
                format!(
                    "Truncated pending_prs from {} to {} on rehydrate — state file may be corrupt; excess entries dropped to protect process memory",
                    original_pending_prs_count,
                    crate::provider::github::types::PENDING_PRS_REHYDRATE_CAP,
                ),
                LogLevel::Warn,
            );
        }
        if recovered_prs_count > 0 {
            // List the actual issue numbers so the user knows which panels
            // to focus before pressing Shift+P, instead of guessing across
            // the whole pool.
            let issue_list: Vec<String> = app
                .pending_prs
                .iter()
                .map(|p| format!("#{}", p.issue_number))
                .collect();
            app.activity_log.push_simple(
                "#orphan-prs".into(),
                format!(
                    "{} pending PR(s) restored from previous run: {} — focus the matching session and press Shift+P to retry",
                    recovered_prs_count,
                    issue_list.join(", ")
                ),
                LogLevel::Warn,
            );
        }
        app
    }

    /// Builder for tests: swap the production git ops adapter for a fake.
    #[cfg(test)]
    pub(crate) fn with_git_ops(mut self, git_ops: Box<dyn crate::git::GitOps>) -> Self {
        self.git_ops = git_ops;
        self
    }

    /// Builder for tests: override the `$HOME` lookup so the marker
    /// watcher reads from a tempdir instead of the real home.
    #[cfg(test)]
    pub(crate) fn with_home_dir(mut self, home: std::path::PathBuf) -> Self {
        self.home_dir_override = Some(home);
        self
    }

    /// Builder for tests: swap the system clipboard for a fake.
    #[cfg(test)]
    pub(crate) fn with_clipboard(
        mut self,
        clipboard: Box<dyn crate::tui::clipboard::Clipboard>,
    ) -> Self {
        self.clipboard = clipboard;
        self
    }

    /// Builder for tests: swap the shell launcher for a `CapturingShellLauncher`.
    #[cfg(test)]
    pub(crate) fn with_shell_launcher(
        mut self,
        launcher: Box<dyn crate::tui::shell_launcher::ShellLauncher>,
    ) -> Self {
        self.shell_launcher = launcher;
        self
    }

    /// Builder for tests: swap the gate runner for a fake.
    #[cfg(test)]
    pub(crate) fn with_gate_runner(
        mut self,
        runner: Box<dyn crate::gates::runner::GateCheck>,
    ) -> Self {
        self.gate_runner = runner;
        self
    }

    /// Builder for tests: swap the desktop notifier for a fake.
    #[cfg(test)]
    pub(crate) fn with_desktop_notifier(
        mut self,
        notifier: std::sync::Arc<dyn DesktopNotifier>,
    ) -> Self {
        self.desktop_notifier = notifier;
        self
    }

    /// Drain any pending desktop-notifier error into the activity log.
    /// Called once per render frame from `tui::ui::draw`.
    pub fn tick_notify_error(&mut self) {
        let Some(err) = self.desktop_notifier.take_last_error() else {
            return;
        };
        let msg = match err {
            crate::notifications::desktop::NotifyError::PermissionDenied => {
                "Desktop notifications blocked. Grant access in System Settings → Notifications."
                    .to_string()
            }
            crate::notifications::desktop::NotifyError::DispatchFailed(m) => {
                format!("Desktop notification failed: {}", m)
            }
            crate::notifications::desktop::NotifyError::Internal(m) => {
                format!("Desktop notification internal error: {}", m)
            }
        };
        self.activity_log
            .push_simple("NOTIFICATIONS".into(), msg, LogLevel::Warn);
    }

    pub fn configure(&mut self, config: Config) {
        self.session_config.apply_config(&config.sessions);
        if self.bypass_active {
            self.session_config.permission_mode = "bypassPermissions".to_string();
        }

        // Shared adapter so fork and pool observe the same enabled state.
        let tq_adapter = if config.turboquant.enabled {
            Some(std::sync::Arc::new(
                crate::turboquant::adapter::TurboQuantAdapter::new(config.turboquant.bit_width),
            ))
        } else {
            None
        };

        let mut fp = ForkPolicy::new(config.sessions.context_overflow.max_fork_depth);
        if let Some(ref adapter) = tq_adapter {
            fp = fp.with_turboquant(
                std::sync::Arc::clone(adapter),
                config.turboquant.fork_handoff_budget,
            );
        }
        self.fork_policy = Some(fp);

        if let Some(ref adapter) = tq_adapter {
            self.pool.set_turboquant_adapter(
                std::sync::Arc::clone(adapter),
                config.turboquant.system_prompt_budget,
            );
        }
        self.pool
            .set_knowledge_appendix(crate::adapt::knowledge::load_appendix());
        self.turboquant_adapter = tq_adapter;

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
        crate::icon_mode::init_from_config(config.tui.ascii_icons);
        self.show_mascot = config.tui.show_mascot;
        self.mascot_style = config.tui.mascot_style;
        self.desktop_notifier =
            std::sync::Arc::new(OsascriptNotifier::new(config.notifications.desktop));
        // Sync TurboQuant flag from [turboquant] config section
        if config.turboquant.enabled {
            self.flags.set_enabled(crate::flags::Flag::TurboQuant, true);
        }
        self.config = Some(config);
    }

    /// Record the filesystem path the config was loaded from, so the Settings
    /// screen can save back to the same file regardless of CWD at save time.
    pub fn set_config_path(&mut self, path: std::path::PathBuf) {
        self.config_path = Some(path);
    }

    /// Returns the preview theme if set, otherwise the base theme.
    pub fn active_theme(&self) -> &Theme {
        self.preview_theme.as_ref().unwrap_or(&self.theme)
    }

    fn check_gh_auth_error(&mut self, e: &anyhow::Error) -> bool {
        if crate::provider::github::client::is_gh_auth_error(e) {
            self.gh_auth_ok = false;
            self.activity_log.push_simple(
                "AUTH".into(),
                format!("GitHub authentication lost. {}", AUTH_RECOVERY_HINT),
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
                "Skipping {} — GitHub not authenticated. {}",
                operation, AUTH_RECOVERY_HINT
            ),
            LogLevel::Warn,
        );
    }

    /// Agent-graph view toggle. Fails closed when no config is loaded.
    pub fn is_agent_graph_enabled(&self) -> bool {
        self.config
            .as_ref()
            .map(|c| c.views.agent_graph_enabled)
            .unwrap_or(false)
    }

    /// Bypasses `navigate_to` deliberately so `Esc` does not pop back through
    /// every toggle press — this is a flat view-switch, not navigation.
    pub fn toggle_agent_graph(&mut self) {
        if !self.is_agent_graph_enabled() {
            return;
        }
        self.tui_mode = match self.tui_mode {
            TuiMode::Overview => TuiMode::AgentGraph,
            TuiMode::AgentGraph => TuiMode::Overview,
            _ => return,
        };
    }

    /// Navigate to a new mode, pushing the current mode onto the back-stack.
    /// Navigate to `target`, maintaining a back-stack invariant that
    /// disallows duplicates at the top and collapses cycles.
    ///
    /// Rules:
    /// - Same-mode nav is a no-op (repeatedly pressing F5 while on
    ///   TokenDashboard must not grow the breadcrumb trail).
    /// - If `target` already appears in the stack, truncate to that
    ///   position instead of pushing (A → B → A collapses to just [A]
    ///   rather than [A, B, A], so `Esc` takes the user back one real
    ///   step, not one keypress).
    pub fn navigate_to(&mut self, target: TuiMode) {
        // Defense-in-depth; canonical gate is in tui::ui::draw.
        let target = if target == TuiMode::AgentGraph && !self.is_agent_graph_enabled() {
            TuiMode::Overview
        } else {
            target
        };

        if self.tui_mode == target {
            return;
        }
        // Post-fix the stack cannot contain duplicates (same-mode pushes
        // are blocked above, and cycles are collapsed on entry), so the
        // first match IS the only match. `position` communicates that
        // invariant more directly than `rposition`.
        if let Some(idx) = self
            .nav_stack
            .breadcrumbs()
            .iter()
            .position(|m| *m == target)
        {
            self.nav_stack.truncate_to(idx);
        } else {
            self.nav_stack.push(self.tui_mode);
        }
        self.tui_mode = target;
    }

    /// Navigate back. If the stack is empty, trigger ConfirmExit.
    pub fn navigate_back(&mut self) {
        if let Some(prev) = self.nav_stack.pop() {
            self.tui_mode = prev;
        } else {
            self.tui_mode = TuiMode::ConfirmExit;
        }
    }

    /// Navigate back without triggering ConfirmExit (for cancel flows).
    /// Falls back to Dashboard if stack is empty.
    pub fn navigate_back_or_dashboard(&mut self) {
        if let Some(prev) = self.nav_stack.pop() {
            self.tui_mode = prev;
        } else {
            self.tui_mode = TuiMode::Dashboard;
        }
    }

    pub fn navigate_to_root(&mut self) {
        self.nav_stack.clear();
        self.tui_mode = TuiMode::Dashboard;
    }

    fn sync_state(&mut self) {
        self.state.sessions = self.pool.all_sessions().into_iter().cloned().collect();
        self.state.update_total_cost();
        self.total_cost = self.state.total_cost_usd;
        self.state.last_updated = Some(Utc::now());
        // Mirror in-memory pending completions to persisted state so a
        // shutdown between session-end and the next check_completions tick
        // does not orphan the worktree (#514).
        self.state.pending_completions = self.pending_issue_completions.clone();
        // Mirror the pending-PR retry queue so Shift+P (#521) can resurrect
        // entries after a restart — without this the in-memory queue is
        // lost on shutdown.
        self.state.pending_prs = self.pending_prs.clone();
        if let Err(e) = self.store.save(&self.state) {
            let message = e.to_string();
            self.activity_log.push_simple(
                "State".into(),
                format!("Failed to save persisted state: {}", message),
                LogLevel::Error,
            );
            self.notifications.notify(
                crate::notifications::types::InterruptLevel::Critical,
                "State save failed",
                &message,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::Flag;

    fn make_app() -> crate::tui::app::App {
        crate::tui::make_test_app("maestro-tui-app-test")
    }

    fn make_app_with_flags(flags: crate::flags::store::FeatureFlags) -> App {
        let mut app = make_app();
        app.flags = flags;
        app
    }

    #[test]
    fn app_flags_field_defaults_to_feature_flags_default() {
        let app = make_app();
        assert!(
            app.flags.is_enabled(Flag::ContinuousMode),
            "app.flags must default ContinuousMode to true"
        );
        assert!(
            app.flags.is_enabled(Flag::AutoFork),
            "app.flags must default AutoFork to true"
        );
        assert!(
            !app.flags.is_enabled(Flag::CiAutoFix),
            "app.flags must default CiAutoFix to false"
        );
    }

    #[test]
    fn app_flags_can_be_replaced_after_construction() {
        let mut app = make_app();
        let custom = crate::flags::store::FeatureFlags::new(
            std::collections::HashMap::new(),
            vec!["ci_auto_fix".to_string()],
            vec![],
        );
        app.flags = custom;
        assert!(
            app.flags.is_enabled(Flag::CiAutoFix),
            "app.flags must reflect newly assigned FeatureFlags"
        );
    }

    #[test]
    fn continuous_mode_not_set_when_flag_disabled() {
        let flags = crate::flags::store::FeatureFlags::new(
            std::collections::HashMap::new(),
            vec![],
            vec!["continuous_mode".to_string()],
        );
        let mut app = make_app_with_flags(flags);
        // Simulate gating logic from cmd_run
        let cli_continuous = true;
        if app.flags.is_enabled(Flag::ContinuousMode) && cli_continuous {
            app.continuous_mode = Some(ContinuousModeState::new());
        }
        assert!(
            app.continuous_mode.is_none(),
            "continuous_mode must remain None when Flag::ContinuousMode is disabled"
        );
    }

    #[test]
    fn continuous_mode_set_when_flag_enabled() {
        let mut app = make_app();
        let cli_continuous = true;
        if app.flags.is_enabled(Flag::ContinuousMode) && cli_continuous {
            app.continuous_mode = Some(ContinuousModeState::new());
        }
        assert!(
            app.continuous_mode.is_some(),
            "continuous_mode must be Some when Flag::ContinuousMode is enabled"
        );
    }

    #[test]
    fn check_context_overflow_skips_fork_when_auto_fork_flag_disabled() {
        let flags = crate::flags::store::FeatureFlags::new(
            std::collections::HashMap::new(),
            vec![],
            vec!["auto_fork".to_string()],
        );
        let mut app = make_app_with_flags(flags);
        app.fork_policy = Some(crate::session::fork::ForkPolicy::new(5));
        let dummy_id = uuid::Uuid::new_v4();
        app.check_context_overflow(dummy_id);
        assert!(
            app.pending_session_launches.is_empty(),
            "check_context_overflow must not fork when Flag::AutoFork is disabled"
        );
    }

    fn make_app_with_views_toml(extra: &str) -> App {
        let mut app = make_app();
        let toml = format!(
            "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n\
         per_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
         [github]\n[notifications]\n{extra}"
        );
        app.config = Some(toml::from_str(&toml).expect("test config parse"));
        app
    }

    #[test]
    fn navigate_to_agent_graph_with_toggle_off_redirects_to_overview() {
        let mut app = make_app_with_views_toml("");
        app.tui_mode = TuiMode::Overview;
        app.navigate_to(TuiMode::AgentGraph);
        assert_eq!(
            app.tui_mode,
            TuiMode::Overview,
            "navigate_to(AgentGraph) with toggle OFF must redirect to Overview"
        );
    }

    #[test]
    fn navigate_to_agent_graph_with_toggle_on_succeeds() {
        let mut app = make_app_with_views_toml("[views]\nagent_graph_enabled = true\n");
        app.tui_mode = TuiMode::Overview;
        app.navigate_to(TuiMode::AgentGraph);
        assert_eq!(
            app.tui_mode,
            TuiMode::AgentGraph,
            "navigate_to(AgentGraph) with toggle ON must land on AgentGraph"
        );
    }

    #[test]
    fn navigate_to_agent_graph_with_no_config_redirects_to_overview() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Overview;
        app.navigate_to(TuiMode::AgentGraph);
        assert_eq!(
            app.tui_mode,
            TuiMode::Overview,
            "navigate_to(AgentGraph) with no config (None) must redirect to Overview"
        );
    }

    // -- Issue #528: App::toggle_agent_graph ---------------------------------

    #[test]
    fn toggle_agent_graph_with_flag_off_is_noop() {
        let mut app = make_app_with_views_toml("[views]\nagent_graph_enabled = false\n");
        app.tui_mode = TuiMode::Overview;
        app.toggle_agent_graph();
        assert_eq!(app.tui_mode, TuiMode::Overview);
        assert!(app.nav_stack.is_empty());
    }

    #[test]
    fn toggle_agent_graph_with_no_config_is_noop() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Overview;
        app.toggle_agent_graph();
        assert_eq!(app.tui_mode, TuiMode::Overview);
    }

    #[test]
    fn toggle_agent_graph_overview_to_graph_when_enabled() {
        let mut app = make_app_with_views_toml("[views]\nagent_graph_enabled = true\n");
        app.tui_mode = TuiMode::Overview;
        app.toggle_agent_graph();
        assert_eq!(app.tui_mode, TuiMode::AgentGraph);
    }

    #[test]
    fn toggle_agent_graph_graph_to_overview_when_enabled() {
        let mut app = make_app_with_views_toml("[views]\nagent_graph_enabled = true\n");
        app.tui_mode = TuiMode::AgentGraph;
        app.toggle_agent_graph();
        assert_eq!(app.tui_mode, TuiMode::Overview);
    }

    #[test]
    fn toggle_agent_graph_round_trip_returns_to_overview() {
        let mut app = make_app_with_views_toml("[views]\nagent_graph_enabled = true\n");
        app.tui_mode = TuiMode::Overview;
        app.toggle_agent_graph();
        assert_eq!(app.tui_mode, TuiMode::AgentGraph);
        app.toggle_agent_graph();
        assert_eq!(app.tui_mode, TuiMode::Overview);
    }

    #[test]
    fn toggle_agent_graph_from_unrelated_mode_is_noop() {
        let mut app = make_app_with_views_toml("[views]\nagent_graph_enabled = true\n");
        app.tui_mode = TuiMode::Settings;
        app.toggle_agent_graph();
        assert_eq!(
            app.tui_mode,
            TuiMode::Settings,
            "toggle must only act on Overview/AgentGraph"
        );
    }

    #[test]
    fn toggle_agent_graph_does_not_push_nav_stack() {
        let mut app = make_app_with_views_toml("[views]\nagent_graph_enabled = true\n");
        app.tui_mode = TuiMode::Overview;
        let depth_before = app.nav_stack.depth();
        app.toggle_agent_graph();
        app.toggle_agent_graph();
        assert_eq!(
            app.nav_stack.depth(),
            depth_before,
            "toggle must not grow nav_stack — Esc should not pop back through toggle history"
        );
    }

    mod pending_prs_persistence {
        use super::*;
        use crate::provider::github::types::PendingPr;
        use crate::session::worktree::MockWorktreeManager;
        use crate::state::store::StateStore;
        use crate::state::types::MaestroState;
        use crate::tui::activity_log::LogLevel;

        fn make_pending_pr(issue_number: u64) -> PendingPr {
            let mut p = crate::provider::github::types::awaiting_pending_pr(issue_number);
            p.attempt = 1;
            p.last_errors.push_back("boom".into());
            p
        }

        fn build_app_with_seeded_state(state: MaestroState) -> App {
            let path = std::env::temp_dir().join(format!(
                "pending-prs-rehydrate-{}.json",
                uuid::Uuid::new_v4()
            ));
            let store = StateStore::new(path);
            store.save(&state).expect("seed save");
            App::new(
                store,
                3,
                Box::new(MockWorktreeManager::new()),
                "bypassPermissions".into(),
                vec![],
            )
        }

        fn build_app_with_raw_state(contents: &[u8]) -> App {
            let path = std::env::temp_dir()
                .join(format!("state-load-error-{}.json", uuid::Uuid::new_v4()));
            if let Err(e) = std::fs::write(&path, contents) {
                panic!("raw state should be written: {e}");
            }
            App::new(
                StateStore::new(path),
                3,
                Box::new(MockWorktreeManager::new()),
                "bypassPermissions".into(),
                vec![],
            )
        }

        #[test]
        fn app_new_rehydrates_pending_prs_from_persisted_state() {
            let mut seed = MaestroState::default();
            seed.pending_prs.push(make_pending_pr(7));
            seed.pending_prs.push(make_pending_pr(11));

            let app = build_app_with_seeded_state(seed);

            assert_eq!(
                app.pending_prs.len(),
                2,
                "App::new must rehydrate state.pending_prs (was lost previously)"
            );
            let issue_numbers: Vec<u64> = app.pending_prs.iter().map(|p| p.issue_number).collect();
            assert!(issue_numbers.contains(&7));
            assert!(issue_numbers.contains(&11));
        }

        #[test]
        fn app_new_logs_warn_when_pending_prs_recovered() {
            let mut seed = MaestroState::default();
            seed.pending_prs.push(make_pending_pr(7));
            seed.pending_prs.push(make_pending_pr(11));

            let app = build_app_with_seeded_state(seed);

            let warn =
                app.activity_log.entries().iter().find(|e| {
                    matches!(e.level, LogLevel::Warn) && e.message.contains("pending PR")
                });
            let warn = warn.expect("orphan-PR warn entry must be present");
            assert!(
                warn.message.contains("2 pending PR"),
                "got: {}",
                warn.message
            );
            assert!(
                warn.message.contains("Shift+P"),
                "the warn must mention Shift+P so users know how to recover"
            );
            assert_eq!(warn.session_label, "#orphan-prs");
        }

        #[test]
        fn app_new_notifies_when_state_load_fails() {
            let app = build_app_with_raw_state(b"{not valid json");

            let error_log = app.activity_log.entries().iter().any(|e| {
                matches!(e.level, LogLevel::Error)
                    && e.session_label == "State"
                    && e.message.contains("Failed to load persisted state")
            });
            assert!(
                error_log,
                "state load failure must be visible in the TUI log"
            );
            assert!(
                app.notifications
                    .active_banners()
                    .iter()
                    .any(|n| n.title == "State load failed"),
                "state load failure must create a TUI notification banner"
            );
        }

        #[test]
        fn app_new_truncates_pending_prs_above_rehydrate_cap() {
            use crate::provider::github::types::PENDING_PRS_REHYDRATE_CAP;
            let mut seed = MaestroState::default();
            for i in 0..(PENDING_PRS_REHYDRATE_CAP + 1) {
                seed.pending_prs.push(make_pending_pr(i as u64));
            }
            let app = build_app_with_seeded_state(seed);

            assert_eq!(
                app.pending_prs.len(),
                PENDING_PRS_REHYDRATE_CAP,
                "App::new must truncate pending_prs to PENDING_PRS_REHYDRATE_CAP"
            );
            let truncate_warn = app.activity_log.entries().iter().any(|e| {
                matches!(e.level, LogLevel::Warn) && e.message.contains("Truncated pending_prs")
            });
            assert!(
                truncate_warn,
                "App::new must emit a Warn log entry containing 'Truncated pending_prs' when the cap is exceeded",
            );
        }

        #[test]
        fn auth_recovery_hint_const_is_well_formed() {
            use super::AUTH_RECOVERY_HINT;
            assert!(!AUTH_RECOVERY_HINT.is_empty());
            assert!(
                AUTH_RECOVERY_HINT.contains("gh auth login"),
                "AUTH_RECOVERY_HINT must reference `gh auth login` so users know how to recover"
            );
        }

        // ── /pushup → maestro auto-review hand-off via marker file ──

        fn marker_path(home: &std::path::Path) -> std::path::PathBuf {
            home.join(".maestro").join("last-pr-created")
        }

        fn make_app_with_home(home: std::path::PathBuf) -> App {
            let path =
                std::env::temp_dir().join(format!("marker-test-{}.json", uuid::Uuid::new_v4()));
            let store = StateStore::new(path);
            App::new(
                store,
                3,
                Box::new(MockWorktreeManager::new()),
                "bypassPermissions".into(),
                vec![],
            )
            .with_home_dir(home)
        }

        #[tokio::test]
        async fn poll_pr_marker_fresh_enqueues_pr_created_and_deletes_file() {
            let home = tempfile::tempdir().unwrap();
            let marker = marker_path(home.path());
            std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
            std::fs::write(
                &marker,
                r#"{"pr_number":42,"owner":"o","repo":"r","ts":"2026-04-30T00:00:00Z"}"#,
            )
            .unwrap();

            let mut app = make_app_with_home(home.path().to_path_buf());
            app.poll_last_pr_created_marker().await;

            assert!(
                !marker.exists(),
                "fresh marker must be deleted after consume (consume-once semantics)"
            );
            let queued = app
                .pending_commands
                .iter()
                .any(|c| matches!(c, TuiCommand::PrCreated { pr_number: 42, .. }));
            assert!(queued, "fresh marker must enqueue TuiCommand::PrCreated");
        }

        #[tokio::test]
        async fn poll_pr_marker_corrupt_json_warns_and_deletes() {
            let home = tempfile::tempdir().unwrap();
            let marker = marker_path(home.path());
            std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
            std::fs::write(&marker, b"not valid json {{{").unwrap();

            let mut app = make_app_with_home(home.path().to_path_buf());
            let cmds_before = app.pending_commands.len();
            app.poll_last_pr_created_marker().await;

            assert!(!marker.exists(), "corrupt marker must be deleted");
            assert_eq!(
                app.pending_commands.len(),
                cmds_before,
                "corrupt marker must NOT enqueue a command"
            );
            let warn = app.activity_log.entries().iter().any(|e| {
                matches!(e.level, LogLevel::Warn) && e.message.contains("last-pr-created")
            });
            assert!(
                warn,
                "corrupt marker must produce a Warn-level activity log entry"
            );
        }

        #[tokio::test]
        async fn poll_pr_marker_missing_is_noop() {
            let home = tempfile::tempdir().unwrap();
            let mut app = make_app_with_home(home.path().to_path_buf());
            let cmds_before = app.pending_commands.len();
            app.poll_last_pr_created_marker().await;
            assert_eq!(
                app.pending_commands.len(),
                cmds_before,
                "missing marker must be a silent no-op"
            );
        }

        #[tokio::test]
        async fn poll_pr_marker_refuses_symlink() {
            // A same-user attacker could plant a symlink at
            // ~/.maestro/last-pr-created. The reader must detect it (via
            // symlink_metadata) and unlink the symlink without following
            // it; the link target must stay untouched.
            let home = tempfile::tempdir().unwrap();
            let marker_dir = home.path().join(".maestro");
            std::fs::create_dir_all(&marker_dir).unwrap();
            let target = home.path().join("decoy.txt");
            std::fs::write(&target, "decoy contents").unwrap();
            let marker = marker_dir.join("last-pr-created");
            std::os::unix::fs::symlink(&target, &marker).unwrap();

            let mut app = make_app_with_home(home.path().to_path_buf());
            app.poll_last_pr_created_marker().await;

            assert!(!marker.exists(), "symlink at marker path must be unlinked");
            assert!(target.exists(), "symlink target must NOT be deleted");
            let warn = app
                .activity_log
                .entries()
                .iter()
                .any(|e| matches!(e.level, LogLevel::Warn) && e.message.contains("symlink"));
            assert!(
                warn,
                "symlink must produce a Warn entry mentioning 'symlink'"
            );
            let queued = app
                .pending_commands
                .iter()
                .any(|c| matches!(c, TuiCommand::PrCreated { .. }));
            assert!(!queued, "symlink must NOT enqueue a PrCreated command");
        }

        #[tokio::test]
        async fn poll_pr_marker_rejects_owner_with_path_traversal() {
            // Marker owner/repo from JSON must not be a vehicle for argv
            // injection or path traversal.
            let home = tempfile::tempdir().unwrap();
            let marker = marker_path(home.path());
            std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
            std::fs::write(
                &marker,
                r#"{"pr_number":42,"owner":"../evil","repo":"r","ts":"2026-04-30T00:00:00Z"}"#,
            )
            .unwrap();

            let mut app = make_app_with_home(home.path().to_path_buf());
            app.poll_last_pr_created_marker().await;

            assert!(!marker.exists(), "rejected marker must be deleted");
            let queued = app
                .pending_commands
                .iter()
                .any(|c| matches!(c, TuiCommand::PrCreated { .. }));
            assert!(!queued, "owner with traversal must NOT enqueue PrCreated");
        }

        #[test]
        fn app_new_emits_no_warn_when_state_has_no_pending_prs() {
            let app = build_app_with_seeded_state(MaestroState::default());

            let any_orphan_warn = app
                .activity_log
                .entries()
                .iter()
                .any(|e| e.session_label == "#orphan-prs");
            assert!(
                !any_orphan_warn,
                "no orphan warn should be emitted on a clean restart"
            );
        }

        #[test]
        fn sync_state_mirrors_pending_prs_to_persisted_state() {
            let mut app = crate::tui::make_test_app("pending-prs-sync");
            assert!(app.state.pending_prs.is_empty(), "precondition");
            app.pending_prs.push(make_pending_pr(42));
            app.pending_prs.push(make_pending_pr(99));

            app.sync_state();

            assert_eq!(
                app.state.pending_prs.len(),
                2,
                "sync_state must mirror in-memory pending_prs to persisted state"
            );
            let mirrored: Vec<u64> = app
                .state
                .pending_prs
                .iter()
                .map(|p| p.issue_number)
                .collect();
            assert!(mirrored.contains(&42));
            assert!(mirrored.contains(&99));
        }
    }
}
