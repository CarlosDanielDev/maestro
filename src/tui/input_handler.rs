use crate::tui::activity_log::LogLevel;
use crate::tui::app::{self, App};
use crate::tui::screen_dispatch::{dispatch_to_active_screen, handle_screen_action};
use crate::tui::screens;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

/// Result of handling a key event in the TUI loop.
pub(super) enum KeyAction {
    /// Event was handled; continue the loop.
    Consumed,
    /// The app should quit.
    Quit,
}

/// Top-level key event dispatcher. Handles overlays, mode-specific input,
/// global shortcuts, and screen dispatch in priority order.
pub(super) async fn handle_key(app: &mut App, key: KeyEvent) -> KeyAction {
    if handle_upgrade_keys(app, &key) {
        return KeyAction::Consumed;
    }

    if handle_help_overlay(app, &key) {
        return KeyAction::Consumed;
    }

    if let Some(action) = handle_mode_keys(app, &key).await {
        return action;
    }

    if handle_global_shortcuts(app, &key) {
        return KeyAction::Consumed;
    }

    if handle_quit_shortcut(app, &key) {
        return KeyAction::Quit;
    }

    let event = Event::Key(key);
    if let Some(action) = dispatch_to_active_screen(app, &event) {
        handle_screen_action(app, action);
        if !app.running {
            return KeyAction::Quit;
        }
        return KeyAction::Consumed;
    }

    if let Some(action) = handle_secondary_mode_keys(app, &key).await {
        return action;
    }

    handle_overview_keys(app, &key);
    KeyAction::Consumed
}

/// Handle upgrade banner input (u/y/n/Esc).
fn handle_upgrade_keys(app: &mut App, key: &KeyEvent) -> bool {
    match &app.upgrade_state {
        crate::updater::UpgradeState::Available(info) => {
            if key.code == KeyCode::Char('u') {
                let info_clone = info.clone();
                let tx = app.data_tx.clone();
                app.upgrade_state = crate::updater::UpgradeState::Downloading {
                    version: info_clone.version.clone(),
                };
                super::background_tasks::spawn_upgrade_download(tx, info_clone);
                return true;
            }
            if key.code == KeyCode::Esc {
                app.upgrade_state = crate::updater::UpgradeState::Hidden;
                return true;
            }
        }
        crate::updater::UpgradeState::ReadyToRestart { .. } => {
            if key.code == KeyCode::Char('y') {
                // Restart is handled in the main loop since it needs terminal access
                return false;
            }
            if key.code == KeyCode::Char('n') || key.code == KeyCode::Esc {
                app.upgrade_state = crate::updater::UpgradeState::Hidden;
                app.activity_log.push_simple(
                    "UPDATE".into(),
                    "Upgrade installed. Restart manually to use new version.".into(),
                    crate::tui::activity_log::LogLevel::Info,
                );
                return true;
            }
        }
        crate::updater::UpgradeState::Failed(_) => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                app.upgrade_state = crate::updater::UpgradeState::Hidden;
                return true;
            }
        }
        _ => {}
    }
    false
}

/// Handle help overlay input (scroll, search, close).
fn handle_help_overlay(app: &mut App, key: &KeyEvent) -> bool {
    let Some(ref mut help) = app.help_state else {
        return false;
    };
    if help.search_active {
        match key.code {
            KeyCode::Esc => help.clear_search(),
            KeyCode::Enter => help.search_active = false,
            KeyCode::Backspace => help.pop_char(),
            KeyCode::Char(c) => help.push_char(c),
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::F(1) => {
                app.help_state = None;
            }
            KeyCode::Char('j') | KeyCode::Down => help.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => help.scroll_up(),
            KeyCode::PageDown => help.page_down(),
            KeyCode::PageUp => help.page_up(),
            KeyCode::Char('/') => help.toggle_search(),
            _ => {}
        }
    }
    true
}

/// Handle mode-specific keys that preempt screen dispatch.
async fn handle_mode_keys(app: &mut App, key: &KeyEvent) -> Option<KeyAction> {
    match app.tui_mode {
        app::TuiMode::QueueExecution => Some(handle_queue_execution(app, key)),
        app::TuiMode::CompletionSummary => Some(handle_completion_summary(app, key)),
        app::TuiMode::ContinuousPause => Some(handle_continuous_pause(app, key)),
        _ => None,
    }
}

