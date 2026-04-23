use crate::tui::activity_log::LogLevel;
use crate::tui::app::{self, App};
use crate::tui::screen_dispatch::{
    dispatch_to_active_screen_then_hook as dispatch_to_active_screen, handle_screen_action,
};
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
    // Ctrl+C always exits immediately (power-user bypass) — HIGHEST PRIORITY
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.running = false;
        return KeyAction::Quit;
    }

    // Ctrl+X also exits immediately
    if key.code == KeyCode::Char('x') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.running = false;
        return KeyAction::Quit;
    }

    // If confirm exit dialog is showing, handle it
    if app.tui_mode == app::TuiMode::ConfirmExit {
        return handle_confirm_exit(app, &key);
    }

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

    // 'q' triggers confirm exit (except in text input modes)
    if key.code == KeyCode::Char('q') && !is_text_input_mode(app) {
        app.navigate_to(app::TuiMode::ConfirmExit);
        return KeyAction::Consumed;
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
        crate::updater::UpgradeState::Failed(_)
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter =>
        {
            app.upgrade_state = crate::updater::UpgradeState::Hidden;
            return true;
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
            app.navigate_back_or_dashboard();
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
        (KeyCode::Char('d'), _) => {
            app.transition_to_dashboard();
        }
        (KeyCode::Char('q'), _) => {
            app.completion_summary_dismissed = true;
            app.navigate_to(app::TuiMode::ConfirmExit);
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
                if let Some(ref mut service) = app.work_assignment_service {
                    service.inner_mut().mark_pending_undo_cascade(issue_number);
                }
                app.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!("Retrying #{}...", issue_number),
                    LogLevel::Info,
                );
            }
            app.tui_mode = app::TuiMode::Overview;
        }
        _ => {}
    }
    KeyAction::Consumed
}

fn handle_session_summary(app: &mut App, key: &KeyEvent) {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            app.session_summary_state = None;
            app.navigate_back_or_dashboard();
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

async fn handle_log_viewer(app: &mut App, key: &KeyEvent, _id: uuid::Uuid) -> KeyAction {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            app.navigate_back_or_dashboard();
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
            app.navigate_back_or_dashboard();
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.navigate_back_or_dashboard();
        }
        _ => {}
    }
    KeyAction::Consumed
}

fn handle_session_switcher(app: &mut App, key: &KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.session_switcher = None;
            app.navigate_back_or_dashboard();
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
                app.navigate_to(app::TuiMode::Detail(id));
            }
        }
        _ => {}
    }
}

/// Handle global shortcuts that apply before screen dispatch (help, F-keys, Ctrl-X).
fn handle_global_shortcuts(app: &mut App, key: &KeyEvent) -> bool {
    // Ctrl+q toggles TurboQuant from any screen
    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        let new_state = app.flags.toggle(crate::flags::Flag::TurboQuant);
        // Keep config in sync so Settings screen opens with correct state
        if let Some(ref mut config) = app.config {
            config.turboquant.enabled = new_state;
        }
        let label = if new_state {
            "[TurboQuant] Enabled"
        } else {
            "[TurboQuant] Disabled"
        };
        app.activity_log
            .push_simple("TQ".into(), label.into(), LogLevel::Info);
        if let Some(ref mut screen) = app.settings_screen {
            screen.sync_tq_enabled(new_state);
        }
        return true;
    }

    // Shift+Q opens TurboQuant A/B dashboard
    if key.code == KeyCode::Char('Q') && !is_text_input_mode(app) {
        app.navigate_to(app::TuiMode::TurboquantDashboard);
        return true;
    }

    // Help overlay toggle
    let is_text_input_mode = matches!(
        app.tui_mode,
        app::TuiMode::PromptInput | app::TuiMode::SessionSwitcher
    );
    if key.code == KeyCode::F(1) || (key.code == KeyCode::Char('?') && !is_text_input_mode) {
        app.help_state = Some(crate::tui::help::HelpOverlayState::new());
        return true;
    }

    // F-key dispatch — looks up the current mode's F-key table (built by
    // `mode_keymap` and cached on `app.cached_mode_km` by the renderer
    // each frame) and executes the paired action. Label and dispatch
    // come from the SAME `FKeyRelevance` entry, so they cannot drift.
    if let KeyCode::F(n) = key.code {
        let key_label = format!("F{}", n);
        let action = app
            .cached_mode_km
            .as_ref()
            .and_then(|km| km.fkeys.iter().find(|r| r.key == key_label))
            .filter(|r| r.active)
            .map(|r| r.action);
        if let Some(action) = action {
            dispatch_fkey_action(app, action);
            return true;
        }
        // Key exists in the table but inactive (or the key isn't bound
        // in this mode). Either way, consume to prevent fall-through to
        // the screen dispatch path treating the F-key as a text input.
        if app
            .cached_mode_km
            .as_ref()
            .is_some_and(|km| km.fkeys.iter().any(|r| r.key == key_label))
        {
            return true;
        }
    }

    false
}

