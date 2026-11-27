use crate::tui::activity_log::LogLevel;
use crate::tui::app::{self, App};
use crate::tui::screen_dispatch::{
    dispatch_to_active_screen_then_hook as dispatch_to_active_screen, handle_screen_action,
};
use crate::tui::screens;
use crate::tui::screens::dashboard::rows::WorkKey;
use crate::tui::screens::dashboard::usage::{View, open_agent_graph, open_view};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Result of handling a key event in the TUI loop.
pub(super) enum KeyAction {
    /// Event was handled; continue the loop.
    Consumed,
    /// The app should quit.
    Quit,
}

/// #1253: while the SizeGuard is shown (terminal below 80x24) the screen is
/// hidden, so keys go only to a confirm drawn on top of the guard
/// (`ConfirmExit`, `ConfirmKill`, `BudgetPreSpawn`). Every other key,
/// global chords included, is ignored, except `Ctrl+C`. Returns `None` when
/// the guard is not shown and the key takes the normal path.
pub(super) async fn handle_key_under_size_guard(
    app: &mut App,
    key: KeyEvent,
    area: ratatui::layout::Rect,
) -> Option<KeyAction> {
    if !crate::tui::ds::size_guard::is_shown(area) {
        return None;
    }
    // #1260: Ctrl+C opens Confirm Exit, which is drawn on the guard.
    if key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && app.tui_mode != app::TuiMode::ConfirmExit
    {
        return handle_exit_chord(app, &key);
    }
    Some(match app.tui_mode {
        app::TuiMode::ConfirmExit => handle_confirm_exit(app, &key),
        app::TuiMode::ConfirmKill(id) => handle_confirm_kill(app, &key, id).await,
        // #1276: `b` would open Settings, which the guard hides; below
        // 80x24 only `S` / `n` answer the gate.
        app::TuiMode::BudgetPreSpawn { .. } if key.code == KeyCode::Char('b') => {
            KeyAction::Consumed
        }
        app::TuiMode::BudgetPreSpawn { session_id } => {
            handle_budget_prespawn(app, &key, session_id).await
        }
        _ => KeyAction::Consumed,
    })
}

/// Top-level key event dispatcher. Handles overlays, mode-specific input,
/// global shortcuts, and screen dispatch in priority order. After every
/// key, a session the budget gate's `b` left parked gets its gate back
/// once Settings is left, by any route (#1276); never while a text field,
/// the palette or a typed confirm has the keys.
pub(super) async fn handle_key(app: &mut App, key: KeyEvent) -> KeyAction {
    let action = dispatch_key(app, key).await;
    if !is_text_input_mode(app) {
        app.restore_budget_gate();
    }
    action
}