/// Handle keys after screen dispatch (modes without Screen impl).
async fn handle_secondary_mode_keys(app: &mut App, key: &KeyEvent) -> Option<KeyAction> {
    match app.tui_mode {
        app::TuiMode::SessionSummary => {
            handle_session_summary(app, key);
            Some(KeyAction::Consumed)
        }
        app::TuiMode::LogViewer(id) => Some(handle_log_viewer(app, key, id).await),
        app::TuiMode::ConfirmKill(id) => Some(handle_confirm_kill(app, key, id).await),
        app::TuiMode::SessionSwitcher => {
            handle_session_switcher(app, key);
            Some(KeyAction::Consumed)
        }
        _ => None,
    }
}

fn handle_queue_execution(app: &mut App, key: &KeyEvent) -> KeyAction {
    use crate::work::executor::{ExecutorPhase, FailureAction};
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            app.tui_mode = app::TuiMode::Overview;
        }
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.running = false;
            return KeyAction::Quit;
        }
        (KeyCode::Char('r'), _) => {
            if let Some(ref mut exec) = app.queue_executor
                && matches!(exec.phase(), ExecutorPhase::AwaitingDecision { .. })
            {
                exec.apply_decision(FailureAction::Retry);
                app.advance_queue_and_launch();
            }
        }
        (KeyCode::Char('s'), _) => {
            if let Some(ref mut exec) = app.queue_executor
                && matches!(exec.phase(), ExecutorPhase::AwaitingDecision { .. })
            {
                exec.apply_decision(FailureAction::Skip);
                if exec.is_finished() {
                    app.completion_summary = Some(app.build_completion_summary());
                    app.tui_mode = app::TuiMode::CompletionSummary;
                } else {
                    app.advance_queue_and_launch();
                }
            }
        }
        (KeyCode::Char('a'), _) => {
            if let Some(ref mut exec) = app.queue_executor
                && matches!(exec.phase(), ExecutorPhase::AwaitingDecision { .. })
            {
                exec.apply_decision(FailureAction::Abort);
                app.completion_summary = Some(app.build_completion_summary());
                app.tui_mode = app::TuiMode::CompletionSummary;
            }
        }
        _ => {}
    }
    KeyAction::Consumed
}

fn handle_completion_summary(app: &mut App, key: &KeyEvent) -> KeyAction {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) | (KeyCode::Esc, _) => {
            app.transition_to_dashboard();
        }
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.running = false;
            return KeyAction::Quit;
        }
        (KeyCode::Char('i'), _) => {
            app.completion_summary = None;
            app.completion_summary_dismissed = true;
            let mut screen = screens::IssueBrowserScreen::new(vec![]);
            screen.loading = true;
            app.issue_browser_screen = Some(screen);
            app.pending_commands.push(app::TuiCommand::FetchIssues);
            app.tui_mode = app::TuiMode::IssueBrowser;
        }
        (KeyCode::Char('r'), _) => {
            app.prompt_input_screen = Some(app::helpers::create_prompt_input_screen(
                &app.prompt_history,
            ));
            app.tui_mode = app::TuiMode::PromptInput;
        }
        (KeyCode::Char('l'), _) => {
            if let Some(ref summary) = app.completion_summary {
                if let Some(first) = summary.sessions.first() {
                    let sid = first.session_id;
                    app.log_viewer_scroll = 0;
                    app.completion_summary = None;
                    app.tui_mode = app::TuiMode::LogViewer(sid);
                } else {
                    app.tui_mode = app::TuiMode::Overview;
                }
            } else {
                app.tui_mode = app::TuiMode::Overview;
            }
        }
        (KeyCode::Char('f'), _) => {
            handle_completion_fix(app);
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
            if let Some(ref mut summary) = app.completion_summary
                && !summary.suggestions.is_empty()
            {
                summary.selected_suggestion = summary.selected_suggestion.saturating_sub(1);
            }
            app.panel_view.scroll_up();
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
            if let Some(ref mut summary) = app.completion_summary
                && !summary.suggestions.is_empty()
            {
                let max = summary.suggestions.len().saturating_sub(1);
                if summary.selected_suggestion < max {
                    summary.selected_suggestion += 1;
                }
            }
            app.panel_view.scroll_down();
        }
        _ => {}
    }
    if !matches!(
        key.code,
        KeyCode::Up | KeyCode::Down | KeyCode::Char('k' | 'j')
    ) {
        app.completion_summary = None;
    }
    KeyAction::Consumed
}