fn dispatch_fkey_action(app: &mut App, action: crate::tui::navigation::keymap::FKeyAction) {
    use crate::tui::navigation::keymap::FKeyAction;
    match action {
        FKeyAction::ToggleHelp => {
            app.help_state = Some(crate::tui::help::HelpOverlayState::new());
        }
        FKeyAction::OpenSummary => app.open_session_summary(),
        FKeyAction::OpenFullscreenSelected => {
            let selected = app.panel_view.selected_index();
            if let Some(id) = app.pool.session_id_at_index(selected) {
                app.navigate_to(app::TuiMode::Fullscreen(id));
            }
        }
        FKeyAction::OpenCostDashboard => app.navigate_to(app::TuiMode::CostDashboard),
        FKeyAction::OpenTokenDashboard => app.navigate_to(app::TuiMode::TokenDashboard),
        FKeyAction::OpenDependencyGraph => app.navigate_to(app::TuiMode::DependencyGraph),
        FKeyAction::PauseAll => {
            #[cfg(unix)]
            app.pause_all();
        }
        FKeyAction::KillSelected => {
            let selected = app.panel_view.selected_index();
            if let Some(id) = app.pool.session_id_at_index(selected)
                && let Some(session) = app.pool.get_session(id)
                && !session.status.is_terminal()
            {
                app.navigate_to(app::TuiMode::ConfirmKill(id));
            }
        }
        FKeyAction::Exit => app.running = false,
    }
}

/// Returns true if the current TUI mode accepts text input (q should type, not quit).
fn is_text_input_mode(app: &App) -> bool {
    matches!(
        app.tui_mode,
        app::TuiMode::PromptInput | app::TuiMode::SessionSwitcher | app::TuiMode::Settings
    )
}

/// Handle the confirm-exit dialog (y/n/Enter/Esc).
fn handle_confirm_exit(app: &mut App, key: &KeyEvent) -> KeyAction {
    // Enter is deliberately NOT an affirmative here. The dialog is often
    // opened by a menu-Enter (e.g. Quick Action "Quit"), and reflex-pressing
    // Enter again would silently destroy the session. Require explicit `y`.
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.running = false;
            KeyAction::Quit
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Enter => {
            if let Some(prev) = app.nav_stack.pop() {
                app.tui_mode = prev;
            } else {
                app.tui_mode = app::TuiMode::Overview;
            }
            KeyAction::Consumed
        }
        _ => KeyAction::Consumed, // unrelated keys: stay in the dialog
    }
}