async fn dispatch_key(app: &mut App, key: KeyEvent) -> KeyAction {
    // #1280: a key the Dashboard does not take disarms its armed PR row.
    screens::dashboard::dispatch::note_key(app, &key);
    // If confirm exit dialog is showing, handle it
    if app.tui_mode == app::TuiMode::ConfirmExit {
        return handle_confirm_exit(app, &key);
    }

    // #1260: ⌃C, ⌃X and ⌃Q go through Confirm Exit — HIGHEST PRIORITY.
    if let Some(action) = handle_exit_chord(app, &key) {
        return action;
    }

    // #1263: the shown log drawer is modal. Only Ctrl+L and Esc (close)
    // act; every other key is swallowed so nothing under it fires. The
    // quit chords above still win.
    if crate::tui::ui::log_drawer_shown(app) {
        let ctrl_l =
            key.code == KeyCode::Char('l') && key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl_l || key.code == KeyCode::Esc {
            app.activity_log.drawer_open = false;
        }
        return KeyAction::Consumed;
    }

    // #1255: the open palette takes every key; the quit chords above win.
    if app.palette.is_some() {
        return handle_palette(app, &key).await;
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

    // #1292: the visible file-conflict toast takes ⌃P and ⌃O.
    if app.conflict_toast_key(&key, is_text_input_mode(app)) {
        return KeyAction::Consumed;
    }

    if key.code == KeyCode::Char('A') && !is_text_input_mode(app) {
        app.cycle_selected_agent();
        return KeyAction::Consumed;
    }

    // [e] opens the manual CI Error Review popup when at least one CI
    // check is currently failing (#695). Gate on text-input mode so the
    // letter still types into editors.
    if key.code == KeyCode::Char('e')
        && !is_text_input_mode(app)
        && app.tui_mode == app::TuiMode::Dashboard
        && app.has_visible_ci_failure()
    {
        app.request_ci_error_review();
        return KeyAction::Consumed;
    }

    // 'q' quits an idle fleet, else opens Confirm Exit (not in text input).
    if key.code == KeyCode::Char('q') && !is_text_input_mode(app) {
        if !has_live_work(app) {
            app.running = false;
            return KeyAction::Quit;
        }
        open_confirm_exit(app, None);
        return KeyAction::Consumed;
    }

    // #1258: digits 1-7 switch tabs on a top-level screen.
    if let Some(mode) = tab_key_target(app, &key) {
        if mode != app.tui_mode {
            handle_screen_action(app, screens::ScreenAction::Push(mode));
        }
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

/// Handle upgrade banner input (u/y/n/Esc). #1368: only a bare key acts,
/// and never while a text field has focus, so typed text reaches the field.
fn handle_upgrade_keys(app: &mut App, key: &KeyEvent) -> bool {
    if !key.modifiers.is_empty() || is_text_input_mode(app) {
        return false;
    }
    match &app.upgrade_state {
        // #1267: `u` downloads when idle, else queues; `u` again unqueues.
        // Only `u` cancels a queued upgrade; `Esc` hides an available one.
        crate::updater::UpgradeState::Available(_) | crate::updater::UpgradeState::Queued(_) => {
            if key.code == KeyCode::Char('u') {
                app.press_upgrade();
                return true;
            }
            if key.code == KeyCode::Esc
                && matches!(
                    app.upgrade_state,
                    crate::updater::UpgradeState::Available(_)
                )
            {
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
        // #1287: the run loop takes every key.
        app::TuiMode::QueueExecution | app::TuiMode::ContinuousPause => {
            screens::run_loop::input::handle(app, key).await;
            Some(KeyAction::Consumed)
        }
        app::TuiMode::CompletionSummary => Some(handle_completion_summary(app, key)),
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
        app::TuiMode::Session { id, view } => Some(handle_session(app, key, id, view).await),
        app::TuiMode::ConfirmKill(id) => Some(handle_confirm_kill(app, key, id).await),
        app::TuiMode::BudgetPreSpawn { session_id } => {
            Some(handle_budget_prespawn(app, key, session_id).await)
        }
        app::TuiMode::SessionSwitcher => {
            handle_session_switcher(app, key);
            Some(KeyAction::Consumed)
        }
        _ => None,
    }
}

/// #1288: the session screen takes every key. A strip key (`d f l v e`)
/// switches the view in place; the rest go to the view, which acts on `id`
/// (never a list position). Keys match the Session blocks in the registry.
async fn handle_session(
    app: &mut App,
    key: &KeyEvent,
    id: uuid::Uuid,
    view: app::SessionView,
) -> KeyAction {
    use app::SessionView as V;
    if key.modifiers.is_empty()
        && let KeyCode::Char(c) = key.code
        && let Some(to) = V::for_key(c)
    {
        if to != view {
            screens::session::open_view(app, id, to);
        }
        return KeyAction::Consumed;
    }
    match view {
        V::Detail => handle_session_detail(app, key, id),
        V::Output => handle_fullscreen(app, key),
        V::Log => handle_log_viewer(app, key, id).await,
        V::Gates => handle_gate_output_viewer(app, key),
        V::Events => handle_call_log(app, key, id),
    }
}

/// Detail acts on the session on screen: `X` kills it (typed confirm), `c`
/// copies its last reply, `w` opens the switcher. Nothing falls through to
/// the Overview keys, whose target is the Dashboard's marked session.
fn handle_session_detail(app: &mut App, key: &KeyEvent, id: uuid::Uuid) -> KeyAction {
    match key.code {
        KeyCode::Char('c')
            if key.modifiers == KeyModifiers::NONE && app.copy_focused_response_enabled() =>
        {
            if let Some((kind, msg)) = app.copy_focused_response().toast() {
                app.set_copy_toast(kind, msg);
            }
        }
        KeyCode::Char('X')
            if app
                .pool
                .get_session(id)
                .is_some_and(|s| !s.status.is_terminal()) =>
        {
            app.open_kill_confirm(id);
        }
        KeyCode::Char('w') => open_session_switcher(app),
        KeyCode::Esc => app.navigate_back_or_dashboard(),
        _ => {}
    }
    KeyAction::Consumed
}

/// #1240: Output takes every key that reaches it. Falling through to
/// `handle_overview_keys` made `k` (advertised as scroll) open the kill
/// dialog for the hidden grid selection, and `p` / `r` / `D` act on the
/// whole fleet. #1288: scrolling up stops following; `End` follows again.
fn handle_fullscreen(app: &mut App, key: &KeyEvent) -> KeyAction {
    let scroll = &mut app.screen_state.output_scroll;
    match key.code {
        KeyCode::Char('k') | KeyCode::Up => scroll.up(),
        KeyCode::Char('j') | KeyCode::Down => scroll.down(),
        KeyCode::End => scroll.end(),
        KeyCode::Esc => app.navigate_back_or_dashboard(),
        _ => {}
    }
    KeyAction::Consumed
}

async fn handle_budget_prespawn(
    app: &mut App,
    key: &KeyEvent,
    session_id: uuid::Uuid,
) -> KeyAction {
    // #1276: `S` spawn, `b` raise cap, `n` drop. The modal consumes every
    // other key: no fall-through to outer chords.
    let Some(chord) = crate::tui::budget_prespawn::chord(key) else {
        return KeyAction::Consumed;
    };
    app.resolve_budget_prespawn(chord, session_id).await;
    KeyAction::Consumed
}

fn handle_completion_summary(app: &mut App, key: &KeyEvent) -> KeyAction {
    // Recovery keys (s/g/r/v/q) override the success table because `r`
    // and `q` mean different things in failed-gates mode.
    let in_failed_gates_mode = app
        .completion_summary
        .as_ref()
        .map(|s| s.has_failed_gates())
        .unwrap_or(false);
    if in_failed_gates_mode {
        return handle_completion_summary_failed_gates(app, key);
    }
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) | (KeyCode::Esc, _) | (KeyCode::Char('s'), _) => {
            // Dismiss the modal and pop back to the screen the user was
            // on before the modal opened. `open_completion_summary`
            // pushes the prior mode onto `nav_stack`, so
            // `navigate_back_or_dashboard` pops to that prior mode
            // (Overview/Detail/etc.). Falls back to Dashboard — NEVER
            // ConfirmExit — when the stack is empty. Reported 2026-05-23
            // (Esc on completion modal was bouncing to the Quit dialog).
            app.dismiss_completion_summary();
            app.navigate_back_or_dashboard();
        }
        (KeyCode::Char('o'), _) => {
            open_first_completion_pr(app);
        }
        (KeyCode::Char('i'), _) => {
            app.dismiss_completion_summary();
            let mut screen = screens::IssueBrowserScreen::new(vec![]);
            screen.loading = true;
            app.screen_state.issue_browser_screen = Some(screen);
            app.pending_commands.push(app::TuiCommand::FetchIssues);
            app.navigate_to(app::TuiMode::IssueBrowser);
        }
        (KeyCode::Char('r'), _) => {
            app.screen_state.prompt_input_screen = Some(app::helpers::create_prompt_input_screen(
                &app.prompt_history,
                app.config
                    .as_ref()
                    .map(|c| c.launch_defaults())
                    .unwrap_or((true, false)),
            ));
            app.navigate_to(app::TuiMode::PromptInput);
        }
        (KeyCode::Char('d'), _) => {
            // `transition_to_dashboard` now pushes Dashboard onto the
            // nav stack (instead of clearing it), so `Esc` from the
            // dashboard returns the user to the screen they came from.
            app.transition_to_dashboard();
        }
        (KeyCode::Char('q'), _) => {
            app.dismiss_completion_summary();
            app.navigate_to(app::TuiMode::ConfirmExit);
        }
        (KeyCode::Char('l'), _) => {
            // [l] Logs: navigate (push onto stack) to the LogViewer for
            // the first session so `Esc` returns to wherever the user
            // was. Previous code set `tui_mode` directly which skipped
            // the nav-stack push AND fell back to Overview when the
            // summary was empty — losing context. If no session exists,
            // leave the modal open instead of redirecting.
            if let Some(sid) = app
                .completion_summary
                .as_ref()
                .and_then(|s| s.sessions.first())
                .map(|line| line.session_id)
            {
                app.log_viewer_scroll = 0;
                app.dismiss_completion_summary();
                app.navigate_to(app::TuiMode::Session {
                    id: sid,
                    view: app::SessionView::Log,
                });
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
        KeyCode::Up | KeyCode::Down | KeyCode::Char('k' | 'j' | 'o')
    ) {
        app.completion_summary = None;
    }
    KeyAction::Consumed
}

fn open_first_completion_pr(app: &mut App) {
    let pr_link = app.completion_summary.as_ref().and_then(|summary| {
        summary
            .sessions
            .iter()
            .find_map(|session| non_empty_url(&session.pr_link))
    });

    let Some(url) = pr_link else {
        app.activity_log.push_simple(
            "PR".into(),
            "No PR link available for completed sessions".to_string(),
            LogLevel::Warn,
        );
        return;
    };

    match open_url(&url) {
        Ok(()) => {
            app.activity_log
                .push_simple("PR".into(), format!("Opened {}", url), LogLevel::Info)
        }
        Err(e) => app.activity_log.push_simple(
            "PR".into(),
            format!("Failed to open {}: {}", url, e),
            LogLevel::Error,
        ),
    }
}

fn non_empty_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };

    command.spawn().map(|_| ())
}

/// Recovery-modal key dispatch. Unlike the success handler, this one
/// does NOT clear `completion_summary` on every key: `[v]` round-trips
/// through the gate-output viewer and `[Esc]` returns here.
fn handle_completion_summary_failed_gates(app: &mut App, key: &KeyEvent) -> KeyAction {
    let first_failed = app
        .completion_summary
        .as_ref()
        .and_then(|s| s.first_failed_gates_session())
        .cloned();

    match (key.code, key.modifiers) {
        (KeyCode::Char('o'), _) => {
            // `[o]` Open PR: the recovery footer advertises this key, so
            // it must be live here too — not just in the success handler.
            // When CI-style gates fail *after* PR creation the session
            // carries a pr_link to open; when no PR exists (local gates)
            // `open_first_completion_pr` surfaces a clear notice instead
            // of silently doing nothing.
            open_first_completion_pr(app);
        }
        (KeyCode::Char('s'), _) => {
            if let Some(sl) = first_failed.as_ref()
                && let Some(wt) = sl.worktree_path.as_ref()
                && let Err(e) = app.shell_launcher.open_shell_at(wt)
            {
                app.activity_log.push_simple(
                    "shell".into(),
                    format!("Shell open failed: {}", e),
                    LogLevel::Error,
                );
            }
        }
        (KeyCode::Char('g'), _) => {
            if let Some(sl) = first_failed.as_ref() {
                if let Err(e) = app.retry_completion_gates(sl.session_id) {
                    app.activity_log.push_simple(
                        "gates".into(),
                        format!("Gate retry failed: {}", e),
                        LogLevel::Error,
                    );
                } else {
                    app.completion_summary = Some(app.build_completion_summary());
                }
            }
        }
        (KeyCode::Char('r'), _) => {
            if let Some(sl) = first_failed.as_ref() {
                app.spawn_resume_implement_session(sl);
                app.dismiss_completion_summary();
                app.tui_mode = app::TuiMode::Dashboard;
            }
        }
        (KeyCode::Char('q'), _) => {
            // Recovery `[q]` closes the modal, NOT ConfirmExit — the
            // success-modal disambiguation is asserted by tests. Pop back
            // to the mode the user came from (Detail/Overview) via the
            // nav stack instead of forcing Overview, so the Detail chord
            // set (`[l]`/`[L]`/`[k]`) is preserved (#894). Falls back to
            // Dashboard, never ConfirmExit, when the stack is empty.
            app.dismiss_completion_summary();
            app.navigate_back_or_dashboard();
        }
        (KeyCode::Char('v'), _) => {
            if let Some(sl) = first_failed.as_ref() {
                app.log_viewer_scroll = 0;
                app.tui_mode = app::TuiMode::Session {
                    id: sl.session_id,
                    view: app::SessionView::Gates,
                };
            }
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => app.panel_view.scroll_up(),
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => app.panel_view.scroll_down(),
        _ => {}
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
            app.tui_mode = app::TuiMode::Dashboard;
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
            app.tui_mode = app::TuiMode::Dashboard;
        }
    }
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

/// Gate-output viewer key dispatch (issue #560 → `[v]`).
///
/// Mirrors the log-viewer scroll keys, but `[Esc]` returns to the
/// failed-gates completion modal (still rendered underneath) when one is
/// open, rather than navigating to the dashboard.
fn handle_gate_output_viewer(app: &mut App, key: &KeyEvent) -> KeyAction {
    match (key.code, key.modifiers) {
        // #1288: Gates is also a session tab; with no summary to return
        // to, Esc goes back like the other views.
        // `q` never gets here: `handle_key` takes it as quit first.
        (KeyCode::Esc, _) if app.completion_summary.is_some() => {
            app.tui_mode = app::TuiMode::CompletionSummary;
        }
        (KeyCode::Esc, _) => app.navigate_back_or_dashboard(),
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

fn handle_call_log(app: &mut App, key: &KeyEvent, session_id: uuid::Uuid) -> KeyAction {
    let total = app
        .pool
        .get_session(session_id)
        .map(|s| s.call_log.len())
        .unwrap_or(0);
    let state = &mut app.call_log_state;
    state.clamp_to_total(total);
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            app.navigate_back_or_dashboard();
        }
        (KeyCode::Enter, _) if total > 0 => {
            state.toggle_expand();
        }
        // #1288: `f` opens Output now; End follows the tail, as in Output.
        (KeyCode::End, _) if !state.follow_tail => {
            state.toggle_follow_tail();
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
            // Manual movement cancels follow-tail so the user can scroll back.
            state.disable_follow_tail();
            if state.expanded {
                state.scroll_payload_up();
            } else {
                state.move_up();
            }
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
            state.disable_follow_tail();
            if state.expanded {
                state.scroll_payload_down();
            } else {
                state.move_down(total);
            }
        }
        (KeyCode::Char('g'), _) => {
            state.disable_follow_tail();
            state.jump_first();
        }
        (KeyCode::Char('G'), _) => {
            state.disable_follow_tail();
            state.jump_last(total);
        }
        _ => {}
    }
    KeyAction::Consumed
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

/// #1261: kill asks for the session's issue number (`ds::confirm`). `⏎` is
/// inert until the typed text matches; `Esc` cancels.
async fn handle_confirm_kill(app: &mut App, key: &KeyEvent, session_id: uuid::Uuid) -> KeyAction {
    use crate::tui::ds::confirm::ConfirmOutcome;
    if !matches!(&app.screen_state.kill_confirm, Some((id, _)) if *id == session_id) {
        app.screen_state.kill_confirm = Some((session_id, app.kill_confirm(session_id)));
    }
    let Some((_, confirm)) = app.screen_state.kill_confirm.as_mut() else {
        return KeyAction::Consumed;
    };
    let outcome = confirm.handle_key(*key);
    if outcome == ConfirmOutcome::Pending {
        return KeyAction::Consumed;
    }
    app.screen_state.kill_confirm = None;
    if outcome == ConfirmOutcome::Confirmed {
        app.kill_selected_session(session_id).await;
    }
    app.navigate_back_or_dashboard();
    KeyAction::Consumed
}

/// Where the session switcher sends the user on `Enter` (#930). A live
/// Interactive-mode session rejoins its chat screen; everything else opens the
/// one-shot Detail view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SwitcherTarget {
    /// Rejoin the kept-alive Interactive session for this issue.
    Interaction(u64),
    /// Open the read-only Detail view for this session id.
    Detail(uuid::Uuid),
}

/// Decide where the switcher routes `session` (#930). Pure — unit-testable
/// without the TUI.
pub(crate) fn switcher_target(session: &crate::session::types::Session) -> SwitcherTarget {
    use crate::session::types::SessionMode;
    if session.session_mode == SessionMode::Interactive
        && !session.status.is_terminal()
        && let Some(issue) = session.issue_number
    {
        SwitcherTarget::Interaction(issue)
    } else {
        SwitcherTarget::Detail(session.id)
    }
}

fn open_session_switcher(app: &mut App) {
    app.screen_state.session_switcher =
        Some(crate::tui::session_switcher::SessionSwitcher::default());
    // #895 sibling: Esc returns to the prior mode (Dashboard, Detail,
    // etc.) via the nav-stack.
    app.navigate_to(app::TuiMode::SessionSwitcher);
}

fn handle_session_switcher(app: &mut App, key: &KeyEvent) {
    // #1288: typing filters at once; ⌃ chords are not text.
    if let Some(sw) = &mut app.screen_state.session_switcher {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                return sw.push_char(c);
            }
            KeyCode::Backspace => return sw.pop_char(),
            _ => {}
        }
    }
    match key.code {
        KeyCode::Esc => {
            app.screen_state.session_switcher = None;
            app.navigate_back_or_dashboard();
        }
        KeyCode::Up | KeyCode::Down => {
            if let Some(sw) = &mut app.screen_state.session_switcher {
                let sessions = app.pool.all_sessions();
                if key.code == KeyCode::Up {
                    sw.move_up(&sessions);
                } else {
                    sw.move_down(&sessions);
                }
            }
        }
        KeyCode::Enter => {
            let target = app.screen_state.session_switcher.as_ref().and_then(|sw| {
                let sessions = app.pool.all_sessions();
                sw.selected_session(&sessions).map(switcher_target)
            });
            if let Some(target) = target {
                app.screen_state.session_switcher = None;
                // Consume the SessionSwitcher from the nav-stack so [Esc]
                // from the target returns to whatever pushed SessionSwitcher
                // (Overview) instead of an empty Switcher mode with no
                // screen-state, which leaves the header showing Switcher
                // chords while no modal is drawn (sibling of #893).
                app.navigate_back();
                match target {
                    // #930: a live Interactive session rejoins its chat — the
                    // same re-entry path #738 uses, treating it like any live
                    // session.
                    SwitcherTarget::Interaction(issue) => {
                        crate::tui::screen_dispatch::open_interaction_session(
                            app, issue, false, None,
                        );
                    }
                    SwitcherTarget::Detail(id) => app.navigate_to(app::TuiMode::Session {
                        id,
                        view: app::SessionView::Detail,
                    }),
                }
            }
        }
        _ => {}
    }
}

/// #1255: ⌃K opens the palette over the current screen. Its entries are
/// read from the registry now, so a disabled action shows why.
fn open_palette(app: &mut App) {
    use crate::tui::ds::palette::{PaletteState, Run, entries};
    use crate::tui::navigation::registry::{PaletteCommand, registry};
    let review = app.pending_review_report.is_some();
    let agent_graph = app.is_agent_graph_enabled();
    let list = entries(registry(), app.tui_mode, |run| match run {
        Run::Command(PaletteCommand::ReviewConcerns) if !review => Some("no review pending"),
        Run::Command(PaletteCommand::AgentGraph) if !agent_graph => {
            Some("views.agent_graph_enabled is off")
        }
        _ => None,
    });
    app.palette = Some(PaletteState::new(list));
}

/// #1255: a key while the palette is open. ⏎ closes it, then runs the
/// entry: a tab opens its screen, any other binding goes through
/// `handle_key` as its own key, so the palette never has its own handler
/// for a bound action (destructive ones keep their confirm).
async fn handle_palette(app: &mut App, key: &KeyEvent) -> KeyAction {
    use crate::tui::ds::palette::{Outcome, Run};
    use crate::tui::navigation::registry::{Action, PaletteCommand};
    let Some(palette) = app.palette.as_mut() else {
        return KeyAction::Consumed;
    };
    let run = match palette.handle_key(key) {
        Outcome::Open => return KeyAction::Consumed,
        Outcome::Close => None,
        Outcome::Run(run) => Some(run),
    };
    app.palette = None;
    match run {
        None => KeyAction::Consumed,
        Some(Run::Binding(Action::SwitchTab(_), chord)) => {
            if let KeyCode::Char(c) = chord.code
                && let Some(mode) = crate::tui::ds::tab_bar::tab_for_digit(c)
                && mode != app.tui_mode
            {
                handle_screen_action(app, screens::ScreenAction::Push(mode));
            }
            KeyAction::Consumed
        }
        Some(Run::Binding(_, chord)) => {
            Box::pin(handle_key(app, KeyEvent::new(chord.code, chord.mods))).await
        }
        Some(Run::Command(PaletteCommand::Network)) => {
            use crate::tui::ds::toast::{Severity, Toast};
            let toast = Toast::new(Severity::Info, NETWORK_PENDING);
            app.push_toast(toast, std::time::Instant::now());
            KeyAction::Consumed
        }
        Some(Run::Command(PaletteCommand::Turboquant)) => {
            open_view(app, View::Tokens);
            KeyAction::Consumed
        }
        Some(Run::Command(PaletteCommand::CompleteModal)) => {
            handle_screen_action(app, screens::ScreenAction::Push(app::TuiMode::Settings));
            KeyAction::Consumed
        }
        Some(Run::Command(PaletteCommand::ReviewConcerns)) => {
            app.navigate_to(app::TuiMode::PrReview);
            KeyAction::Consumed
        }
        // #1280: the Dashboard with the first pending PR row selected;
        // its ⏎ ⏎ (armed) is the retry, so the palette writes nothing.
        // #1280: the Agent graph was Overview's `g`; Esc comes back.
        Some(Run::Command(PaletteCommand::AgentGraph)) => {
            open_agent_graph(app);
            KeyAction::Consumed
        }
        // #1276: Code health (TuiMode::Sanitize) with a background scan.
        Some(Run::Command(PaletteCommand::CodeHealth)) => {
            app.open_code_health();
            KeyAction::Consumed
        }
        Some(Run::Command(PaletteCommand::PrPipeline)) => {
            handle_screen_action(app, screens::ScreenAction::Push(app::TuiMode::Dashboard));
            screens::dashboard::dispatch::select_first_pending_pr(app);
            KeyAction::Consumed
        }
    }
}

/// What `network` shows until a diagnostics view exists (#1255).
const NETWORK_PENDING: &str = "Network diagnostics: not built yet";

/// Handle global shortcuts that apply before screen dispatch (help, F-keys, Ctrl-X).
fn handle_global_shortcuts(app: &mut App, key: &KeyEvent) -> bool {
    // #1255: ⌃K opens the command palette on any screen, except while a
    // text field has focus (the field keeps ⌃K) or a modal is up.
    if key.code == KeyCode::Char('k')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && !is_text_input_mode(app)
        && !crate::tui::ui::log_drawer_blocked(app)
    {
        open_palette(app);
        return true;
    }

    // #1254: Ctrl+Z undoes and Esc dismisses only while a matching toast
    // is visible; otherwise the key falls through to the screen.
    if key.code == KeyCode::Char('z')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && app.toast_ctrl_z(std::time::Instant::now())
    {
        return true;
    }
    if key.code == KeyCode::Esc && app.toast_esc(std::time::Instant::now()) {
        return true;
    }

    // #1263: Ctrl+L opens the log drawer on any screen (`handle_key`
    // closes it while shown). Under a modal the key goes to the modal.
    if key.code == KeyCode::Char('l')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && !crate::tui::ui::log_drawer_blocked(app)
    {
        app.activity_log.drawer_open = true;
        return true;
    }

    // #1265: Shift+Q/M/C are no longer globals. TurboQuant and the
    // concerns panel open from the ⌃K palette; the session-complete modal
    // is a Settings row, saved only by Settings' ⌃S.

    // #1280: Shift+P (#521, Overview only since #1265) is retired with the
    // Overview grid; ⏎ ⏎ on a Dashboard PR pipeline row is the retry.

    // Ctrl+B toggles bypass mode from any screen (#328 AC: TUI toggle).
    if key.code == KeyCode::Char('b') && key.modifiers.contains(KeyModifiers::CONTROL) {
        let pushed = app.request_bypass_toggle();
        if pushed {
            app.navigate_to(app::TuiMode::BypassWarning);
        }
        return true;
    }

    // 'A'/'R' on the concerns panel accept/reject the focused concern (#327).
    if matches!(app.tui_mode, app::TuiMode::PrReview)
        && app.pending_review_report.is_some()
        && matches!(key.code, KeyCode::Char('a') | KeyCode::Char('r'))
        && !is_text_input_mode(app)
    {
        let action = crate::tui::screens::pr_review::actions::handle_key(*key);
        let cursor = app.concerns_cursor;
        if let Some(report) = app.pending_review_report.as_mut() {
            let (next_cursor, _mutated) = crate::tui::screens::pr_review::actions::apply(
                &mut report.concerns,
                cursor,
                action,
            );
            app.concerns_cursor = next_cursor;
        }
        // Auto-disable bypass when every concern is decided (Accepted or
        // Rejected) — the review cycle is functionally complete (#328 AC).
        if app.bypass_active
            && app
                .pending_review_report
                .as_ref()
                .map(|r| {
                    r.concerns
                        .iter()
                        .all(|c| !matches!(c.status, crate::review::types::ConcernStatus::Pending))
                })
                .unwrap_or(false)
        {
            app.deactivate_bypass("review-cycle-complete");
        }
        return true;
    }

    // Help overlay toggle. `?` must NOT open help while the user is typing in
    // a text-input screen (chat Message bar, prompt, filters) — reuse the
    // single source of truth, not a stale inline mode list (#738 QA).
    if key.code == KeyCode::F(1) || (key.code == KeyCode::Char('?') && !is_text_input_mode(app)) {
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
            if let Some(id) = app.focused_session_id() {
                app.screen_state.output_scroll = Default::default();
                app.navigate_to(app::TuiMode::Session {
                    id,
                    view: app::SessionView::Output,
                });
            }
        }
        FKeyAction::OpenCostDashboard => open_view(app, View::Cost),
        FKeyAction::OpenTokenDashboard => open_view(app, View::Tokens),
        FKeyAction::OpenDependencyGraph => app.navigate_to(app::TuiMode::DependencyGraph),
        FKeyAction::PauseAll => {
            #[cfg(unix)]
            app.pause_all();
        }
        FKeyAction::KillSelected => {
            if let Some(id) = app.focused_session_id()
                && let Some(session) = app.pool.get_session(id)
                && !session.status.is_terminal()
            {
                app.open_kill_confirm(id);
            }
        }
        FKeyAction::Exit => app.running = false,
    }
}

/// Returns true if the current TUI mode accepts text input (q should type, not quit).
///
/// Primary source of truth is the active `Screen`'s `desired_input_mode()`
/// — this lets per-step wizards (Insert on form steps, Normal elsewhere)
/// toggle the global `q` gate without the gate needing to know about
/// every screen. Falls back to a small allowlist for modes whose text
/// entry predates the `Screen` trait (`SessionSwitcher`) or whose
/// default mode is Normal but still have ambient typing surfaces
/// (`PromptInput`, `Settings`).
///
/// #1377: also the help overlay search, the PRD edit field and the Roadmap
/// label/assignee filters (their screens never report `Insert`).
pub(crate) fn is_text_input_mode(app: &App) -> bool {
    use crate::tui::navigation::InputMode;
    if app.palette.is_some() {
        return true;
    }
    if crate::tui::ui::active_screen_input_mode(app) == InputMode::Insert {
        return true;
    }
    if app.help_state.as_ref().is_some_and(|h| h.search_active) {
        return true;
    }
    let state = &app.screen_state;
    match app.tui_mode {
        app::TuiMode::PromptInput | app::TuiMode::SessionSwitcher | app::TuiMode::Settings => true,
        // #1261: typed confirms (issue number, `CONFIRM`).
        app::TuiMode::ConfirmKill(_) | app::TuiMode::BypassWarning => true,
        // #1287: the run loop's "Stop the run?" typed confirm.
        app::TuiMode::QueueExecution => state.run_loop.stop.is_some(),
        app::TuiMode::Prd => state.prd_screen.as_ref().is_some_and(|s| s.edit.is_some()),
        app::TuiMode::Roadmap => {
            (state.roadmap_screen.as_ref()).is_some_and(|s| s.editing_filter.is_some())
        }
        _ => false,
    }
}

/// The run loop restarts maestro on this key (`y` while a restart is
/// pending). #1377: only a bare `y`, and never while a text field has focus.
pub(super) fn is_restart_key(app: &App, key: &KeyEvent) -> bool {
    matches!(
        app.upgrade_state,
        crate::updater::UpgradeState::ReadyToRestart { .. }
    ) && key.code == KeyCode::Char('y')
        && key.modifiers.is_empty()
        && !is_text_input_mode(app)
}

/// The tab a plain digit opens: only on a top-level screen (inside a flow
/// digits are content, e.g. issue numbers) and never while a text field has
/// focus.
fn tab_key_target(app: &App, key: &KeyEvent) -> Option<app::TuiMode> {
    let KeyCode::Char(c) = key.code else {
        return None;
    };
    if !key.modifiers.is_empty()
        || crate::tui::ds::tab_bar::tab_index(app.tui_mode).is_none()
        || is_text_input_mode(app)
    {
        return None;
    }
    crate::tui::ds::tab_bar::tab_for_digit(c)
}

/// Agent sessions that may still be running (pool active + queued).
pub(crate) fn live_agents(app: &App) -> usize {
    app.pool.active_count() + app.pool.queued_count()
}

/// Work a quit would cut off (#1260): live sessions, a queue that has not
/// finished (also between items), or a completion / auto-PR job still in
/// flight. With any of it, quitting asks for the ⌃Q hold.
pub(crate) fn has_live_work(app: &App) -> bool {
    live_agents(app) > 0
        || !app.completions_in_flight.is_empty()
        || app
            .queue_executor
            .as_ref()
            .is_some_and(|q| !q.is_finished())
        || team_run_live(app)
        || milestone_write_live(app)
        || adapt_write_live(app)
}

/// An Adapt write in flight (#1281): quitting would stop it after the item
/// in flight, with the run cut off partway.
fn adapt_write_live(app: &App) -> bool {
    use crate::tui::screens::adapt::AdaptStep;
    app.screen_state
        .adapt_screen
        .as_ref()
        .is_some_and(|s| matches!(s.step, AdaptStep::Scaffolding | AdaptStep::Materializing))
}

/// A milestone wizard create in flight (#1284): quitting would cut it off
/// partway.
fn milestone_write_live(app: &App) -> bool {
    app.screen_state
        .milestone_wizard_screen
        .as_ref()
        .is_some_and(|w| {
            w.step() == crate::tui::screens::milestone_wizard::MilestoneWizardStep::Materializing
        })
}

/// A team run (`run_team_until`, detached) until its result lands; between
/// waves the pool is empty while it fetches the next issue.
fn team_run_live(app: &App) -> bool {
    app.screen_state
        .team_wizard_screen
        .as_ref()
        .is_some_and(|w| w.team_run_live())
}

/// The Confirm Exit question while there is live work: the most specific
/// thing a quit would cut off.
pub(crate) fn live_work_question(app: &App) -> String {
    use crate::work::executor::ExecutorPhase;
    let live = live_agents(app);
    let awaiting = app
        .queue_executor
        .as_ref()
        .is_some_and(|q| matches!(q.phase(), ExecutorPhase::AwaitingDecision { .. }));
    if live > 0 {
        format!("{live} session(s) still running. Exit anyway?")
    } else if awaiting {
        "A queue is waiting for your decision. Exit anyway?".into()
    } else if team_run_live(app) {
        "A team run is between waves. Exit anyway?".into()
    } else if milestone_write_live(app) {
        "A milestone create is still writing. Exit anyway?".into()
    } else if adapt_write_live(app) {
        "An Adapt run is still writing. Exit anyway?".into()
    } else {
        "A queue or PR job is still running. Exit anyway?".into()
    }
}

/// #1260: ⌃C, ⌃X and ⌃Q, outside Confirm Exit. ⌃C and ⌃X always open it;
/// ⌃Q quits when there is no live work ([`has_live_work`]) and otherwise
/// opens it armed (the press counts). The first ⌃X points to ⌃Q.
fn handle_exit_chord(app: &mut App, key: &KeyEvent) -> Option<KeyAction> {
    let ctrl = |c| key.code == KeyCode::Char(c) && key.modifiers.contains(KeyModifiers::CONTROL);
    if !(ctrl('q') || ctrl('c') || ctrl('x')) {
        return None;
    }
    // Terminals that report releases send one after every press.
    if key.kind == KeyEventKind::Release {
        return Some(KeyAction::Consumed);
    }
    let now = quit_now(app);
    if ctrl('q') {
        if !has_live_work(app) {
            app.running = false;
            return Some(KeyAction::Quit);
        }
        open_confirm_exit(app, Some((key, now)));
        return Some(KeyAction::Consumed);
    }
    if ctrl('x') && !app.screen_state.quit_hints.ctrl_x_shown {
        app.screen_state.quit_hints.ctrl_x_shown = true;
        quit_hint(app, QUIT_HINT_CTRL_X, now);
    }
    open_confirm_exit(app, None);
    Some(KeyAction::Consumed)
}

/// Once per launch (the run loop calls it before the first frame): say
/// that ⌃Q quits now and where the TurboQuant toggle went. It never
/// blocks a quit.
pub(crate) fn announce_quit_key(app: &mut App, now: std::time::Instant) {
    if app.screen_state.quit_hints.ctrl_q_shown {
        return;
    }
    app.screen_state.quit_hints.ctrl_q_shown = true;
    app.activity_log
        .push_simple("KEY".into(), QUIT_HINT_CTRL_Q.into(), LogLevel::Info);
    quit_hint(app, QUIT_HINT_CTRL_Q, now);
}

const QUIT_HINT_CTRL_Q: &str = "⌃Q quits now. TurboQuant: Settings › TurboQuant";
const QUIT_HINT_CTRL_X: &str = "⌃Q quits from anywhere";

fn quit_hint(app: &mut App, message: &str, now: std::time::Instant) {
    use crate::tui::ds::toast::{Severity, Toast};
    app.push_toast(Toast::new(Severity::Info, message), now);
}

/// The time the quit hold reads: `ScreenState::quit_clock` when a test
/// injects one, else the wall clock.
pub(crate) fn quit_now(app: &App) -> std::time::Instant {
    app.screen_state
        .quit_clock
        .map_or_else(std::time::Instant::now, |clock| clock())
}

/// The Confirm Exit hold: ⌃Q, the key that quits.
pub(crate) const QUIT_HOLD: crate::tui::ds::confirm::HoldToConfirm =
    crate::tui::ds::confirm::HoldToConfirm::new(KeyCode::Char('q'), KeyModifiers::CONTROL);

/// Open Confirm Exit with a fresh hold. `press` is the ⌃Q that opened it,
/// which arms the hold.
fn open_confirm_exit(app: &mut App, press: Option<(&KeyEvent, std::time::Instant)>) {
    let mut hold = QUIT_HOLD;
    if let Some((key, now)) = press {
        hold.handle_key(key, now);
    }
    app.screen_state.exit_hold = Some(hold);
    // #1255: a quit chord closes the palette; Confirm Exit is drawn alone.
    app.palette = None;
    if app.tui_mode != app::TuiMode::ConfirmExit {
        app.navigate_to(app::TuiMode::ConfirmExit);
    }
}

fn close_confirm_exit(app: &mut App) -> KeyAction {
    app.screen_state.exit_hold = None;
    if let Some(prev) = app.nav_stack.pop() {
        app.tui_mode = prev;
    } else {
        app.tui_mode = app::TuiMode::Dashboard;
    }
    KeyAction::Consumed
}

/// Handle the confirm-exit dialog (the ⌃Q hold, or y/n/Enter/Esc when idle).
fn handle_confirm_exit(app: &mut App, key: &KeyEvent) -> KeyAction {
    // #1260: while agents run only the ⌃Q hold quits; Esc (default) and
    // `n` stay. `y` and Enter do nothing.
    if has_live_work(app) {
        use crate::tui::ds::confirm::ConfirmOutcome;
        let now = quit_now(app);
        let hold = app.screen_state.exit_hold.get_or_insert(QUIT_HOLD);
        return match hold.handle_key(key, now) {
            ConfirmOutcome::Confirmed => {
                app.running = false;
                KeyAction::Quit
            }
            ConfirmOutcome::Cancelled => close_confirm_exit(app),
            ConfirmOutcome::Pending if matches!(key.code, KeyCode::Char('n' | 'N')) => {
                close_confirm_exit(app)
            }
            ConfirmOutcome::Pending => KeyAction::Consumed,
        };
    }
    // Enter is deliberately NOT an affirmative here. The dialog is often
    // opened by a menu-Enter (e.g. Quick Action "Quit"), and reflex-pressing
    // Enter again would silently destroy the session. Require explicit `y`.
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.running = false;
            KeyAction::Quit
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Enter => {
            close_confirm_exit(app)
        }
        // Nothing to lose: ⌃Q, the quit key, quits here too.
        KeyCode::Char('q')
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.kind != KeyEventKind::Release =>
        {
            app.running = false;
            KeyAction::Quit
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
        // #1280: kill is `X` as on the Dashboard list (`k` is up there).
        // #1486: `X`, `f` and `⏎` act on the Dashboard's marked session by
        // id, never a list index a finish or a promotion moved.
        (KeyCode::Char('X'), _) => {
            if let Some(id) = app.focused_session_id()
                && let Some(session) = app.pool.get_session(id)
                && !session.status.is_terminal()
            {
                app.open_kill_confirm(id);
            }
        }
        (KeyCode::Char('K'), _) => {
            // kill_all is async but we're in a sync context here;
            // it will be handled by the caller if needed
        }
        (KeyCode::Char('f'), _) => {
            if let Some(id) = app.focused_session_id() {
                app.screen_state.output_scroll = Default::default();
                app.navigate_to(app::TuiMode::Session {
                    id,
                    view: app::SessionView::Output,
                });
            }
        }
        (KeyCode::Char('$'), _) => open_view(app, View::Cost),
        (KeyCode::Char('t'), _) => open_view(app, View::Tokens),
        (KeyCode::Char('S'), _) => app.open_session_summary(),
        // #1294: Tab leaves the Dependency graph for the Dashboard.
        (KeyCode::Tab, _) => app.tui_mode = app::TuiMode::Dashboard,
        (KeyCode::Esc, _) => {
            // Match Dashboard's Esc semantics: pop nav stack, fall back
            // to Dashboard on empty stack — NEVER ConfirmExit. The user
            // canonical exit paths are `q`, `Ctrl+X`, and `F4`. Reported
            // 2026-05-23 (LaunchSession cleared the stack, so after
            // dismissing the completion modal the next `Esc` from
            // Overview was triggering the Quit dialog).
            app.navigate_back_or_dashboard();
        }
        (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => {
            // Honor the advertised `[Enter] Detail` chord. Pre-2026-05-23
            // code routed terminal (Completed/Errored/etc.) sessions to
            // `toggle_session_summary` — a popup that conflicted with the
            // hint shown in the top-bar. Detail works for terminal
            // sessions too (renders the recorded transcript), so use one
            // path for both states.
            if let Some(id) = app.focused_session_id() {
                app.navigate_to(app::TuiMode::Session {
                    id,
                    view: app::SessionView::Detail,
                });
            }
        }
        // #1509: a digit opens the Nth session row the Dashboard shows, by
        // id, not the Nth session in pool order. A row `D` dismissed since
        // the Dashboard drew opens nothing.
        (KeyCode::Char(c), _) if c.is_ascii_digit() && c != '0' => {
            let n = (c as usize) - ('1' as usize);
            let shown = app.screen_state.dashboard.list.rows.iter()
                .filter_map(|k| match k {
                    WorkKey::Session(id) => Some(*id),
                    _ => None,
                })
                .nth(n);
            if let Some(id) = shown
                && app.pool.get_session(id).is_some()
            {
                app.navigate_to(app::TuiMode::Session {
                    id,
                    view: app::SessionView::Detail,
                });
            }
        }
        (KeyCode::Char('w'), _) => open_session_switcher(app),
        // Ctrl+C is short-circuited at the top of handle_key, so reaching
        // this arm always means plain `c`. A disabled `c` falls through to
        // the trailing `_ => {}` and becomes a no-op — the dimmed hint
        // already advertises the disabled state.
        (KeyCode::Char('c'), KeyModifiers::NONE) if app.copy_focused_response_enabled() => {
            let outcome = app.copy_focused_response();
            if let Some((kind, msg)) = outcome.toast() {
                app.set_copy_toast(kind, msg);
            }
        }
        (KeyCode::Char('d'), _) => {
            app.show_activity_log = !app.show_activity_log;
        }
        (KeyCode::Char('D'), _) => {
            app.dismiss_all_completed();
        }
        (KeyCode::Up, KeyModifiers::SHIFT) => {
            app.activity_log.scroll_up();
        }
        (KeyCode::Down, KeyModifiers::SHIFT) => {
            app.activity_log.scroll_down();
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "input_handler_kill_tests.rs"]
mod kill_tests;

#[cfg(test)]
#[path = "input_handler_size_guard_tests.rs"]
mod size_guard_tests;

#[cfg(test)]
#[path = "input_handler_tab_tests.rs"]
mod tab_tests;

#[cfg(test)]
#[path = "input_handler_upgrade_tests.rs"]
mod upgrade_tests;

#[cfg(test)]
#[path = "input_handler_text_input_tests.rs"]
mod text_input_tests;

#[cfg(test)]
#[path = "input_handler_fullscreen_tests.rs"]
mod fullscreen_tests;

#[cfg(test)]
#[path = "input_handler_overview_grid_tests.rs"]
mod overview_grid_tests;

#[cfg(test)]
#[path = "input_handler_marked_tests.rs"]
mod marked_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, SessionView, TuiMode};
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
        crate::tui::make_test_app("maestro-input-handler-test")
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
        app.tui_mode = TuiMode::Dashboard;
        assert!(!is_text_input_mode(&app));
    }

    #[test]
    fn is_text_input_mode_false_for_dependency_graph() {
        let mut app = make_app();
        app.tui_mode = TuiMode::DependencyGraph;
        assert!(!is_text_input_mode(&app));
    }

    #[test]
    fn is_text_input_mode_true_for_interaction() {
        // Regression (#738 QA): the Interaction chat must be a text-input mode
        // so global single-letter chords (`q` quit, `?` help, `d` log) don't
        // swallow keys the user is typing into the Message bar.
        let mut app = make_app();
        app.tui_mode = TuiMode::Interaction;
        app.screen_state.interaction_screen = Some(crate::tui::screens::InteractionScreen::new());
        assert!(is_text_input_mode(&app));
    }

    #[test]
    fn question_mark_does_not_open_help_in_interaction_chat() {
        // Regression (#738 QA): `?` is a normal character in the chat Message
        // bar, not the global help toggle.
        let mut app = make_app();
        app.tui_mode = TuiMode::Interaction;
        app.screen_state.interaction_screen = Some(crate::tui::screens::InteractionScreen::new());
        let consumed = handle_global_shortcuts(&mut app, &key('?'));
        assert!(
            !consumed,
            "? must fall through to the chat, not the help toggle"
        );
        assert!(
            app.help_state.is_none(),
            "? must not open the help overlay in chat"
        );
    }

    #[test]
    fn question_mark_opens_help_on_non_text_screen() {
        // Counterpart: `?` still opens help where it is not a text input.
        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        assert!(handle_global_shortcuts(&mut app, &key('?')));
        assert!(app.help_state.is_some());
    }

    #[test]
    fn is_text_input_mode_true_for_issue_wizard_on_basic_info() {
        // Regression: pressing `q` on a wizard text field must not route
        // through the global ConfirmExit handler. The wizard returns
        // `InputMode::Insert` on BasicInfo/DorFields; the global gate
        // must honour that (mirrors neovim's mode-aware keybindings).
        let mut app = make_app();
        let mut wizard = crate::tui::screens::IssueWizardScreen::new();
        // Walk the step machine into BasicInfo (the first form step).
        use crate::tui::screens::issue_wizard::IssueWizardStep;
        while wizard.step() != IssueWizardStep::BasicInfo {
            wizard.try_advance();
        }
        app.screen_state.issue_wizard_screen = Some(wizard);
        app.tui_mode = TuiMode::IssueWizard;
        assert!(is_text_input_mode(&app));
    }

    #[test]
    fn is_text_input_mode_false_for_issue_wizard_on_type_row() {
        // The Type row requests Normal input — `q` should still route to
        // ConfirmExit because nothing is being typed here.
        let mut app = make_app();
        let mut wizard = crate::tui::screens::IssueWizardScreen::new();
        wizard.focus_type_row(); // Basics › Type row (NORMAL mode)
        app.screen_state.issue_wizard_screen = Some(wizard);
        app.tui_mode = TuiMode::IssueWizard;
        assert!(!is_text_input_mode(&app));
    }

    #[test]
    fn is_text_input_mode_true_for_milestone_wizard_on_goal_definition() {
        let mut app = make_app();
        app.screen_state.milestone_wizard_screen =
            Some(crate::tui::screens::MilestoneWizardScreen::new());
        app.tui_mode = TuiMode::MilestoneWizard;
        assert!(is_text_input_mode(&app));
    }

    #[test]
    fn is_text_input_mode_false_for_project_stats() {
        let mut app = make_app();
        app.screen_state.project_stats_screen =
            Some(crate::tui::screens::ProjectStatsScreen::new());
        app.tui_mode = TuiMode::ProjectStats;
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
        app.nav_stack.push(TuiMode::Dashboard);
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
        app.nav_stack.push(TuiMode::Dashboard);
        app.tui_mode = TuiMode::ConfirmExit;
        handle_confirm_exit(&mut app, &key_code(KeyCode::Esc));
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
    }

    #[test]
    fn n_in_confirm_exit_does_not_quit() {
        let mut app = make_app();
        app.nav_stack.push(TuiMode::Dashboard);
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
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
    }

    // ── Group 5: unrelated key swallowing ─────────────────────────────

    #[test]
    fn unrelated_key_in_confirm_exit_stays() {
        let mut app = make_app();
        app.tui_mode = TuiMode::ConfirmExit;
        app.nav_stack.push(TuiMode::Dashboard);
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

    // #1260: Ctrl+C no longer quits directly; it opens Confirm Exit.
    #[tokio::test]
    async fn ctrl_c_opens_confirm_exit() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        handle_key(&mut app, ctrl_c_event()).await;
        assert!(app.running);
        assert!(matches!(app.tui_mode, TuiMode::ConfirmExit));
    }

    #[tokio::test]
    async fn ctrl_c_in_confirm_exit_stays() {
        let mut app = make_app();
        app.tui_mode = TuiMode::ConfirmExit;
        app.nav_stack.push(TuiMode::Dashboard);
        handle_key(&mut app, ctrl_c_event()).await;
        assert!(app.running);
        assert!(matches!(app.tui_mode, TuiMode::ConfirmExit));
    }

    #[tokio::test]
    async fn q_in_overview_enters_confirm_exit() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        // #1260: an idle `q` quits at once; with a live agent it asks.
        app.pool.enqueue(crate::session::types::Session::new(
            "prompt".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        ));
        handle_key(&mut app, key('q')).await;
        assert!(matches!(app.tui_mode, TuiMode::ConfirmExit));
        assert_eq!(app.nav_stack.peek(), Some(&TuiMode::Dashboard));
        assert!(app.running);
    }

    // ── Poka-yoke (#8): ConfirmKill must not treat Enter as "kill" ────────

    #[tokio::test]
    async fn confirm_kill_enter_does_not_kill() {
        // Enter is the safe/confirm key everywhere else, so a reflexive Enter
        // must NOT destroy a session. The modal stays open until the issue
        // number is typed (#1261).
        let mut app = make_app();
        let id = uuid::Uuid::new_v4();
        app.tui_mode = TuiMode::ConfirmKill(id);
        app.nav_stack.push(TuiMode::Dashboard);
        handle_key(&mut app, key_code(KeyCode::Enter)).await;
        assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    }

    #[tokio::test]
    async fn confirm_kill_esc_cancels() {
        // #1261: `n` is now typed text; Esc is the cancel (and the default).
        let mut app = make_app();
        let id = uuid::Uuid::new_v4();
        app.tui_mode = TuiMode::ConfirmKill(id);
        app.nav_stack.push(TuiMode::Dashboard);
        handle_key(&mut app, key('n')).await;
        assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
        handle_key(&mut app, key_code(KeyCode::Esc)).await;
        assert!(!matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    }

    // ── Issue #342: Nav-stack integration tests ──────────────────────────

    #[test]
    fn navigate_to_pushes_current_mode_and_switches() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        app.navigate_to(TuiMode::IssueBrowser);
        assert_eq!(app.tui_mode, TuiMode::IssueBrowser);
        assert_eq!(app.nav_stack.peek(), Some(&TuiMode::Dashboard));
    }

    #[test]
    fn navigate_back_pops_to_previous_mode() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        app.navigate_to(TuiMode::IssueBrowser);
        app.navigate_back();
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
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
        app.tui_mode = TuiMode::Dashboard;
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
            "Sessions" => Some(TuiMode::Dashboard),
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
            // #1289: F4 / F5 open the Cost / Tokens views of the Dashboard
            // (`turboquant_mode_redirects_to_tokens` checks the view).
            FKeyAction::OpenCostDashboard => Some(TuiMode::Dashboard),
            FKeyAction::OpenTokenDashboard => Some(TuiMode::Dashboard),
            FKeyAction::OpenDependencyGraph => Some(TuiMode::DependencyGraph),
            FKeyAction::OpenSummary => Some(TuiMode::SessionSummary),
            _ => None,
        }
    }

    #[test]
    fn completion_summary_hints_dispatch_to_advertised_mode() {
        use crate::tui::navigation::keymap::mode_keymap;

        let km = mode_keymap(TuiMode::CompletionSummary, None, &[], false);
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
    // repeatedly while already on one screen used to grow the
    // breadcrumb trail by one entry per keystroke; navigating A → B → A
    // used to produce [A, B] on the stack instead of just [A].

    #[test]
    fn navigate_to_same_mode_is_a_noop() {
        let mut app = make_app();
        // #1289: TokenDashboard is a Dashboard view now; PrReview is a screen.
        app.tui_mode = TuiMode::PrReview;
        app.navigate_to(TuiMode::PrReview);
        app.navigate_to(TuiMode::PrReview);
        app.navigate_to(TuiMode::PrReview);
        assert_eq!(app.tui_mode, TuiMode::PrReview);
        assert_eq!(app.nav_stack.depth(), 0);
    }

    #[test]
    fn navigate_to_truncates_to_existing_ancestor_instead_of_pushing() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        app.navigate_to(TuiMode::PrReview);
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
        app.navigate_to(TuiMode::PrReview);
        app.navigate_to(TuiMode::PrReview);
        app.navigate_to(TuiMode::PrReview);
        assert_eq!(app.tui_mode, TuiMode::PrReview);
        assert_eq!(
            app.nav_stack.breadcrumbs(),
            &[TuiMode::Dashboard, TuiMode::DependencyGraph]
        );
    }

    #[test]
    fn fkey_dashboard_mode_advertises_f4_f5_f6_and_each_dispatches() {
        use crate::tui::navigation::keymap::mode_keymap;
        let km = mode_keymap(TuiMode::Dashboard, None, &[], false);
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

    // ── copy-focused-response keybinding ─────────────────────────────────

    use crate::session::types::{Session, SessionStatus};
    use crate::tui::clipboard::testing::MockClipboard;
    use crate::tui::clipboard_toast::CopyToastKind;

    fn make_clipboard_app(mock: MockClipboard) -> App {
        crate::tui::make_test_app("maestro-copy-key-test").with_clipboard(Box::new(mock))
    }

    fn completed_session(last_message: &str) -> Session {
        let mut s = Session::new(
            "task".into(),
            "claude-opus-4-5".into(),
            "orchestrator".into(),
            None,
            None,
        );
        s.status = SessionStatus::Completed;
        s.last_message = last_message.to_string();
        s
    }

    #[tokio::test]
    async fn c_key_with_enabled_state_writes_clipboard_and_sets_toast() {
        let mock = MockClipboard::new();
        let mut app = make_clipboard_app(mock.clone());
        app.pool.enqueue(completed_session("result text"));
        app.mark_session_at(0);
        // #1294: `c` is Cost on the Dashboard; the copy key lives in
        // the shared session keys (`handle_overview_keys`).
        app.tui_mode = TuiMode::DependencyGraph;

        let _ = handle_key(&mut app, key('c')).await;

        assert_eq!(mock.recorded_writes(), vec!["result text"]);
        let toast = app.copy_toast.as_ref().expect("toast must be set");
        assert!(matches!(toast.kind, CopyToastKind::Success));
        assert_eq!(toast.message, "Response copied to clipboard");
    }

    #[tokio::test]
    async fn c_key_with_disabled_state_does_not_write_clipboard() {
        let mock = MockClipboard::new();
        let mut app = make_clipboard_app(mock.clone());
        let mut s = Session::new(
            "task".into(),
            "claude-opus-4-5".into(),
            "orchestrator".into(),
            None,
            None,
        );
        s.status = SessionStatus::Running;
        s.last_message = "in progress".to_string();
        app.pool.enqueue(s);
        app.mark_session_at(0);
        // #1294: `c` is Cost on the Dashboard; the copy key lives in
        // the shared session keys (`handle_overview_keys`).
        app.tui_mode = TuiMode::DependencyGraph;

        let _ = handle_key(&mut app, key('c')).await;

        assert_eq!(mock.write_count(), 0);
        assert!(app.copy_toast.is_none());
    }

    #[tokio::test]
    async fn c_key_with_clipboard_failure_sets_error_toast_without_panic() {
        let mock = MockClipboard::will_fail("backend exposed: /run/user/1000/wayland-0");
        let mut app = make_clipboard_app(mock.clone());
        app.pool.enqueue(completed_session("payload"));
        app.mark_session_at(0);
        // #1294: `c` is Cost on the Dashboard; the copy key lives in
        // the shared session keys (`handle_overview_keys`).
        app.tui_mode = TuiMode::DependencyGraph;

        let _ = handle_key(&mut app, key('c')).await;

        assert_eq!(mock.write_count(), 0);
        let toast = app.copy_toast.as_ref().expect("toast must be set");
        assert!(matches!(toast.kind, CopyToastKind::Error));
        assert_eq!(toast.message, "Copy failed: clipboard unavailable");
    }

    #[tokio::test]
    async fn ctrl_c_does_not_trigger_copy_path() {
        let mock = MockClipboard::new();
        let mut app = make_clipboard_app(mock.clone());
        app.pool.enqueue(completed_session("done"));
        app.mark_session_at(0);
        app.tui_mode = TuiMode::Dashboard;

        let _ = handle_key(&mut app, ctrl_c_event()).await;

        // #1260: Ctrl+C opens Confirm Exit; it never copies.
        assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
        assert_eq!(mock.write_count(), 0);
        assert!(app.copy_toast.is_none());
    }

    // ── Issue #528: agent-graph toggle keybinding (`g`) ──────────────────

    fn agent_graph_app(enabled: bool) -> App {
        let mut app = make_app();
        let toml = format!(
            "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n\
             per_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
             [github]\n[notifications]\n[views]\nagent_graph_enabled = {enabled}\n"
        );
        app.config = Some(toml::from_str(&toml).expect("test config parse"));
        app
    }

    /// #1292: `g` on the Dashboard opens Deps; `g` again switches the
    /// issues and files lenses (the old Overview ⇄ Agent graph toggle).
    fn deps_lens(app: &App) -> crate::tui::screens::dashboard::deps::Lens {
        crate::tui::screens::dashboard::deps::lens(
            &app.screen_state.dashboard,
            app.is_agent_graph_enabled(),
        )
    }

    #[tokio::test]
    async fn g_key_with_flag_on_opens_deps_then_files() {
        use crate::tui::screens::dashboard::{deps::Lens, usage::View};
        let mut app = agent_graph_app(true);
        app.tui_mode = TuiMode::Dashboard;
        let _ = handle_key(&mut app, key('g')).await;
        assert_eq!(app.screen_state.dashboard.view, View::Deps);
        assert_eq!(deps_lens(&app), Lens::Issues);
        let _ = handle_key(&mut app, key('g')).await;
        assert_eq!(deps_lens(&app), Lens::Files);
    }

    #[tokio::test]
    async fn g_key_with_flag_off_never_shows_files() {
        use crate::tui::screens::dashboard::deps::Lens;
        let mut app = agent_graph_app(false);
        app.tui_mode = TuiMode::Dashboard;
        let _ = handle_key(&mut app, key('g')).await;
        let _ = handle_key(&mut app, key('g')).await;
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
        assert_eq!(deps_lens(&app), Lens::Issues);
    }

    #[tokio::test]
    async fn g_key_round_trips_issues_files_issues() {
        use crate::tui::screens::dashboard::deps::Lens;
        let mut app = agent_graph_app(true);
        app.tui_mode = TuiMode::Dashboard;
        for want in [Lens::Issues, Lens::Files, Lens::Issues] {
            let _ = handle_key(&mut app, key('g')).await;
            assert_eq!(deps_lens(&app), want);
        }
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
    }

    /// Regression guard: LogViewer also binds `g` (scroll to top). The
    /// secondary-mode dispatch fires before `handle_overview_keys`, so the
    /// LogViewer `g` must NOT cross over into the agent-graph toggle.
    #[tokio::test]
    async fn g_key_in_log_viewer_scrolls_to_top_not_toggle() {
        let mut app = agent_graph_app(true);
        let id = uuid::Uuid::new_v4();
        app.tui_mode = TuiMode::Session {
            id,
            view: SessionView::Log,
        };
        app.log_viewer_scroll = 42;
        let _ = handle_key(&mut app, key('g')).await;
        assert!(
            matches!(
                app.tui_mode,
                TuiMode::Session {
                    view: crate::tui::app::SessionView::Log,
                    ..
                }
            ),
            "g in LogViewer must stay in LogViewer, got {:?}",
            app.tui_mode
        );
        assert_eq!(app.log_viewer_scroll, 0, "g in LogViewer scrolls to top");
    }

    /// AC: "Switching does not interrupt running sessions."
    #[tokio::test]
    async fn g_key_does_not_modify_session_pool() {
        let mut app = agent_graph_app(true);
        app.tui_mode = TuiMode::Dashboard;
        let mut s = Session::new(
            "running task".into(),
            "claude-opus-4-5".into(),
            "orchestrator".into(),
            Some(42),
            None,
        );
        s.status = SessionStatus::Running;
        app.pool.enqueue(s);
        let count_before = app.pool.total_count();

        let _ = handle_key(&mut app, key('g')).await;
        let _ = handle_key(&mut app, key('g')).await;

        assert_eq!(
            deps_lens(&app),
            crate::tui::screens::dashboard::deps::Lens::Files
        );
        assert_eq!(app.pool.total_count(), count_before);
        let sessions = app.pool.all_sessions();
        assert_eq!(sessions[0].status, SessionStatus::Running);
    }

    // ── Issue #560: failed-gates recovery handler — `[s]` shell ───────

    use crate::tui::app::types::{CompletionSessionLine, CompletionSummaryData, GateFailureInfo};
    use crate::tui::shell_launcher::CapturingShellLauncher;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;

    fn failed_gates_summary(worktree: Option<&str>) -> CompletionSummaryData {
        CompletionSummaryData {
            sessions: vec![CompletionSessionLine {
                session_id: uuid::Uuid::nil(),
                label: "#560".to_string(),
                status: crate::session::types::SessionStatus::FailedGates,
                cost_usd: 0.0,
                elapsed: "0s".to_string(),
                pr_link: String::new(),
                error_summary: String::new(),
                gate_failures: vec![GateFailureInfo {
                    gate: "clippy".to_string(),
                    message: "function never used".to_string(),
                }],
                worktree_path: worktree.map(PathBuf::from),
                issue_number: Some(560),
                model: "opus".to_string(),
                agent_id: None,
            }],
            total_cost_usd: 0.0,
            session_count: 1,
            suggestions: vec![],
            selected_suggestion: 0,
        }
    }

    fn completed_summary() -> CompletionSummaryData {
        CompletionSummaryData {
            sessions: vec![CompletionSessionLine {
                session_id: uuid::Uuid::nil(),
                label: "#560".to_string(),
                status: crate::session::types::SessionStatus::Completed,
                cost_usd: 0.0,
                elapsed: "0s".to_string(),
                pr_link: String::new(),
                error_summary: String::new(),
                gate_failures: vec![],
                worktree_path: None,
                issue_number: Some(560),
                model: "opus".to_string(),
                agent_id: None,
            }],
            total_cost_usd: 0.0,
            session_count: 1,
            suggestions: vec![],
            selected_suggestion: 0,
        }
    }

    // ── [o] Open PR must be wired in failed-gates recovery mode ──────
    // Regression (2026-06-14): the recovery handler had no `[o]` arm, so
    // the advertised "Open PR" hint was a dead key after a gate failure.
    // With no pr_link present it must at least surface the "No PR link
    // available" notice — proving `[o]` now routes to
    // `open_first_completion_pr` instead of falling into `_ => {}`.
    #[test]
    fn failed_gates_o_key_routes_to_open_pr() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(failed_gates_summary(Some(".maestro/worktrees/issue-560")));

        handle_completion_summary(&mut app, &key('o'));

        assert!(
            app.activity_log
                .entries()
                .iter()
                .any(|e| e.message.contains("No PR link available")),
            "[o] in failed-gates recovery mode must route to open_first_completion_pr"
        );
    }

    #[test]
    fn completion_summary_s_key_calls_shell_launcher_with_worktree_path() {
        let calls: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        let launcher = CapturingShellLauncher {
            calls: calls.clone(),
            should_fail: false,
        };
        let mut app = make_app().with_shell_launcher(Box::new(launcher));
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(failed_gates_summary(Some(".maestro/worktrees/issue-560")));

        handle_completion_summary(&mut app, &key('s'));

        let (len, path) = {
            let captured = calls.lock().expect("mutex");
            (captured.len(), captured[0].clone())
        };
        assert_eq!(len, 1);
        assert_eq!(path, PathBuf::from(".maestro/worktrees/issue-560"));
    }

    #[test]
    fn completion_summary_s_key_logs_error_when_shell_launch_fails() {
        let calls: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        let launcher = CapturingShellLauncher {
            calls,
            should_fail: true,
        };
        let mut app = make_app().with_shell_launcher(Box::new(launcher));
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(failed_gates_summary(Some(".maestro/worktrees/issue-560")));

        handle_completion_summary(&mut app, &key('s'));

        // Modal stays open so the user can choose another action.
        assert_eq!(app.tui_mode, TuiMode::CompletionSummary);
        assert!(
            app.activity_log
                .entries()
                .iter()
                .any(|e| e.message.to_lowercase().contains("shell")),
            "shell launch failure must produce an activity-log entry"
        );
    }

    #[test]
    fn completion_summary_g_key_calls_retry_completion_gates_for_failed_session() {
        use crate::gates::runner::GateCheck;
        use crate::gates::types::{CompletionGate, GateResult};
        use std::sync::atomic::{AtomicU32, Ordering};

        // Inline trait fake so this test isn't coupled to gate_retry's
        // CapturingGateRunner internal type.
        struct CountingRunner {
            count: AtomicU32,
        }
        impl GateCheck for CountingRunner {
            fn run_gates(&self, _: &[CompletionGate], _: &std::path::Path) -> Vec<GateResult> {
                self.count.fetch_add(1, Ordering::SeqCst);
                vec![GateResult::fail("clippy", "still failing on retry")]
            }
        }

        let runner = std::sync::Arc::new(CountingRunner {
            count: AtomicU32::new(0),
        });
        let mut app = make_app().with_gate_runner(Box::new(runner.clone()));
        let toml = "[project]\nrepo = \"owner/repo\"\n\
                    [sessions]\n\
                    [sessions.completion_gates]\nenabled = true\n\
                    [[sessions.completion_gates.commands]]\n\
                    name = \"clippy\"\nrun = \"echo c\"\nrequired = true\n\
                    [budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\n\
                    alert_threshold_pct = 80\n\
                    [github]\n[notifications]\n\
                    [gates]\nenabled = false\ntest_command = \"\"\n";
        app.config = Some(toml::from_str(toml).expect("config"));

        // Build a real session with a worktree path so retry can find it.
        use crate::session::transition::TransitionReason;
        use crate::session::types::Session;
        let mut session = Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(560),
            None,
        );
        let session_id = session.id;
        session.worktree_path = Some(PathBuf::from("/tmp/wt"));
        session
            .transition_to(
                crate::session::types::SessionStatus::Spawning,
                TransitionReason::Promoted,
            )
            .expect("Q->Sp");
        session
            .transition_to(
                crate::session::types::SessionStatus::Running,
                TransitionReason::Spawned,
            )
            .expect("Sp->Ru");
        session
            .transition_to(
                crate::session::types::SessionStatus::GatesRunning,
                TransitionReason::GatesStarted,
            )
            .expect("Ru->GR");
        session
            .transition_to(
                crate::session::types::SessionStatus::FailedGates,
                TransitionReason::GatesFailed,
            )
            .expect("GR->FG");
        app.pool.enqueue(session);
        app.pool.try_promote();
        app.pool.finalize(session_id);

        let mut summary = failed_gates_summary(Some("/tmp/wt"));
        summary.sessions[0].session_id = session_id;
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(summary);

        handle_completion_summary(&mut app, &key('g'));

        assert_eq!(runner.count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn completion_summary_g_key_does_nothing_when_no_failed_gates_session() {
        use crate::gates::runner::GateCheck;
        use crate::gates::types::{CompletionGate, GateResult};
        use std::sync::atomic::{AtomicU32, Ordering};

        struct CountingRunner {
            count: AtomicU32,
        }
        impl GateCheck for CountingRunner {
            fn run_gates(&self, _: &[CompletionGate], _: &std::path::Path) -> Vec<GateResult> {
                self.count.fetch_add(1, Ordering::SeqCst);
                vec![]
            }
        }

        let runner = std::sync::Arc::new(CountingRunner {
            count: AtomicU32::new(0),
        });
        let mut app = make_app().with_gate_runner(Box::new(runner.clone()));
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(completed_summary());

        handle_completion_summary(&mut app, &key('g'));

        assert_eq!(
            runner.count.load(Ordering::SeqCst),
            0,
            "[g] on a success modal must not invoke the gate runner"
        );
    }

    #[test]
    fn completion_summary_r_key_in_failed_gates_mode_spawns_resume_session() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(failed_gates_summary(Some(".maestro/worktrees/issue-560")));

        handle_completion_summary(&mut app, &key('r'));

        assert_eq!(app.pending_session_launches.len(), 1);
        assert!(
            app.pending_session_launches[0]
                .prompt
                .contains("/implement #560 --continue"),
        );
        assert_eq!(
            app.tui_mode,
            TuiMode::Dashboard,
            "after spawning resume the modal closes and we land on Overview"
        );
    }

    #[test]
    fn completion_summary_r_key_in_success_mode_navigates_to_prompt_input() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(completed_summary());

        handle_completion_summary(&mut app, &key('r'));

        assert_eq!(
            app.tui_mode,
            TuiMode::PromptInput,
            "success-modal [r] preserves existing PromptInput navigation"
        );
        assert!(
            app.pending_session_launches.is_empty(),
            "success-modal [r] does not spawn a session"
        );
    }

    #[test]
    fn failed_gates_q_returns_to_overview_without_confirm_exit() {
        let mut app = make_app();
        // Production opens the modal via `open_completion_summary`, which
        // pushes the prior mode (here Overview) onto `nav_stack`.
        app.nav_stack.push(TuiMode::Dashboard);
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(failed_gates_summary(Some(".maestro/worktrees/issue-560")));

        handle_completion_summary(&mut app, &key('q'));

        assert_ne!(
            app.tui_mode,
            TuiMode::ConfirmExit,
            "failed-gates [q] must NOT trigger app-exit confirmation"
        );
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
        assert!(app.completion_summary.is_none());
    }

    #[test]
    fn failed_gates_q_returns_to_prior_detail_mode() {
        let mut app = make_app();
        let id = uuid::Uuid::new_v4();
        // Mirror production: the prior Detail mode is on the nav stack when
        // the recovery modal opens. Dismissing with [q] must restore it
        // (and its [l]/[L]/[k] chord set), not force Overview (#894).
        app.nav_stack.push(TuiMode::Session {
            id,
            view: SessionView::Detail,
        });
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(failed_gates_summary(Some(".maestro/worktrees/issue-560")));

        handle_completion_summary(&mut app, &key('q'));

        assert_eq!(
            app.tui_mode,
            TuiMode::Session {
                id,
                view: SessionView::Detail
            },
            "dismissing the recovery modal must return to the prior Detail screen"
        );
        assert!(app.completion_summary.is_none());
    }

    #[test]
    fn success_modal_q_still_navigates_to_confirm_exit() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(completed_summary());

        handle_completion_summary(&mut app, &key('q'));

        assert_eq!(
            app.tui_mode,
            TuiMode::ConfirmExit,
            "success-modal [q] preserves existing ConfirmExit behaviour"
        );
    }

    #[test]
    fn completion_summary_v_key_enters_gate_output_viewer() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        let summary = failed_gates_summary(Some(".maestro/worktrees/issue-560"));
        let session_id = summary.sessions[0].session_id;
        app.completion_summary = Some(summary);

        handle_completion_summary(&mut app, &key('v'));

        assert_eq!(
            app.tui_mode,
            TuiMode::Session {
                id: session_id,
                view: SessionView::Gates
            }
        );
        assert!(
            app.completion_summary.is_some(),
            "[v] must keep the summary populated so [Esc] can return to it"
        );
        assert_eq!(app.log_viewer_scroll, 0, "[v] resets scroll on entry");
    }

    #[tokio::test]
    async fn gate_output_viewer_esc_returns_to_completion_summary() {
        let mut app = make_app();
        let summary = failed_gates_summary(Some(".maestro/worktrees/issue-560"));
        app.completion_summary = Some(summary);
        app.tui_mode = TuiMode::Session {
            id: uuid::Uuid::nil(),
            view: SessionView::Gates,
        };

        let action = handle_secondary_mode_keys(&mut app, &key_code(KeyCode::Esc)).await;
        assert!(action.is_some());
        assert_eq!(app.tui_mode, TuiMode::CompletionSummary);
        assert!(
            app.completion_summary.is_some(),
            "summary must remain populated for re-entry"
        );
    }

    #[test]
    fn completion_summary_s_key_dismisses_success_modal() {
        let calls: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        let launcher = CapturingShellLauncher {
            calls: calls.clone(),
            should_fail: false,
        };
        let mut app = make_app().with_shell_launcher(Box::new(launcher));
        // Simulate the real flow: user navigated to Overview, modal
        // auto-opened via `open_completion_summary` which pushes
        // Overview onto `nav_stack`.
        app.navigate_to(TuiMode::CompletionSummary);
        app.completion_summary = Some(completed_summary());

        handle_completion_summary(&mut app, &key('s'));

        // [s] pops the nav stack back to the prior mode (Overview)
        // instead of falling through to ConfirmExit on an empty stack
        // (2026-05-23 UX fix).
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
        assert!(app.completion_summary.is_none());
        assert!(app.completion_summary_dismissed);
        assert!(calls.lock().expect("mutex").is_empty());
    }

    /// Regression for 2026-05-23: empty `nav_stack` (e.g. modal opens on
    /// a fresh Overview with no prior navigation) must fall back to
    /// Dashboard on dismiss, NOT ConfirmExit.
    #[test]
    fn completion_summary_s_key_falls_back_to_dashboard_on_empty_stack() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(completed_summary());
        // Note: no navigate_to push — stack is empty.

        handle_completion_summary(&mut app, &key('s'));

        assert_eq!(
            app.tui_mode,
            TuiMode::Dashboard,
            "[s] with empty stack must fall back to Dashboard, NEVER ConfirmExit"
        );
        assert!(!matches!(app.tui_mode, TuiMode::ConfirmExit));
    }

    /// Regression for 2026-05-23: dismissing the completion modal pops
    /// back to Overview (correct), but the user's next `Esc` from
    /// Overview triggered ConfirmExit because the launch path cleared
    /// the nav stack. Overview now matches Dashboard's Esc semantics —
    /// falls back to Dashboard on empty stack, NEVER ConfirmExit.
    #[test]
    fn overview_esc_falls_back_to_dashboard_when_stack_empty() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        assert_eq!(app.nav_stack.depth(), 0);

        handle_overview_keys(&mut app, &key_code(KeyCode::Esc));

        assert_eq!(
            app.tui_mode,
            TuiMode::Dashboard,
            "Esc on Overview with empty stack must fall back to Dashboard"
        );
        assert!(!matches!(app.tui_mode, TuiMode::ConfirmExit));
    }

    #[test]
    fn overview_esc_pops_back_when_stack_has_history() {
        let mut app = make_app();
        app.tui_mode = TuiMode::Landing;
        app.navigate_to(TuiMode::Dashboard);
        let depth_before = app.nav_stack.depth();
        assert!(depth_before >= 1);

        handle_overview_keys(&mut app, &key_code(KeyCode::Esc));

        assert_eq!(
            app.tui_mode,
            TuiMode::Landing,
            "Esc on Overview must pop back to the previous mode"
        );
    }

    // --- #930: session-switcher routing (switcher_target) ---

    #[test]
    fn switcher_target_live_interactive_rejoins_chat() {
        use crate::session::types::{Session, SessionMode, SessionStatus};
        let mut s = Session::new(
            "p".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(42),
            None,
        );
        s.session_mode = SessionMode::Interactive;
        s.status = SessionStatus::Interactive;
        assert_eq!(switcher_target(&s), SwitcherTarget::Interaction(42));
    }

    #[test]
    fn switcher_target_killed_interactive_opens_detail() {
        use crate::session::types::{Session, SessionMode, SessionStatus};
        let mut s = Session::new(
            "p".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(42),
            None,
        );
        s.session_mode = SessionMode::Interactive;
        s.status = SessionStatus::Killed;
        assert_eq!(switcher_target(&s), SwitcherTarget::Detail(s.id));
    }

    #[test]
    fn switcher_target_one_shot_opens_detail() {
        use crate::session::types::Session;
        let s = Session::new(
            "p".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(42),
            None,
        );
        assert_eq!(switcher_target(&s), SwitcherTarget::Detail(s.id));
    }

    #[test]
    fn switcher_enter_on_live_interactive_rejoins_chat() {
        // #930: selecting a live Interactive session in the switcher reopens
        // its chat screen, not the one-shot Detail view.
        let mut app = make_app();
        app.pool.create_interaction_session(
            42,
            false,
            "opus".to_string(),
            "orchestrator".to_string(),
            None,
        );
        app.screen_state.session_switcher =
            Some(crate::tui::session_switcher::SessionSwitcher::default());

        handle_session_switcher(&mut app, &key_code(KeyCode::Enter));

        assert!(
            matches!(app.tui_mode, TuiMode::Interaction),
            "live Interactive session must rejoin the chat, got {:?}",
            app.tui_mode
        );
        assert!(app.screen_state.interaction_screen.is_some());
        assert!(app.screen_state.session_switcher.is_none());
    }

    /// Regression for 2026-05-23: `[Enter] Detail` on Overview routed
    /// terminal (Completed/Errored) sessions to `toggle_session_summary`
    /// (a popup), contradicting the top-bar chord hint. Unify both
    /// states under Detail navigation.
    #[test]
    fn overview_enter_navigates_to_detail_for_terminal_session() {
        use crate::session::types::{Session, SessionStatus};

        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        let mut session = Session::new(
            "test".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
            None,
        );
        session.status = SessionStatus::Completed;
        let id = session.id;
        app.pool.enqueue(session);
        app.mark_session_at(0);

        handle_overview_keys(&mut app, &key_code(KeyCode::Enter));

        assert!(
            app.tui_mode
                == TuiMode::Session {
                    id,
                    view: SessionView::Detail
                },
            "Enter on terminal session must navigate to Detail; got {:?}",
            app.tui_mode
        );
    }

    #[test]
    fn overview_enter_navigates_to_detail_for_running_session() {
        use crate::session::types::{Session, SessionStatus};

        let mut app = make_app();
        app.tui_mode = TuiMode::Dashboard;
        let mut session = Session::new(
            "test".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(2),
            None,
        );
        session.status = SessionStatus::Running;
        let id = session.id;
        app.pool.enqueue(session);
        app.mark_session_at(0);

        handle_overview_keys(&mut app, &key_code(KeyCode::Enter));

        assert!(
            app.tui_mode
                == TuiMode::Session {
                    id,
                    view: SessionView::Detail
                },
            "Enter on running session must navigate to Detail; got {:?}",
            app.tui_mode
        );
    }

    /// Regression: `ScreenAction::LaunchTeam` must queue
    /// `TuiCommand::RunTeam` (not the legacy `LaunchSessions` fan-out
    /// from #877) and leave the wizard at `Executing`; the wizard only
    /// advances to `LaunchSuccess` once the runner posts
    /// `TeamLaunchResult(Ok)` back through `data_handler`. Bundle PR
    /// #880 QA flagged the original Step-6/8 stuck symptom; #881
    /// replaces the fan-out with a real level-walk runner.
    #[test]
    fn launch_team_action_transitions_wizard_to_launch_success() {
        use crate::orchestration::team::{ResolvedTeam, SourceTier};
        use crate::orchestration::types::{Primitive, TeamInput};
        use crate::tui::app::TuiDataEvent;
        use crate::tui::screen_dispatch::handle_screen_action;
        use crate::tui::screens::team_wizard::types::{
            LaunchInputKind, LaunchPhase, TeamWizardMode,
        };
        use crate::tui::screens::{ScreenAction, TeamWizardScreen};
        use std::collections::HashMap;

        let mut app = make_app();
        app.tui_mode = TuiMode::TeamWizard;

        let team = ResolvedTeam {
            name: "default-coder".into(),
            primitive: Primitive::Pipeline,
            min_agents: vec!["claude".into()],
            bindings: HashMap::new(),
            source_tier: SourceTier::BuiltIn,
        };

        let mut screen = TeamWizardScreen::new(Default::default());
        screen.apply_resolved_teams(vec![team]);
        screen.set_known_agents(vec!["claude".into()]);
        screen.set_launch_team_for_test("default-coder");
        screen.set_launch_input_for_test(LaunchInputKind::Issue, Some(42));
        screen.switch_mode(TeamWizardMode::Launch);
        screen.set_launch_phase_for_test(LaunchPhase::Executing);
        app.screen_state.team_wizard_screen = Some(screen);

        handle_screen_action(
            &mut app,
            ScreenAction::LaunchTeam {
                team_name: "default-coder".into(),
                input: TeamInput::Issue { number: 42 },
                max_parallel: 1,
            },
        );

        // Dispatcher leaves the wizard at Executing and queues RunTeam.
        let mid_step = app
            .screen_state
            .team_wizard_screen
            .as_ref()
            .expect("wizard preserved")
            .launch_phase();
        assert_eq!(mid_step, LaunchPhase::Executing);
        assert!(
            app.pending_commands
                .iter()
                .any(|cmd| matches!(cmd, app::TuiCommand::RunTeam { .. })),
            "dispatcher must queue TuiCommand::RunTeam (not LaunchSessions)"
        );
        assert!(
            !app.pending_commands
                .iter()
                .any(|cmd| matches!(cmd, app::TuiCommand::LaunchSessions(_))),
            "legacy LaunchSessions fan-out must be gone"
        );

        // The runner running in the command pump would post this back.
        // Drive it synchronously to lock the round-trip contract.
        app.handle_data_event(TuiDataEvent::TeamLaunchResult(Ok(())));

        let final_step = app
            .screen_state
            .team_wizard_screen
            .as_ref()
            .expect("wizard preserved")
            .launch_phase();
        assert_eq!(
            final_step,
            LaunchPhase::Launched,
            "wizard must advance to LaunchSuccess after TeamLaunchResult(Ok)"
        );
    }

    /// Sibling regression for #881: `TeamLaunchResult(Err)` must flip
    /// the wizard to `LaunchFailed` with the runner's reason surfaced.
    #[test]
    fn team_launch_result_err_flips_wizard_to_launch_failed() {
        use crate::tui::app::TuiDataEvent;
        use crate::tui::screens::TeamWizardScreen;
        use crate::tui::screens::team_wizard::types::{LaunchPhase, TeamWizardMode};

        let mut app = make_app();
        app.tui_mode = TuiMode::TeamWizard;
        let mut screen = TeamWizardScreen::new(Default::default());
        screen.switch_mode(TeamWizardMode::Launch);
        screen.set_launch_phase_for_test(LaunchPhase::Executing);
        app.screen_state.team_wizard_screen = Some(screen);

        app.handle_data_event(TuiDataEvent::TeamLaunchResult(Err(
            "Issue #42 failed: simulated".to_string(),
        )));

        let screen = app
            .screen_state
            .team_wizard_screen
            .as_ref()
            .expect("wizard preserved");
        assert_eq!(screen.launch_phase(), LaunchPhase::Failed);
    }

    /// Regression for 2026-05-23: `ScreenAction::LaunchSession` (and its
    /// 4 siblings) used to call `nav_stack.clear()` before setting
    /// `tui_mode = Overview`. That wiped the stable anchors (Landing,
    /// Dashboard) below the launch wizard — every session start lost
    /// the Welcome breadcrumb. Replace the current wizard without
    /// touching the stack.
    #[test]
    fn launch_session_action_preserves_nav_stack_anchors() {
        use crate::tui::screen_dispatch::handle_screen_action;
        use crate::tui::screens::{ScreenAction, SessionConfig as ScreenSessionConfig};

        let mut app = make_app();
        app.tui_mode = TuiMode::Landing;
        app.navigate_to(TuiMode::Dashboard);
        app.navigate_to(TuiMode::PromptInput);
        let breadcrumbs_before: Vec<TuiMode> = app.nav_stack.breadcrumbs().to_vec();
        assert_eq!(
            breadcrumbs_before.len(),
            2,
            "fixture must have Landing + Dashboard"
        );

        let cfg = ScreenSessionConfig {
            issue_number: Some(1),
            title: "test".to_string(),
            custom_prompt: None,
            agent_id: None,
            ..ScreenSessionConfig::default()
        };
        handle_screen_action(&mut app, ScreenAction::LaunchSession(cfg));

        assert_eq!(app.tui_mode, TuiMode::Dashboard);
        assert_eq!(
            app.nav_stack.breadcrumbs(),
            breadcrumbs_before.as_slice(),
            "LaunchSession must preserve nav_stack anchors (Welcome/Dashboard)"
        );
    }

    #[test]
    fn completion_summary_s_key_pops_back_to_previous_mode() {
        let mut app = make_app();
        // Simulate Overview → Detail → CompletionSummary via real
        // navigation. [s] must pop the modal off the stack and land on
        // Detail (the screen the user was on before the modal opened),
        // NOT jump to ConfirmExit. Reported 2026-05-23.
        let detail_id = uuid::Uuid::nil();
        app.navigate_to(TuiMode::Session {
            id: detail_id,
            view: SessionView::Detail,
        });
        app.navigate_to(TuiMode::CompletionSummary);
        app.completion_summary = Some(completed_summary());

        handle_completion_summary(&mut app, &key('s'));

        assert_eq!(
            app.tui_mode,
            TuiMode::Session {
                id: detail_id,
                view: SessionView::Detail
            },
            "[s] dismiss must restore the prior mode via nav_stack pop"
        );
        assert!(app.completion_summary.is_none());
        assert!(app.completion_summary_dismissed);
        assert!(!matches!(app.tui_mode, TuiMode::ConfirmExit));
    }

    #[test]
    fn completion_summary_d_key_pushes_dashboard_onto_nav_stack() {
        let mut app = make_app();
        app.tui_mode = TuiMode::IssueBrowser;
        app.navigate_to(TuiMode::Session {
            id: uuid::Uuid::nil(),
            view: SessionView::Detail,
        });
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(completed_summary());
        let depth_before = app.nav_stack.depth();

        handle_completion_summary(&mut app, &key('d'));

        assert_eq!(app.tui_mode, TuiMode::Dashboard);
        // Esc from Dashboard should return the user to whatever they
        // were on before — `transition_to_dashboard` must push instead
        // of wiping the stack (2026-05-22 UX fix).
        assert!(
            app.nav_stack.depth() >= depth_before,
            "[d] Dashboard must push onto nav_stack, not clear it"
        );
    }

    #[test]
    fn completion_summary_l_key_navigates_to_log_viewer() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        let summary = completed_summary();
        let first_id = summary.sessions.first().expect("test fixture").session_id;
        app.completion_summary = Some(summary);

        handle_completion_summary(&mut app, &key('l'));

        assert_eq!(
            app.tui_mode,
            TuiMode::Session {
                id: first_id,
                view: SessionView::Log
            }
        );
        assert!(app.completion_summary_dismissed);
        // Post-match auto-clear nulls the summary on any non-scroll key.
        assert!(app.completion_summary.is_none());
    }

    #[test]
    fn completion_summary_l_key_with_no_sessions_keeps_modal_open() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        let mut empty_summary = completed_summary();
        empty_summary.sessions.clear();
        app.completion_summary = Some(empty_summary);

        handle_completion_summary(&mut app, &key('l'));

        // No sessions = no LogViewer to open; user should stay on the
        // overlay rather than fall through to Overview (the old code
        // routed them off the modal even when [l] was a no-op).
        assert!(!matches!(
            app.tui_mode,
            TuiMode::Session {
                view: crate::tui::app::SessionView::Log,
                ..
            }
        ));
    }

    #[test]
    fn completion_summary_o_key_without_pr_link_keeps_success_modal_open_and_logs_warning() {
        let mut app = make_app();
        app.tui_mode = TuiMode::CompletionSummary;
        app.completion_summary = Some(completed_summary());

        handle_completion_summary(&mut app, &key('o'));

        assert_eq!(app.tui_mode, TuiMode::CompletionSummary);
        assert!(
            app.completion_summary.is_some(),
            "[o] without a PR link should leave the modal open"
        );
        assert!(
            app.activity_log.entries().iter().any(|entry| {
                entry.message.contains("No PR link available")
                    && matches!(entry.level, LogLevel::Warn)
            }),
            "[o] without a PR link should explain the missing URL in the activity log"
        );
    }

    #[test]
    fn non_empty_url_accepts_only_http_urls() {
        assert_eq!(
            non_empty_url(" https://github.com/owner/repo/pull/686 ").as_deref(),
            Some("https://github.com/owner/repo/pull/686")
        );
        assert_eq!(
            non_empty_url("http://example.test").as_deref(),
            Some("http://example.test")
        );
        assert!(non_empty_url("").is_none());
        assert!(non_empty_url("not a url").is_none());
    }

    // --- Issue #886: call-log follow-tail toggle ---

    /// #1288: `f` opens the Output view now; End follows, as in Output.
    #[test]
    fn call_log_end_key_follows_tail() {
        let mut app = make_app();
        let session = completed_session("x");
        let id = session.id;
        app.pool.enqueue(session);
        app.tui_mode = TuiMode::Session {
            id,
            view: SessionView::Events,
        };

        handle_call_log(&mut app, &key_code(KeyCode::End), id);
        assert!(
            app.call_log_state.follow_tail,
            "[End] must enable follow-tail"
        );

        handle_call_log(&mut app, &key_code(KeyCode::End), id);
        assert!(
            app.call_log_state.follow_tail,
            "[End] again keeps following"
        );
    }

    #[test]
    fn call_log_manual_move_disables_follow_tail() {
        let mut app = make_app();
        let session = completed_session("x");
        let id = session.id;
        app.pool.enqueue(session);
        app.tui_mode = TuiMode::Session {
            id,
            view: SessionView::Events,
        };
        app.call_log_state.follow_tail = true;

        handle_call_log(&mut app, &key('j'), id);
        assert!(
            !app.call_log_state.follow_tail,
            "manual [j] move must cancel follow-tail"
        );
    }
}