fn handle_completion_fix(app: &mut App) {
    let has_suggestions = app
        .completion_summary
        .as_ref()
        .map(|s| s.has_conflict_suggestions())
        .unwrap_or(false);
    if has_suggestions {
        let config = app.completion_summary.as_ref().and_then(|s| {
            s.suggestions
                .get(s.selected_suggestion)
                .map(|sg| screens::ConflictFixConfig {
                    pr_number: sg.pr_number,
                    issue_number: sg.issue_number,
                    branch: sg.branch.clone(),
                    conflicting_files: sg.conflicting_files.clone(),
                })
        });
        if let Some(config) = config {
            app.spawn_conflict_fix_session(&config);
            app.completion_summary = None;
            app.tui_mode = app::TuiMode::Overview;
        }
    } else {
        let needs_review: Vec<_> = app
            .completion_summary
            .as_ref()
            .into_iter()
            .flat_map(|s| &s.sessions)
            .filter(|s| s.status == crate::session::types::SessionStatus::NeedsReview)
            .cloned()
            .collect();
        if !needs_review.is_empty() {
            for sl in &needs_review {
                app.spawn_gate_fix_session(sl);
            }
            app.completion_summary = None;
            app.tui_mode = app::TuiMode::Overview;
        }
    }
}

fn handle_continuous_pause(app: &mut App, key: &KeyEvent) -> KeyAction {
    match (key.code, key.modifiers) {
        (KeyCode::Char('s'), _) => {
            if let Some(ref mut cont) = app.continuous_mode {
                let skipped = cont.current_failure().map(|f| f.issue_number);
                cont.on_skip();
                if let Some(num) = skipped {
                    app.activity_log.push_simple(
                        "CONTINUOUS".into(),
                        format!("Skipped #{}, advancing...", num),
                        LogLevel::Warn,
                    );
                }
            }
            app.tui_mode = app::TuiMode::Overview;
        }
        (KeyCode::Char('r'), _) => {
            if let Some(ref mut cont) = app.continuous_mode
                && let Some(issue_number) = cont.on_retry()
            {
                if let Some(ref mut assigner) = app.work_assigner {
                    assigner.mark_pending_undo_cascade(issue_number);
                }
                app.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!("Retrying #{}...", issue_number),
                    LogLevel::Info,
                );
            }
            app.tui_mode = app::TuiMode::Overview;
        }
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.continuous_mode = None;
            app.running = false;
            return KeyAction::Quit;
        }
        _ => {}
    }
    KeyAction::Consumed
}

fn handle_session_summary(app: &mut App, key: &KeyEvent) {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            app.session_summary_state = None;
            app.tui_mode = app::TuiMode::Overview;
        }
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.running = false;
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
            if let Some(state) = app.session_summary_state.as_mut() {
                if state.selected_index > 0 {
                    state.selected_index -= 1;
                }
                state.scroll_up();
            }
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
            let max_idx = app
                .completion_summary
                .as_ref()
                .map(|s| s.sessions.len().saturating_sub(1))
                .unwrap_or(0);
            if let Some(state) = app.session_summary_state.as_mut() {
                if state.selected_index < max_idx {
                    state.selected_index += 1;
                }
                state.scroll_down();
            }
        }
        (KeyCode::Enter, _) => {
            let session_id = app.completion_summary.as_ref().and_then(|s| {
                let idx = app
                    .session_summary_state
                    .as_ref()
                    .map(|st| st.selected_index)
                    .unwrap_or(0);
                s.sessions.get(idx).map(|sl| sl.session_id)
            });
            if let (Some(id), Some(state)) = (session_id, app.session_summary_state.as_mut()) {
                state.toggle_expand(id);
            }
        }
        _ => {}
    }
}

async fn handle_log_viewer(app: &mut App, key: &KeyEvent, id: uuid::Uuid) -> KeyAction {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            app.tui_mode = app::TuiMode::Detail(id);
        }
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.running = false;
            return KeyAction::Quit;
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
            app.log_viewer_scroll = app.log_viewer_scroll.saturating_sub(1);
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
            app.log_viewer_scroll = app.log_viewer_scroll.saturating_add(1);
        }
        (KeyCode::Char('G'), _) => {
            app.log_viewer_scroll = u16::MAX;
        }
        (KeyCode::Char('g'), _) => {
            app.log_viewer_scroll = 0;
        }
        _ => {}
    }
    KeyAction::Consumed
}

async fn handle_confirm_kill(app: &mut App, key: &KeyEvent, session_id: uuid::Uuid) -> KeyAction {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            app.kill_selected_session(session_id).await;
            app.tui_mode = app::TuiMode::Overview;
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.tui_mode = app::TuiMode::Overview;
        }
        _ => {}
    }
    KeyAction::Consumed
}