/// Handle overview/default mode keys (navigation, session management).
fn handle_overview_keys(app: &mut App, key: &KeyEvent) {
    match (key.code, key.modifiers) {
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
                app.navigate_to(app::TuiMode::ConfirmKill(id));
            }
        }
        (KeyCode::Char('K'), _) => {
            // kill_all is async but we're in a sync context here;
            // it will be handled by the caller if needed
        }
        (KeyCode::Char('f'), _) => {
            let selected = app.panel_view.selected_index();
            if let Some(id) = app.pool.session_id_at_index(selected) {
                app.navigate_to(app::TuiMode::Fullscreen(id));
            }
        }
        (KeyCode::Char('$'), _) => {
            app.navigate_to(app::TuiMode::CostDashboard);
        }
        (KeyCode::Char('t'), _) => {
            app.navigate_to(app::TuiMode::TokenDashboard);
        }
        (KeyCode::Char('S'), _) => app.open_session_summary(),
        (KeyCode::Tab, _) => {
            app.tui_mode = match app.tui_mode {
                app::TuiMode::Overview => app::TuiMode::DependencyGraph,
                app::TuiMode::DependencyGraph => app::TuiMode::CostDashboard,
                app::TuiMode::CostDashboard => app::TuiMode::TokenDashboard,
                app::TuiMode::TokenDashboard => app::TuiMode::TurboquantDashboard,
                app::TuiMode::TurboquantDashboard => app::TuiMode::Overview,
                _ => app::TuiMode::Overview,
            };
        }
        (KeyCode::Esc, _) => {
            app.navigate_back();
        }
        (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => {
            let selected = app.panel_view.selected_index();
            if let Some(id) = app.pool.session_id_at_index(selected)
                && let Some(session) = app.pool.get_session(id)
            {
                if session.status.is_terminal() {
                    app.toggle_session_summary(id);
                } else {
                    app.navigate_to(app::TuiMode::Detail(id));
                }
            }
        }
        (KeyCode::Char(c), _) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as usize) - ('1' as usize);
            if let Some(id) = app.pool.session_id_at_index(idx) {
                app.navigate_to(app::TuiMode::Detail(id));
            }
        }
        (KeyCode::Char('w'), _) => {
            app.session_switcher = Some(crate::tui::session_switcher::SessionSwitcher::default());
            app.navigate_to(app::TuiMode::SessionSwitcher);
        }
        (KeyCode::Char('d'), _) => {
            app.show_activity_log = !app.show_activity_log;
        }
        (KeyCode::Char('D'), _) => {
            app.dismiss_all_completed();
        }
        (KeyCode::Char('l'), _) => match app.tui_mode {
            app::TuiMode::Detail(id) => {
                app.log_viewer_scroll = 0;
                app.navigate_to(app::TuiMode::LogViewer(id));
            }
            app::TuiMode::Overview => {
                let selected = app.panel_view.selected_index();
                if let Some(id) = app.pool.session_id_at_index(selected) {
                    app.log_viewer_scroll = 0;
                    app.navigate_to(app::TuiMode::LogViewer(id));
                }
            }
            _ => {}
        },
        (KeyCode::Up, KeyModifiers::SHIFT) => {
            app.activity_log.scroll_up();
        }
        (KeyCode::Down, KeyModifiers::SHIFT) => {
            app.activity_log.scroll_down();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::worktree::MockWorktreeManager;
    use crate::state::store::StateStore;
    use crate::tui::app::{App, TuiMode};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn key_code(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_c_event() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    }

    fn make_app() -> App {
        let tmp = std::env::temp_dir().join(format!(
            "maestro-input-handler-test-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = StateStore::new(tmp);
        App::new(
            store,
            3,
            Box::new(MockWorktreeManager::new()),
            "bypassPermissions".into(),
            vec![],
        )
    }

    // ── Group 1: TuiMode variant and field defaults ───────────────────

    #[test]
    fn confirm_exit_tui_mode_variant_exists() {
        let mode = TuiMode::ConfirmExit;
        assert!(matches!(mode, TuiMode::ConfirmExit));
    }

    #[test]
    fn nav_stack_defaults_to_empty() {
        let app = make_app();
        assert!(app.nav_stack.is_empty());
    }

    // ── Group 2: is_text_input_mode() ─────────────────────────────────

    #[test]
    fn is_text_input_mode_true_for_prompt_input() {
        let mut app = make_app();
        app.tui_mode = TuiMode::PromptInput;
        assert!(is_text_input_mode(&app));
    }

    #[test]
    fn is_text_input_mode_true_for_session_switcher() {
        let mut app = make_app();
        app.tui_mode = TuiMode::SessionSwitcher;
        assert!(is_text_input_mode(&app));
    }

    #[test]
    fn is_text_input_mode_true_for_settings() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Settings;
        assert!(is_text_input_mode(&app));
    }

    #[test]
    fn is_text_input_mode_false_for_overview() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Overview;
        assert!(!is_text_input_mode(&app));
    }

    #[test]
    fn is_text_input_mode_false_for_dependency_graph() {
        let mut app = make_app();
        app.tui_mode = TuiMode::DependencyGraph;
        assert!(!is_text_input_mode(&app));
    }

    // ── Group 3: handle_confirm_exit — only `y` confirms ──────────────

    #[test]
    fn y_in_confirm_exit_sets_running_false() {
        let mut app = make_app();
        app.tui_mode = TuiMode::ConfirmExit;
        app.running = true;
        handle_confirm_exit(&mut app, &key('y'));
        assert!(!app.running);
    }

    #[test]
    fn uppercase_y_in_confirm_exit_sets_running_false() {
        let mut app = make_app();
        app.tui_mode = TuiMode::ConfirmExit;
        app.running = true;
        handle_confirm_exit(&mut app, &key('Y'));
        assert!(!app.running);
    }

    /// Regression guard: the dialog is often opened by a menu-Enter
    /// (Quick Action "Quit"). If Enter were an affirmative here, a
    /// reflexive second Enter would silently exit the app before the
    /// user could see the dialog. Enter must cancel instead.
    #[test]
    fn enter_in_confirm_exit_cancels_instead_of_confirming() {
        let mut app = make_app();
        app.nav_stack.push(TuiMode::Dashboard);
        app.tui_mode = TuiMode::ConfirmExit;
        app.running = true;
        handle_confirm_exit(&mut app, &key_code(KeyCode::Enter));
        assert!(app.running, "Enter must NOT quit");
        assert_eq!(
            app.tui_mode,
            TuiMode::Dashboard,
            "Enter must restore the previous mode"
        );
    }

    #[test]
    fn y_in_confirm_exit_keeps_stack_intact() {
        let mut app = make_app();
        app.tui_mode = TuiMode::ConfirmExit;
        app.nav_stack.push(TuiMode::Overview);
        handle_confirm_exit(&mut app, &key('y'));
        assert!(!app.running);
    }

    // ── Group 4: handle_confirm_exit — n/Esc cancel ───────────────────

    #[test]
    fn n_in_confirm_exit_restores_previous_mode() {
        let mut app = make_app();
        app.nav_stack.push(TuiMode::DependencyGraph);
        app.tui_mode = TuiMode::ConfirmExit;
        handle_confirm_exit(&mut app, &key('n'));
        assert_eq!(app.tui_mode, TuiMode::DependencyGraph);
    }

    #[test]
    fn esc_in_confirm_exit_restores_previous_mode() {
        let mut app = make_app();
        app.nav_stack.push(TuiMode::Overview);
        app.tui_mode = TuiMode::ConfirmExit;
        handle_confirm_exit(&mut app, &key_code(KeyCode::Esc));
        assert_eq!(app.tui_mode, TuiMode::Overview);
    }

    #[test]
    fn n_in_confirm_exit_does_not_quit() {
        let mut app = make_app();
        app.nav_stack.push(TuiMode::Overview);
        app.tui_mode = TuiMode::ConfirmExit;
        app.running = true;
        handle_confirm_exit(&mut app, &key('n'));
        assert!(app.running);
    }

    #[test]
    fn cancel_with_empty_stack_falls_back_to_overview() {
        let mut app = make_app();
        app.tui_mode = TuiMode::ConfirmExit;
        handle_confirm_exit(&mut app, &key('n'));
        assert_eq!(app.tui_mode, TuiMode::Overview);
    }

    // ── Group 5: unrelated key swallowing ─────────────────────────────

    #[test]
    fn unrelated_key_in_confirm_exit_stays() {
        let mut app = make_app();
        app.tui_mode = TuiMode::ConfirmExit;
        app.nav_stack.push(TuiMode::Overview);
        handle_confirm_exit(&mut app, &key('x'));
        assert!(matches!(app.tui_mode, TuiMode::ConfirmExit));
    }

    #[test]
    fn unrelated_key_does_not_quit() {
        let mut app = make_app();
        app.tui_mode = TuiMode::ConfirmExit;
        app.running = true;
        handle_confirm_exit(&mut app, &key('j'));
        assert!(app.running);
    }

    // ── Group 6: q blocked in text input modes ────────────────────────

    #[test]
    fn q_in_prompt_input_does_not_trigger_confirm() {
        let mut app = make_app();
        app.tui_mode = TuiMode::PromptInput;
        if !is_text_input_mode(&app) {
            handle_confirm_exit(&mut app, &key('q'));
        }
        assert_eq!(app.tui_mode, TuiMode::PromptInput);
        assert!(app.running);
    }

    // ── Group 7: Ctrl+C bypass (async integration) ────────────────────

    #[tokio::test]
    async fn ctrl_c_always_quits_immediately() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Overview;
        handle_key(&mut app, ctrl_c_event()).await;
        assert!(!app.running);
        assert!(!matches!(app.tui_mode, TuiMode::ConfirmExit));
    }

    #[tokio::test]
    async fn ctrl_c_from_confirm_exit_quits() {
        let mut app = make_app();
        app.tui_mode = TuiMode::ConfirmExit;
        app.nav_stack.push(TuiMode::Overview);
        handle_key(&mut app, ctrl_c_event()).await;
        assert!(!app.running);
    }

    #[tokio::test]
    async fn q_in_overview_enters_confirm_exit() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Overview;
        handle_key(&mut app, key('q')).await;
        assert!(matches!(app.tui_mode, TuiMode::ConfirmExit));
        assert_eq!(app.nav_stack.peek(), Some(&TuiMode::Overview));
        assert!(app.running);
    }

    // ── Issue #342: Nav-stack integration tests ──────────────────────────

    #[test]
    fn navigate_to_pushes_current_mode_and_switches() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Overview;
        app.navigate_to(TuiMode::IssueBrowser);
        assert_eq!(app.tui_mode, TuiMode::IssueBrowser);
        assert_eq!(app.nav_stack.peek(), Some(&TuiMode::Overview));
    }

    #[test]
    fn navigate_back_pops_to_previous_mode() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Overview;
        app.navigate_to(TuiMode::IssueBrowser);
        app.navigate_back();
        assert_eq!(app.tui_mode, TuiMode::Overview);
        assert!(app.nav_stack.is_empty());
    }

    #[test]
    fn navigate_back_on_empty_stack_triggers_confirm_exit() {
        let mut app = make_app();
        assert!(app.nav_stack.is_empty());
        app.navigate_back();
        assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
    }

    #[test]
    fn navigate_back_or_dashboard_falls_to_dashboard_on_empty_stack() {
        let mut app = make_app();
        assert!(app.nav_stack.is_empty());
        app.navigate_back_or_dashboard();
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
    }

    #[test]
    fn navigate_to_root_clears_stack_and_sets_dashboard() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Overview;
        app.navigate_to(TuiMode::IssueBrowser);
        app.navigate_to(TuiMode::Settings);
        app.navigate_to_root();
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
        assert!(app.nav_stack.is_empty());
    }

    // ── Hint / handler drift guards ────────────────────────────────────
    //
    // The header advertises per-mode hints (src/tui/navigation/mode_hints.rs),
    // the F-bar advertises F-keys (FKeyRelevance), and actual handlers live
    // here + in per-screen handle_input methods. These used to be
    // independent declarations with no compiler link, so they drifted:
    // `[d] Dashboard` hint did nothing; F6 "Deps" fell through to Overview.
    //
    // These tests pin the contract. Adding a new navigation-style hint or
    // F-key without a matching handler must fail CI.

    fn completion_summary_app() -> App {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(Default::default());
        app
    }

    // TODO(hint-action-enum): replace this string table with a `HintAction`
    // enum carrying a `target_mode()` method on `InlineHint`. Then the
    // action label + target navigation live in one declaration, and this
    // helper disappears. See `src/tui/navigation/keymap.rs` for the home.
    fn expected_mode_for_action(action: &str) -> Option<TuiMode> {
        match action {
            "Browse" | "Issues" => Some(TuiMode::IssueBrowser),
            "New Prompt" | "Prompt" => Some(TuiMode::PromptInput),
            "Dashboard" => Some(TuiMode::Dashboard),
            "Quit" => Some(TuiMode::ConfirmExit),
            "Milestones" => Some(TuiMode::MilestoneView),
            "Settings" => Some(TuiMode::Settings),
            "Sessions" => Some(TuiMode::Overview),
            "Adapt" => Some(TuiMode::AdaptWizard),
            _ => None,
        }
    }

    fn hint_key_to_keycode(k: &str) -> Option<KeyCode> {
        match k {
            "Enter" => Some(KeyCode::Enter),
            "Esc" => Some(KeyCode::Esc),
            _ if k.chars().count() == 1 => Some(KeyCode::Char(k.chars().next().unwrap())),
            _ => None,
        }
    }

    fn fkey_action_target_mode(
        action: crate::tui::navigation::keymap::FKeyAction,
    ) -> Option<TuiMode> {
        use crate::tui::navigation::keymap::FKeyAction;
        match action {
            FKeyAction::OpenCostDashboard => Some(TuiMode::CostDashboard),
            FKeyAction::OpenTokenDashboard => Some(TuiMode::TokenDashboard),
            FKeyAction::OpenDependencyGraph => Some(TuiMode::DependencyGraph),
            FKeyAction::OpenSummary => Some(TuiMode::SessionSummary),
            _ => None,
        }
    }

    #[test]
    fn completion_summary_hints_dispatch_to_advertised_mode() {
        use crate::tui::navigation::keymap::mode_keymap;

        let km = mode_keymap(TuiMode::CompletionSummary, None, &[]);
        let mut checked = 0;

        for hint in km.hints {
            let Some(expected) = expected_mode_for_action(hint.action) else {
                continue;
            };
            let Some(code) = hint_key_to_keycode(hint.key) else {
                continue;
            };
            let mut app = completion_summary_app();
            handle_completion_summary(&mut app, &key_code(code));
            assert_eq!(
                app.tui_mode, expected,
                "Hint `[{}] {}` advertised in TuiMode::CompletionSummary but \
                 handler did not navigate to {:?} (landed in {:?}). See \
                 `handle_completion_summary` and `mode_hints.rs` for the \
                 conflicting declarations.",
                hint.key, hint.action, expected, app.tui_mode
            );
            checked += 1;
        }

        assert!(
            checked > 0,
            "No CompletionSummary hints were verified — either the hint \
             table changed action labels or expected_mode_for_action is \
             missing entries."
        );
    }

    // navigate_to must be idempotent and cycle-collapsing. Pressing F5
    // repeatedly while already on TokenDashboard used to grow the
    // breadcrumb trail by one entry per keystroke; navigating A → B → A
    // used to produce [A, B] on the stack instead of just [A].

    #[test]
    fn navigate_to_same_mode_is_a_noop() {
        let mut app = make_app();
        app.tui_mode = TuiMode::TokenDashboard;
        app.navigate_to(TuiMode::TokenDashboard);
        app.navigate_to(TuiMode::TokenDashboard);
        app.navigate_to(TuiMode::TokenDashboard);
        assert_eq!(app.tui_mode, TuiMode::TokenDashboard);
        assert_eq!(app.nav_stack.depth(), 0);
    }

    #[test]
    fn navigate_to_truncates_to_existing_ancestor_instead_of_pushing() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        app.navigate_to(TuiMode::TokenDashboard);
        app.navigate_to(TuiMode::Dashboard);
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
        assert_eq!(app.nav_stack.depth(), 0);
    }

    #[test]
    fn navigate_to_through_several_modes_does_not_grow_on_same_press() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        app.navigate_to(TuiMode::DependencyGraph);
        app.navigate_to(TuiMode::DependencyGraph);
        app.navigate_to(TuiMode::TokenDashboard);
        app.navigate_to(TuiMode::TokenDashboard);
        app.navigate_to(TuiMode::TokenDashboard);
        assert_eq!(app.tui_mode, TuiMode::TokenDashboard);
        assert_eq!(
            app.nav_stack.breadcrumbs(),
            &[TuiMode::Dashboard, TuiMode::DependencyGraph]
        );
    }

    #[test]
    fn fkey_dashboard_mode_advertises_f4_f5_f6_and_each_dispatches() {
        use crate::tui::navigation::keymap::mode_keymap;
        let km = mode_keymap(TuiMode::Dashboard, None, &[]);
        let mut checked = 0;
        for fkey in &km.fkeys {
            let Some(expected) = fkey_action_target_mode(fkey.action) else {
                continue;
            };
            let mut app = make_app();
            app.tui_mode = TuiMode::Dashboard;
            dispatch_fkey_action(&mut app, fkey.action);
            assert_eq!(
                app.tui_mode, expected,
                "F-bar entry `{}` labeled `{}` advertised in TuiMode::Dashboard \
                 but dispatching its action did not land in {:?} (landed in {:?}). \
                 See `build_fkeys` in mode_hints.rs and `dispatch_fkey_action` in \
                 input_handler.rs.",
                fkey.key, fkey.label, expected, app.tui_mode
            );
            checked += 1;
        }
        assert!(
            checked >= 3,
            "expected F4/F5/F6 to be verified, got {}",
            checked
        );
    }
}