fn handle_session_switcher(app: &mut App, key: &KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.session_switcher = None;
            app.tui_mode = app::TuiMode::Overview;
        }
        KeyCode::Up => {
            if let Some(sw) = &mut app.session_switcher {
                sw.move_up();
            }
        }
        KeyCode::Down => {
            if let Some(sw) = &mut app.session_switcher {
                let count = {
                    let sessions = app.pool.all_sessions();
                    let refs: Vec<&crate::session::types::Session> = sessions;
                    sw.filtered_sessions(&refs).len()
                };
                sw.move_down(count);
            }
        }
        KeyCode::Enter => {
            let selected_id = app.session_switcher.as_ref().and_then(|sw| {
                let sessions = app.pool.all_sessions();
                sw.selected_session(&sessions).map(|s| s.id)
            });
            if let Some(id) = selected_id {
                app.session_switcher = None;
                app.tui_mode = app::TuiMode::Detail(id);
            }
        }
        _ => {}
    }
}

/// Handle global shortcuts that apply before screen dispatch (help, F-keys, Ctrl-X).
fn handle_global_shortcuts(app: &mut App, key: &KeyEvent) -> bool {
    // Help overlay toggle
    let is_text_input_mode = matches!(
        app.tui_mode,
        app::TuiMode::PromptInput | app::TuiMode::SessionSwitcher
    );
    if key.code == KeyCode::F(1) || (key.code == KeyCode::Char('?') && !is_text_input_mode) {
        app.help_state = Some(crate::tui::help::HelpOverlayState::new());
        return true;
    }

    // F-key aliases
    if let KeyCode::F(n) = key.code {
        match n {
            2 => {
                let summary = app.build_completion_summary();
                app.completion_summary = Some(summary);
                app.session_summary_state =
                    Some(crate::tui::app::types::SessionSummaryState::default());
                app.tui_mode = app::TuiMode::SessionSummary;
                return true;
            }
            3 => {
                let selected = app.panel_view.selected_index();
                if let Some(id) = app.pool.session_id_at_index(selected) {
                    app.tui_mode = app::TuiMode::Fullscreen(id);
                }
                return true;
            }
            4 => {
                app.tui_mode = app::TuiMode::CostDashboard;
                return true;
            }
            5 => {
                app.tui_mode = app::TuiMode::TokenDashboard;
                return true;
            }
            6 => {
                app.tui_mode = match app.tui_mode {
                    app::TuiMode::Overview => app::TuiMode::DependencyGraph,
                    app::TuiMode::DependencyGraph => app::TuiMode::CostDashboard,
                    app::TuiMode::CostDashboard => app::TuiMode::TokenDashboard,
                    app::TuiMode::TokenDashboard => app::TuiMode::Overview,
                    _ => app::TuiMode::Overview,
                };
                return true;
            }
            #[cfg(unix)]
            9 => {
                app.pause_all();
                return true;
            }
            10 => {
                let selected = app.panel_view.selected_index();
                if let Some(id) = app.pool.session_id_at_index(selected)
                    && let Some(session) = app.pool.get_session(id)
                    && !session.status.is_terminal()
                {
                    app.tui_mode = app::TuiMode::ConfirmKill(id);
                }
                return true;
            }
            _ => {}
        }
    }

    false
}

/// Handle Ctrl-X / Ctrl-C quit.
fn handle_quit_shortcut(app: &mut App, key: &KeyEvent) -> bool {
    if key.code == KeyCode::Char('x') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.running = false;
        return true;
    }
    false
}

/// Handle overview/default mode keys (navigation, session management).
fn handle_overview_keys(app: &mut App, key: &KeyEvent) {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.running = false;
        }
        #[cfg(unix)]
        (KeyCode::Char('p'), _) => {
            app.pause_all();
        }
        #[cfg(unix)]
        (KeyCode::Char('r'), _) => {
            app.resume_all();
        }
        (KeyCode::Char('k'), _) => {
            let selected = app.panel_view.selected_index();
            if let Some(id) = app.pool.session_id_at_index(selected)
                && let Some(session) = app.pool.get_session(id)
                && !session.status.is_terminal()
            {
                app.tui_mode = app::TuiMode::ConfirmKill(id);
            }
        }
        (KeyCode::Char('K'), _) => {
            // kill_all is async but we're in a sync context here;
            // it will be handled by the caller if needed
        }
        (KeyCode::Char('f'), _) => {
            let selected = app.panel_view.selected_index();
            if let Some(id) = app.pool.session_id_at_index(selected) {
                app.tui_mode = app::TuiMode::Fullscreen(id);
            }
        }
        (KeyCode::Char('$'), _) => {
            app.tui_mode = app::TuiMode::CostDashboard;
        }
        (KeyCode::Char('t'), _) => {
            app.tui_mode = app::TuiMode::TokenDashboard;
        }
        (KeyCode::Char('S'), _) => {
            let summary = app.build_completion_summary();
            app.completion_summary = Some(summary);
            app.session_summary_state =
                Some(crate::tui::app::types::SessionSummaryState::default());
            app.tui_mode = app::TuiMode::SessionSummary;
        }
        (KeyCode::Tab, _) => {
            app.tui_mode = match app.tui_mode {
                app::TuiMode::Overview => app::TuiMode::DependencyGraph,
                app::TuiMode::DependencyGraph => app::TuiMode::CostDashboard,
                app::TuiMode::CostDashboard => app::TuiMode::TokenDashboard,
                app::TuiMode::TokenDashboard => app::TuiMode::Overview,
                _ => app::TuiMode::Overview,
            };
        }
        (KeyCode::Esc, _) => {
            if app.home_screen.is_some() && app.pool.total_count() == 0 {
                app.tui_mode = app::TuiMode::Dashboard;
            } else {
                app.tui_mode = app::TuiMode::Overview;
            }
        }
        (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => {
            let selected = app.panel_view.selected_index();
            if let Some(id) = app.pool.session_id_at_index(selected)
                && let Some(session) = app.pool.get_session(id)
            {
                if session.status.is_terminal() {
                    app.toggle_session_summary(id);
                } else {
                    app.tui_mode = app::TuiMode::Detail(id);
                }
            }
        }
        (KeyCode::Char(c), _) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as usize) - ('1' as usize);
            if let Some(id) = app.pool.session_id_at_index(idx) {
                app.tui_mode = app::TuiMode::Detail(id);
            }
        }
        (KeyCode::Char('w'), _) => {
            app.session_switcher = Some(crate::tui::session_switcher::SessionSwitcher::default());
            app.tui_mode = app::TuiMode::SessionSwitcher;
        }
        (KeyCode::Char('d'), _) => {
            let selected = app.panel_view.selected_index();
            if let Some(id) = app.pool.session_id_at_index(selected)
                && let Some(session) = app.pool.get_session(id)
                && session.status.is_terminal()
            {
                app.dismiss_session(id);
            } else {
                app.notifications.dismiss_latest();
            }
        }
        (KeyCode::Char('D'), _) => {
            app.dismiss_all_completed();
        }
        (KeyCode::Char('l'), _) => match app.tui_mode {
            app::TuiMode::Detail(id) => {
                app.log_viewer_scroll = 0;
                app.tui_mode = app::TuiMode::LogViewer(id);
            }
            app::TuiMode::Overview => {
                let selected = app.panel_view.selected_index();
                if let Some(id) = app.pool.session_id_at_index(selected) {
                    app.log_viewer_scroll = 0;
                    app.tui_mode = app::TuiMode::LogViewer(id);
                }
            }
            _ => {}
        },
        (KeyCode::Up, KeyModifiers::SHIFT) => {
            app.panel_view.scroll_up();
        }
        (KeyCode::Down, KeyModifiers::SHIFT) => {
            app.panel_view.scroll_down();
        }
        (KeyCode::Up, _)
        | (KeyCode::Down, _)
        | (KeyCode::Left, _)
        | (KeyCode::Right, _)
        | (KeyCode::Char('['), _)
        | (KeyCode::Char(']'), _) => {
            handle_grid_navigation(app, key);
        }
        _ => {}
    }
}

fn handle_grid_navigation(app: &mut App, key: &KeyEvent) {
    // Grid navigation needs terminal size — use a reasonable default
    let (width, height) = crossterm::terminal::size().unwrap_or((120, 40));
    let layout = crate::tui::panels::GridLayout::calculate(app.pool.total_count(), width, height);
    let total_sessions = app.pool.total_count();
    match key.code {
        KeyCode::Up => app.panel_view.grid_state.move_up(),
        KeyCode::Down => app.panel_view.grid_state.move_down(&layout, total_sessions),
        KeyCode::Left => app.panel_view.grid_state.move_left(),
        KeyCode::Right => app
            .panel_view
            .grid_state
            .move_right(&layout, total_sessions),
        KeyCode::Char('[') => app.panel_view.grid_state.prev_page(&layout),
        KeyCode::Char(']') => app.panel_view.grid_state.next_page(&layout),
        _ => {}
    }
    app.panel_view.selected = Some(app.panel_view.grid_state.selected_index(&layout));
    if !matches!(key.code, KeyCode::Char('[') | KeyCode::Char(']')) {
        app.panel_view.scroll_offset = 0;
    }
}
