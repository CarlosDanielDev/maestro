use crate::budget::BudgetEnforcer;
use crate::continuous::ContinuousModeState;
use crate::mascot::MascotStyle;
use crate::mascot::animator::SystemClock;
use crate::mascot::frames::{MASCOT_ROWS_ASCII, MASCOT_WIDTH_ASCII};
use crate::mascot::widget::MascotWidget;
use crate::tui::activity_log::LogLevel;
use crate::tui::agent_graph;
use crate::tui::app::{App, TuiMode};
use crate::tui::cost_dashboard;
use crate::tui::dep_graph;
use crate::tui::detail;
use crate::tui::ds::toast::{Severity, Toast};
use crate::tui::ds::{size_guard, status_bar};
use crate::tui::fullscreen;
use crate::tui::help;
use crate::tui::icons::{self, IconId};
use crate::tui::navigation::keymap::{self, KeyBindingGroup, ModeKeyMap};
use crate::tui::screens::Screen;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Render the entire TUI.
/// #1226: on a terminal shorter than this, the landing's activity log
/// shrinks to one content row so the Release Console fits. Taller terminals
/// keep the normal log (and every startup notice in view).
const LANDING_COMPACT_LOG_BELOW_ROWS: u16 = 40;
/// Border + one content row.
const LANDING_COMPACT_LOG_HEIGHT: u16 = 3;

/// The landing is on screen, alone or under the quit modal.
fn shows_landing(app: &App) -> bool {
    match app.tui_mode {
        TuiMode::Landing => true,
        TuiMode::ConfirmExit => matches!(app.nav_stack.peek(), Some(TuiMode::Landing)),
        _ => false,
    }
}

/// #1263: the Activity pane lives on Home and Dashboard only (the session
/// Overview is part of the Dashboard). A modal keeps the screen under it.
fn shows_log_pane(app: &App) -> bool {
    let is_log_screen = |mode: &TuiMode| {
        matches!(
            mode,
            TuiMode::Landing | TuiMode::Dashboard | TuiMode::Overview
        )
    };
    match app.tui_mode {
        TuiMode::ConfirmKill(_) => true,
        TuiMode::ConfirmExit | TuiMode::BudgetPreSpawn { .. } => {
            app.nav_stack.peek().is_some_and(is_log_screen)
        }
        ref mode => is_log_screen(mode),
    }
}

/// Rows for the Activity pane and the log drawer, from
/// `tui.layout.activity_log_height` (a percentage, clamped to 10..=50).
fn configured_log_rows(app: &App, total: u16) -> u16 {
    app.config
        .as_ref()
        .map(|c| {
            let pct = c.tui.layout.activity_log_height.clamp(10, 50);
            ((total as u32 * pct as u32) / 100).max(4) as u16
        })
        .unwrap_or(10)
}

/// #1263: off the log screens, each new WARN/ERR entry raises a toast.
/// While the pane or the drawer shows the log, the entries are only seen.
fn raise_log_toasts(app: &mut App, log_visible: bool) {
    let alerts = app.activity_log.take_alerts();
    if log_visible {
        return;
    }
    let now = std::time::Instant::now();
    for entry in alerts {
        let severity = if entry.level == LogLevel::Error {
            Severity::Error
        } else {
            Severity::Warn
        };
        let message = format!("[{}] {}", entry.session_label, entry.message);
        app.push_toast(Toast::new(severity, message), now);
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    // Advance spinner animation on each draw cycle
    app.spinner_tick = app.spinner_tick.wrapping_add(1);

    // Decrement transition flash counters (#202)
    app.pool.tick_flash_counters();

    let clock = SystemClock;
    app.mascot_animator.tick(&clock);
    let derived = crate::mascot::derive_dashboard_mascot_state(app.pool.all_statuses());
    app.mascot_animator.set_state(derived, &clock);

    // #1253: below 80x24 every mode would overlap; explain the minimum instead.
    if size_guard::is_shown(f.area()) {
        draw_size_guard(f, app);
        return;
    }

    let log_pane = app.show_activity_log && shows_log_pane(app);
    let log_height = if !log_pane {
        0
    } else if shows_landing(app) && f.area().height < LANDING_COMPACT_LOG_BELOW_ROWS {
        LANDING_COMPACT_LOG_HEIGHT
    } else {
        configured_log_rows(app, f.area().height)
    };

    // #1257: the status bar is one state-only row (`ds::status_bar`); the
    // bypass chip lives inside it, so bypass no longer changes the layout.
    let status_bar_height: u16 = 1;
    // #1157: on short terminals the main area's minimum would otherwise eat
    // every activity-log row; shrink it so a visible log keeps one content
    // row (3 rows with borders).
    let log_floor = log_height.min(3);
    let main_min = f
        .area()
        .height
        .saturating_sub(status_bar_height + log_floor + 2)
        .min(10);
    // #1258: the shell row costs one row; on short terminals take it from
    // the log, never from the status bar or the main minimum.
    let log_height = log_height.min(
        f.area()
            .height
            .saturating_sub(status_bar_height + main_min + 2),
    );
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(status_bar_height), // status bar
            Constraint::Length(1),                 // TabBar or CrumbBar (#1258)
            Constraint::Min(main_min),             // main content
            Constraint::Length(log_height),        // activity log
            Constraint::Length(1),                 // footer of key hints (#1258)
        ])
        .split(f.area());
    // The shell row is kept apart so `chunks[0..4]` keep their meaning:
    // status bar, main content, activity log, footer.
    let shell_row = rows[1];
    let chunks = [rows[0], rows[2], rows[3], rows[4]];

    // Use preview theme if active, otherwise base theme
    let theme = app.active_theme().clone();

    let selected_status = app
        .pool
        .session_at_index(app.panel_view.selected_index())
        .map(|s| s.status);
    let agent_graph_enabled = app.is_agent_graph_enabled();
    let cache_key = (app.tui_mode, selected_status, agent_graph_enabled);
    if app.cached_mode_km.is_none() || app.cached_mode_km_key != cache_key {
        let screen_bindings = active_screen_bindings(app);
        app.cached_mode_km = Some(keymap::mode_keymap(
            app.tui_mode,
            selected_status,
            &screen_bindings,
            agent_graph_enabled,
        ));
        app.cached_mode_km_key = cache_key;
    }
    let mode_km = app.cached_mode_km.as_ref().unwrap().clone();

    draw_status_bar(f, app, mode_km.clone(), chunks[0]);

    // Budget alert banner (#776/#850) — overlays the top status bar
    // row when the global budget enforcer reports `Alert(pct)` or `Kill`.
    // No-op when enforcer is None or under threshold.
    let banner_area = Rect::new(chunks[0].x, chunks[0].y, chunks[0].width, 1);
    let _budget_banner_drawn = crate::tui::budget_banner::draw_budget_banner_if_alerting(
        f,
        app.budget_enforcer.as_ref(),
        app.total_cost,
        banner_area,
        &theme,
    );

    // Aged out before render so the empty-toast frame draws cleanly.
    app.tick_copy_toast(std::time::Instant::now());
    app.tick_notify_error();

    // Render main content based on TUI mode
    let spinner_tick = app.spinner_tick;
    match app.tui_mode {
        TuiMode::Overview => {
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
        }
        TuiMode::Detail(id) => {
            if let Some(session) = app.pool.get_session(id) {
                detail::draw_detail_with_claims(
                    f,
                    session,
                    &app.progress_tracker,
                    Some(&app.pool.file_claims),
                    chunks[1],
                    &theme,
                );
            } else {
                let sessions = app.pool.all_sessions();
                app.panel_view.draw_with_claims(
                    f,
                    &sessions,
                    Some(&app.pool.file_claims),
                    chunks[1],
                    &theme,
                    spinner_tick,
                );
            }
        }
        TuiMode::DependencyGraph => {
            dep_graph::draw_dep_graph(
                f,
                app.work_assignment_service.as_ref().map(|s| s.inner()),
                chunks[1],
                &app.theme,
            );
        }
        TuiMode::AgentGraph => {
            // Canonical render-time gate; see TuiMode::AgentGraph docs.
            let sessions = app.pool.all_sessions();
            if app.is_agent_graph_enabled() {
                let (nodes, edges) = agent_graph::model::build_graph(&sessions);
                agent_graph::render::draw_agent_graph(
                    f,
                    chunks[1],
                    &nodes,
                    &edges,
                    agent_graph::render::GraphRenderOptions {
                        use_nerd_font: crate::icon_mode::use_nerd_font(),
                        tick: spinner_tick,
                        sessions: &sessions,
                        theme: &theme,
                    },
                );
            } else {
                app.panel_view.draw_with_claims(
                    f,
                    &sessions,
                    Some(&app.pool.file_claims),
                    chunks[1],
                    &theme,
                    spinner_tick,
                );
            }
        }
        TuiMode::Fullscreen(id) => {
            if let Some(session) = app.pool.get_session(id) {
                fullscreen::draw_fullscreen(
                    f,
                    session,
                    &app.progress_tracker,
                    chunks[1],
                    &theme,
                    spinner_tick,
                );
            } else {
                let sessions = app.pool.all_sessions();
                app.panel_view
                    .draw(f, &sessions, chunks[1], &theme, spinner_tick);
            }
        }
        TuiMode::CostDashboard => {
            let sessions = app.pool.all_sessions();
            let budget_limit = app.budget_enforcer.as_ref().map(|e| e.total_limit());
            cost_dashboard::draw_cost_dashboard(
                f,
                &sessions,
                app.total_cost,
                budget_limit,
                chunks[1],
                &theme,
            );
        }
        TuiMode::Dashboard => {
            if let Some(ref mut screen) = app.screen_state.home_screen {
                screen.set_mascot(
                    app.show_mascot,
                    app.mascot_animator.state(),
                    app.mascot_animator.frame_index(),
                    app.mascot_style,
                );
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::Landing => {
            if let Some(ref mut screen) = app.screen_state.landing_screen {
                screen.set_mascot(
                    app.mascot_animator.state(),
                    app.mascot_animator.frame_index(),
                    app.mascot_style,
                );
                screen.set_animation_context(spinner_tick);
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::IssueWizard => {
            if let Some(ref mut screen) = app.screen_state.issue_wizard_screen {
                screen.set_spinner_context(spinner_tick, crate::icon_mode::use_nerd_font());
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::ProjectStats => {
            if let Some(ref mut screen) = app.screen_state.project_stats_screen {
                screen.set_spinner_context(spinner_tick);
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::MilestoneWizard => {
            if let Some(ref mut screen) = app.screen_state.milestone_wizard_screen {
                screen.set_spinner_context(spinner_tick, crate::icon_mode::use_nerd_font());
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::TeamWizard => {
            if let Some(ref mut screen) = app.screen_state.team_wizard_screen {
                screen.set_spinner_context(spinner_tick, crate::icon_mode::use_nerd_font());
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::IssueBrowser => {
            if let Some(ref mut screen) = app.screen_state.issue_browser_screen {
                screen.set_spinner_context(spinner_tick);
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::MilestoneView => {
            if let Some(ref mut screen) = app.screen_state.milestone_screen {
                screen.set_spinner_context(spinner_tick);
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::PromptInput => {
            if let Some(ref mut screen) = app.screen_state.prompt_input_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::Interaction => {
            // Project the live session into the screen each frame (#950): the
            // screen owns no transcript. Skipped once the session is `Killed`
            // (quit) — `interactive_managed` returns None then, so the view
            // freezes for the terminal banner.
            if let Some(ref mut screen) = app.screen_state.interaction_screen {
                if let Some(managed) = app.pool.interactive_managed(screen.issue_number()) {
                    screen.set_view(crate::tui::screens::InteractionView::from_session(
                        &managed.session,
                    ));
                }
                screen.set_spinner_context(spinner_tick);
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::QueueConfirmation => {
            if let Some(ref mut screen) = app.screen_state.queue_confirmation_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::AdoptPicker => {
            if let Some(ref mut screen) = app.screen_state.adopt_picker_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::FleetBoard => {
            let quota_badges = app.quota_badge_map();
            if let Some(ref mut screen) = app.screen_state.fleet_board_screen {
                screen.set_quota_badges(quota_badges);
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::QueueExecution => {
            // Draw overview underneath
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
            // Draw queue progress overlay
            if let Some(ref executor) = app.queue_executor {
                draw_queue_execution_overlay(f, executor, chunks[1], &app.theme);
            }
        }
        TuiMode::CompletionSummary => {
            // Draw overview underneath as backdrop
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
            // Draw overlay on top
            if let Some(ref summary) = app.completion_summary {
                draw_completion_overlay(f, summary, chunks[1], &app.theme);
            }
        }
        TuiMode::ContinuousPause => {
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
            if let Some(ref cont) = app.continuous_mode {
                draw_continuous_pause_overlay(f, cont, chunks[1], &app.theme);
            }
        }
        TuiMode::TokenDashboard => {
            let sessions = app.pool.all_sessions();
            draw_token_dashboard_with_app_quota(f, app, &sessions, chunks[1], &theme);
        }
        TuiMode::Sanitize => {
            if let Some(ref mut screen) = app.screen_state.sanitize_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::Settings => {
            if let Some(ref mut screen) = app.screen_state.settings_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::SessionSwitcher => {
            // Draw Overview underneath, then the switcher overlay on top
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
            if let Some(ref sw) = app.screen_state.session_switcher {
                let sessions = app.pool.all_sessions();
                sw.draw(
                    f,
                    chunks[1],
                    &sessions,
                    &theme,
                    spinner_tick,
                    crate::icon_mode::use_nerd_font(),
                );
            }
        }
        TuiMode::AdaptWizard => {
            if let Some(ref mut screen) = app.screen_state.adapt_screen {
                screen.tick();
                screen.draw(f, chunks[1], &theme);
            }
        }
        TuiMode::PrReview => {
            // If an auto-review concerns report is pending, render it in
            // place of the PR list/detail flow so the user can act on the
            // findings (#327 AC: TUI panel renders review concerns).
            if let Some(report) = app.pending_review_report.as_ref() {
                crate::tui::screens::pr_review::concerns::draw(
                    f,
                    chunks[1],
                    report,
                    app.concerns_cursor,
                    &theme,
                );
            } else if let Some(ref mut screen) = app.screen_state.pr_review_screen {
                screen.set_spinner_context(spinner_tick);
                screen.draw(f, chunks[1], &theme);
            }
        }
        TuiMode::ReleaseNotes => {
            if let Some(ref mut screen) = app.screen_state.release_notes_screen {
                screen.draw(f, chunks[1], &theme);
            }
        }
        TuiMode::LogViewer(id) => {
            crate::tui::log_viewer::draw_log_viewer(
                f,
                &mut app.log_viewer_cache,
                &app.session_logger,
                id,
                app.log_viewer_scroll,
                chunks[1],
                &theme,
            );
        }
        TuiMode::CallLog(id) => {
            if let Some(session) = app.pool.get_session(id) {
                crate::tui::call_log::draw::draw_call_log(
                    f,
                    session,
                    &app.call_log_state,
                    chunks[1],
                    &theme,
                );
            } else {
                let sessions = app.pool.all_sessions();
                app.panel_view.draw_with_claims(
                    f,
                    &sessions,
                    Some(&app.pool.file_claims),
                    chunks[1],
                    &theme,
                    spinner_tick,
                );
            }
        }
        TuiMode::GateOutputViewer(id) => {
            // Look up the session in the pool (covers both active and
            // finished — FailedGates is terminal so the session lives
            // in `finished` after the pipeline finalizes it).
            let session_clone = app.pool.get_session(id).cloned();
            if let Some(session) = session_clone {
                crate::tui::screens::gate_output_viewer::draw_gate_output_viewer(
                    f,
                    &session,
                    app.log_viewer_scroll,
                    chunks[1],
                    &theme,
                );
            }
        }
        TuiMode::ConfirmKill(_) => {
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
            if let TuiMode::ConfirmKill(id) = app.tui_mode {
                draw_confirm_kill_overlay(f, id, app, chunks[1], &theme);
            }
        }
        TuiMode::ConfirmExit => {
            match app.nav_stack.peek() {
                Some(&TuiMode::Dashboard) => {
                    if let Some(ref mut screen) = app.screen_state.home_screen {
                        screen.draw(f, chunks[1], &theme);
                    }
                }
                Some(&TuiMode::Landing) => {
                    if let Some(ref mut screen) = app.screen_state.landing_screen {
                        screen.set_mascot(
                            app.mascot_animator.state(),
                            app.mascot_animator.frame_index(),
                            app.mascot_style,
                        );
                        screen.set_animation_context(spinner_tick);
                        screen.draw(f, chunks[1], &app.theme);
                    }
                }
                Some(&TuiMode::Prd) => {
                    if let (Some(prd), Some(screen)) =
                        (app.prd.as_ref(), app.screen_state.prd_screen.as_ref())
                    {
                        crate::tui::screens::prd::draw::draw(f, chunks[1], screen, prd, &theme);
                    }
                }
                Some(&TuiMode::Roadmap) => {
                    if let Some(screen) = app.screen_state.roadmap_screen.as_mut() {
                        crate::tui::screens::roadmap::draw(
                            f,
                            chunks[1],
                            screen,
                            &theme,
                            spinner_tick,
                        );
                    }
                }
                Some(&TuiMode::CostDashboard) => {
                    let sessions: Vec<&crate::session::types::Session> =
                        app.pool.all_sessions().into_iter().collect();
                    let budget_limit = app.budget_enforcer.as_ref().map(|e| e.total_limit());
                    crate::tui::cost_dashboard::draw_cost_dashboard(
                        f,
                        &sessions,
                        app.total_cost,
                        budget_limit,
                        chunks[1],
                        &theme,
                    );
                }
                Some(&TuiMode::TokenDashboard) => {
                    let sessions: Vec<&crate::session::types::Session> =
                        app.pool.all_sessions().into_iter().collect();
                    draw_token_dashboard_with_app_quota(f, app, &sessions, chunks[1], &theme);
                }
                Some(&TuiMode::TurboquantDashboard) => {
                    let sessions: Vec<&crate::session::types::Session> =
                        app.pool.all_sessions().into_iter().collect();
                    let bit_width = app
                        .config
                        .as_ref()
                        .map(|c| c.turboquant.bit_width)
                        .unwrap_or(4);
                    crate::tui::turboquant_dashboard::draw_turboquant_dashboard(
                        f, &sessions, &app.flags, bit_width, chunks[1], &theme,
                    );
                }
                _ => {
                    let sessions = app.pool.all_sessions();
                    app.panel_view.draw_with_claims(
                        f,
                        &sessions,
                        Some(&app.pool.file_claims),
                        chunks[1],
                        &theme,
                        spinner_tick,
                    );
                }
            }
            draw_confirm_exit_overlay(f, app, chunks[1], &theme);
        }
        TuiMode::HollowRetry => {
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
            if let Some(ref mut screen) = app.screen_state.hollow_retry_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::CiErrorReview => {
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
            if let Some(ref mut screen) = app.screen_state.ci_error_review_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::SessionSummary => {
            if let Some(ref summary) = app.completion_summary {
                crate::tui::session_summary::draw_session_summary(
                    f,
                    summary,
                    app.session_summary_state.as_ref(),
                    chunks[1],
                    &theme,
                );
            }
        }
        TuiMode::TurboquantDashboard => {
            let sessions: Vec<&crate::session::types::Session> =
                app.pool.all_sessions().into_iter().collect();
            let bit_width = app
                .config
                .as_ref()
                .map(|c| c.turboquant.bit_width)
                .unwrap_or(4);
            crate::tui::turboquant_dashboard::draw_turboquant_dashboard(
                f, &sessions, &app.flags, bit_width, chunks[1], &theme,
            );
        }
        TuiMode::AdaptFollowUp => {
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
            if let Some(ref mut screen) = app.screen_state.adapt_follow_up_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::Prd => {
            // Lazy-init on first render: the dispatch shim only fires on key
            // input, so without this the screen is blank until the user
            // presses a key.
            crate::tui::screens::prd_dispatch::ensure_loaded(app);
            if app.prd_show_explore {
                // Explore panel takes the whole content area when open.
                crate::tui::screens::prd::explore::draw(
                    f,
                    chunks[1],
                    &app.prd_candidates,
                    &app.prd_candidate_parsed,
                    app.prd_explore_cursor,
                    &theme,
                );
            } else if let (Some(prd), Some(screen)) =
                (app.prd.as_ref(), app.screen_state.prd_screen.as_ref())
            {
                crate::tui::screens::prd::draw::draw(f, chunks[1], screen, prd, &theme);
            }
        }
        TuiMode::Roadmap => {
            crate::tui::screens::roadmap_dispatch::ensure_loaded(app);
            if let Some(screen) = app.screen_state.roadmap_screen.as_mut() {
                crate::tui::screens::roadmap::draw(f, chunks[1], screen, &theme, spinner_tick);
            }
        }
        TuiMode::BypassWarning => {
            // Center the warning text in the main content area.
            let para =
                ratatui::widgets::Paragraph::new(crate::tui::screens::bypass_warning::WARNING_TEXT)
                    .style(
                        ratatui::style::Style::default()
                            .fg(ratatui::style::Color::Red)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    );
            f.render_widget(para, chunks[1]);
        }
        TuiMode::MilestoneHealth => {
            if let Some(ref screen) = app.screen_state.milestone_health_screen {
                crate::tui::screens::milestone_health::draw::draw(
                    f,
                    chunks[1],
                    &theme,
                    screen,
                    spinner_tick,
                );
            }
        }
        TuiMode::BudgetPreSpawn { session_id } => {
            // Render the home dashboard underneath the modal so the user sees
            // context. Then overlay the modal at the centered sub-rect.
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &theme,
                spinner_tick,
            );
            let (projected_total, limit) = budget_prespawn_values(app, session_id);
            let area = crate::tui::budget_prespawn::modal_rect(chunks[1]);
            crate::tui::budget_prespawn::draw_budget_prespawn(
                f,
                projected_total,
                limit,
                area,
                &theme,
            );
        }
    }

    // Only render activity log area when visible
    if log_pane {
        // Conditionally split activity area for CI monitor
        let has_ci_checks = app.gh_auth_ok && !app.ci_poller.ci_check_details.is_empty();
        if has_ci_checks {
            let ci_pr_count = app.ci_poller.ci_check_details.len() as u16;
            let ci_height = (ci_pr_count * 6).min(chunks[2].height / 2).max(4);
            let activity_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(ci_height), Constraint::Min(3)])
                .split(chunks[2]);

            // Render CI monitor(s)
            let ci_area = activity_chunks[0];
            if app.ci_poller.ci_check_details.len() == 1 {
                let (&pr_number, details) = app.ci_poller.ci_check_details.iter().next().unwrap();
                let widget = crate::tui::widgets::CiMonitorWidget::new(details, &app.theme)
                    .pr_number(pr_number);
                ratatui::widgets::Widget::render(widget, ci_area, f.buffer_mut());
            } else {
                let constraints: Vec<Constraint> = app
                    .ci_poller
                    .ci_check_details
                    .keys()
                    .map(|_| Constraint::Ratio(1, app.ci_poller.ci_check_details.len() as u32))
                    .collect();
                let pr_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(constraints)
                    .split(ci_area);
                for (i, (&pr_number, details)) in app.ci_poller.ci_check_details.iter().enumerate()
                {
                    let widget = crate::tui::widgets::CiMonitorWidget::new(details, &app.theme)
                        .pr_number(pr_number)
                        .max_visible_rows(3);
                    ratatui::widgets::Widget::render(widget, pr_chunks[i], f.buffer_mut());
                }
            }
            app.activity_log.draw(
                f,
                activity_chunks[1],
                &app.theme,
                crate::icon_mode::use_nerd_font(),
            );
        } else {
            let is_dashboard = matches!(app.tui_mode, TuiMode::Dashboard);
            let log_area = chunks[2];
            if should_show_dashboard_mascot_panel(
                log_area,
                app.show_mascot,
                is_dashboard,
                app.mascot_style,
            ) {
                let panel_width = dashboard_mascot_layout(app.mascot_style).panel_width;
                let h_split = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Min(10), Constraint::Length(panel_width)])
                    .split(log_area);
                app.activity_log
                    .draw(f, h_split[0], &app.theme, crate::icon_mode::use_nerd_font());
                draw_mascot_block(
                    f,
                    app.mascot_animator.state(),
                    app.mascot_animator.frame_index(),
                    app.mascot_style,
                    h_split[1],
                    &theme,
                );
            } else {
                app.activity_log
                    .draw(f, log_area, &app.theme, crate::icon_mode::use_nerd_font());
            }
        }
    }

    // Draw gh auth warning banner if authentication lost
    if !app.gh_auth_ok {
        draw_gh_auth_warning(f, chunks[2], &app.theme);
    }

    // Draw notification banner overlay if any
    let banners = app.notifications.active_banners();
    if !banners.is_empty() {
        draw_notification_banner(f, banners[0], chunks[2], &app.theme);
    }

    // Upgrade progress (downloading / restart / failed) overlays the status
    // bar left of the pinned safety chips, so BYPASS and UNTRUSTED stay
    // visible. An available update is only the bar's chip (#1257).
    let pinned = status_bar::pinned_width(&status_model(app), icons::use_nerd_font());
    let upgrade_area = Rect::new(
        chunks[0].x,
        chunks[0].y,
        chunks[0].width.saturating_sub(pinned),
        chunks[0].height.min(1),
    );
    draw_upgrade_banner(f, &app.upgrade_state, upgrade_area, &app.theme);

    draw_shell_row(f, app, shell_row, &theme);
    draw_footer(f, app, &mode_km, chunks[3], &theme);

    // #1263: the Ctrl+L drawer covers the bottom of the main area.
    if app.activity_log.drawer_open {
        let rows = configured_log_rows(app, f.area().height);
        crate::tui::ds::log_drawer::draw(
            f,
            chunks[1],
            rows,
            &app.activity_log,
            &theme,
            crate::icon_mode::use_nerd_font(),
        );
    }
    raise_log_toasts(app, chunks[2].height > 0 || app.activity_log.drawer_open);

    // #1254: toasts sit above every screen, below the help overlay.
    if let Some(toast) = app.copy_toast.as_ref() {
        crate::tui::clipboard_toast::draw_copy_toast(
            f,
            f.area(),
            toast.kind,
            &toast.message,
            &theme,
        );
    } else if let Some(view) = app.toasts.visible(std::time::Instant::now()) {
        crate::tui::ds::toast::draw(f, f.area(), &view, &theme);
    }

    if let Some(ref help) = app.help_state {
        let input_mode = active_screen_input_mode(app);
        help::draw_help_overlay_with_search(
            f,
            f.area(),
            &mode_km,
            input_mode,
            help.scroll,
            &help.search_query,
            &theme,
        );
    }
}

/// Resolve the active screen (if any) to extract keybindings and input mode.
/// `footer_screen` adds the screens missing here; a screen added to one
/// match may belong in the other.
pub(super) fn active_screen(app: &App) -> Option<&dyn Screen> {
    match app.tui_mode {
        TuiMode::Dashboard => app
            .screen_state
            .home_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::Landing => app
            .screen_state
            .landing_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::IssueBrowser => app
            .screen_state
            .issue_browser_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::IssueWizard => app
            .screen_state
            .issue_wizard_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::MilestoneView => app
            .screen_state
            .milestone_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::MilestoneWizard => app
            .screen_state
            .milestone_wizard_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::TeamWizard => app
            .screen_state
            .team_wizard_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::ProjectStats => app
            .screen_state
            .project_stats_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::PromptInput => app
            .screen_state
            .prompt_input_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::QueueConfirmation => app
            .screen_state
            .queue_confirmation_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::Sanitize => app
            .screen_state
            .sanitize_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::Settings => app
            .screen_state
            .settings_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::PrReview => app
            .screen_state
            .pr_review_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::HollowRetry => app
            .screen_state
            .hollow_retry_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::AdaptFollowUp => app
            .screen_state
            .adapt_follow_up_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::AdaptWizard => app
            .screen_state
            .adapt_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::ReleaseNotes => app
            .screen_state
            .release_notes_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::CiErrorReview => app
            .screen_state
            .ci_error_review_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::Interaction => app
            .screen_state
            .interaction_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        _ => None,
    }
}

fn active_screen_bindings(app: &App) -> Vec<KeyBindingGroup> {
    active_screen(app)
        .map(|s| s.keybindings())
        .unwrap_or_default()
}

pub(super) fn active_screen_input_mode(app: &App) -> crate::tui::navigation::InputMode {
    active_screen(app)
        .and_then(|s| s.desired_input_mode())
        .unwrap_or(crate::tui::navigation::InputMode::Normal)
}

/// Layout knobs for the dashboard's side-panel mascot. The outer thresholds
/// gate whether the panel is drawn; `panel_width` sizes the horizontal split;
/// the inner thresholds guard the widget render inside the bordered block.
struct DashboardMascotLayout {
    outer_min_width: u16,
    outer_min_height: u16,
    panel_width: u16,
    inner_min_width: u16,
    inner_min_height: u16,
}

fn dashboard_mascot_layout(style: MascotStyle) -> DashboardMascotLayout {
    match style {
        MascotStyle::Ascii => DashboardMascotLayout {
            outer_min_width: 25,
            outer_min_height: MASCOT_ROWS_ASCII as u16 + 2,
            panel_width: MASCOT_WIDTH_ASCII as u16 + 2,
            inner_min_width: MASCOT_WIDTH_ASCII as u16,
            inner_min_height: MASCOT_ROWS_ASCII as u16,
        },
        MascotStyle::Sprite => DashboardMascotLayout {
            // Inner needs 32×16 for a full-aspect render; + 2 for the block border
            // on each axis → outer 34×18. Width pads to 44 so the activity log
            // keeps its Min(10) alongside the 32-wide panel.
            outer_min_width: 44,
            outer_min_height: 18,
            panel_width: 32,
            inner_min_width: 32,
            inner_min_height: 16,
        },
    }
}

/// Compute (projected_total, limit) for the pre-spawn modal (#776/#850).
/// Reads `App.budget_projector` against the parked session and
/// `App.budget_enforcer.total_limit()`. Returns `(0.0, 0.0)` when either
/// side is missing — the gate dispatch path only opens the modal when
/// both are present, so this branch is defensive only.
fn budget_prespawn_values(app: &App, session_id: uuid::Uuid) -> (f64, f64) {
    let projected = app
        .pool
        .get_session(session_id)
        .and_then(|session| {
            app.budget_projector
                .as_ref()
                .map(|proj| proj.projected_turn_cost(session))
        })
        .unwrap_or(0.0);
    let limit = app
        .budget_enforcer
        .as_ref()
        .map(BudgetEnforcer::total_limit)
        .unwrap_or(0.0);
    (app.total_cost + projected, limit)
}

fn should_show_dashboard_mascot_panel(
    log_area: Rect,
    show_mascot: bool,
    is_dashboard: bool,
    style: MascotStyle,
) -> bool {
    if !show_mascot || is_dashboard {
        return false;
    }
    let layout = dashboard_mascot_layout(style);
    log_area.width >= layout.outer_min_width && log_area.height >= layout.outer_min_height
}

fn draw_mascot_block(
    f: &mut Frame,
    state: crate::mascot::MascotState,
    frame_index: usize,
    style: MascotStyle,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    use ratatui::widgets::{Block, Borders};

    let color = theme.accent_success;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            format!(" {} ", icons::get(IconId::Fisheye)),
            Style::default().fg(color),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout = dashboard_mascot_layout(style);
    if inner.height >= layout.inner_min_height && inner.width >= layout.inner_min_width {
        f.render_widget(
            MascotWidget::new(state, frame_index, color).with_style(style),
            inner,
        );
    }
}

fn draw_status_bar(f: &mut Frame, app: &App, _mode_km: ModeKeyMap, area: Rect) {
    let theme = app.active_theme().clone();
    draw_status_bar_inner(f, app, &theme, area);
}

/// #1257: state only. Breadcrumbs and key hints are not part of this row.
fn draw_status_bar_inner(f: &mut Frame, app: &App, theme: &crate::tui::theme::Theme, area: Rect) {
    status_bar::draw(f, area, &status_model(app), theme);
}

/// What the status bar shows, read from the App (fleet counts from the pool).
fn status_model(app: &App) -> status_bar::StatusModel {
    let update = match &app.upgrade_state {
        crate::updater::UpgradeState::Available(info) => {
            Some(crate::tui::screens::sanitize_for_terminal(&info.version))
        }
        _ => None,
    };
    status_bar::StatusModel {
        version: env!("CARGO_PKG_VERSION").to_string(),
        fleet: status_bar::FleetCounts::from_statuses(app.pool.all_statuses()),
        spent: app.total_cost,
        cap: app.budget_enforcer.as_ref().map(|e| e.total_limit()),
        quota: app.quota_badges(),
        turboquant: app.flags.is_enabled(crate::flags::Flag::TurboQuant),
        untrusted: !app.workspace_trusted,
        bypass: app.bypass_active,
        update,
        continuous: app
            .continuous_mode
            .as_ref()
            .map(|run| (run.completed_count, run.total_attempted())),
    }
}

/// TabBar on the seven top-level screens, CrumbBar inside flows (#1258).
fn draw_shell_row(f: &mut Frame, app: &App, area: Rect, theme: &crate::tui::theme::Theme) {
    use crate::tui::ds::{crumb_bar, tab_bar};
    match tab_bar::tab_index(app.tui_mode) {
        Some(active) => tab_bar::draw(f, area, active, theme),
        None => {
            let crumbs = crumb_bar::crumbs(app.nav_stack.breadcrumbs(), app.tui_mode);
            crumb_bar::draw(f, area, &crumbs, None, theme);
        }
    }
}

/// The one footer (#1258): the screen's own hints (`Screen::footer_hints`),
/// else the mode's inline hints, with the quit hint pinned right except inside
/// the quit dialog.
fn draw_footer(
    f: &mut Frame,
    app: &App,
    mode_km: &ModeKeyMap,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    use crate::tui::ds::key_hints;
    let pairs = footer_screen(app)
        .map(|s| s.footer_hints())
        .unwrap_or_default();
    let mut hints = if pairs.is_empty() {
        mode_km.hints.to_vec()
    } else {
        key_hints::from_pairs(&pairs)
    };
    // #1263: while the log drawer is open, Esc closes it.
    if app.activity_log.drawer_open {
        hints.retain(|h| h.key != "Esc");
        hints.splice(0..0, key_hints::from_pairs(&[("Esc", "Close log")]));
    }
    let pin_quit = app.tui_mode != TuiMode::ConfirmExit;
    key_hints::draw(f, area, &hints, pin_quit, theme);
}

/// [`active_screen`] plus the screens it leaves out, for footer hints only.
fn footer_screen(app: &App) -> Option<&dyn Screen> {
    let state = &app.screen_state;
    active_screen(app).or_else(|| match app.tui_mode {
        TuiMode::MilestoneHealth => state.milestone_health_screen.as_ref().map(|s| s as _),
        TuiMode::AdoptPicker => state.adopt_picker_screen.as_ref().map(|s| s as _),
        TuiMode::FleetBoard => state.fleet_board_screen.as_ref().map(|s| s as _),
        _ => None,
    })
}

fn draw_gh_auth_warning(f: &mut Frame, area: Rect, theme: &crate::tui::theme::Theme) {
    let banner = Paragraph::new(Line::from(vec![
        Span::styled(
            " AUTH ",
            Style::default()
                .fg(theme.branding_fg)
                .bg(theme.accent_error)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            "GitHub CLI not authenticated. Run `gh auth login` to restore GitHub operations.",
            Style::default().fg(theme.accent_error),
        ),
    ]));
    f.render_widget(banner, area);
}

fn draw_notification_banner(
    f: &mut Frame,
    notification: &crate::notifications::types::Notification,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    let color = match notification.level {
        crate::notifications::types::InterruptLevel::Critical => theme.notification_critical,
        crate::notifications::types::InterruptLevel::Blocker => theme.notification_blocker,
        _ => theme.notification_default,
    };

    let banner = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {} ", notification.level.label()),
            Style::default()
                .fg(theme.branding_fg)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            &notification.title,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(": "),
        Span::styled(
            &notification.message,
            Style::default().fg(theme.text_primary),
        ),
        Span::styled("  [d]ismiss", Style::default().fg(theme.text_secondary)),
    ]));
    f.render_widget(banner, area);
}

/// The SizeGuard, with any open confirm drawn on top of it: a hidden
/// confirm would still take `y` and act unseen (#1253).
fn draw_size_guard(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = app.active_theme();
    size_guard::draw(f, area, theme);
    let popup = crate::tui::budget_prespawn::modal_rect(area);
    match app.tui_mode {
        TuiMode::ConfirmExit => draw_confirm_exit_popup(f, app, popup, theme),
        TuiMode::ConfirmKill(id) => draw_confirm_kill_popup(f, id, app, popup, theme),
        TuiMode::BudgetPreSpawn { session_id } => {
            let (projected_total, limit) = budget_prespawn_values(app, session_id);
            crate::tui::budget_prespawn::draw_budget_prespawn(
                f,
                projected_total,
                limit,
                popup,
                theme,
            );
        }
        _ => {}
    }
}

fn draw_confirm_kill_overlay(
    f: &mut Frame,
    session_id: uuid::Uuid,
    app: &App,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    let popup = help::centered_rect(40, 20, area);
    draw_confirm_kill_popup(f, session_id, app, popup, theme);
}

fn draw_confirm_kill_popup(
    f: &mut Frame,
    session_id: uuid::Uuid,
    app: &App,
    popup: Rect,
    theme: &crate::tui::theme::Theme,
) {
    use ratatui::widgets::Clear;
    f.render_widget(Clear, popup);

    let label = app
        .pool
        .get_session(session_id)
        .map(crate::tui::app::helpers::session_label)
        .unwrap_or_else(|| format!("S-{}", &session_id.to_string()[..8]));

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  Kill session {}?", label),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [y]", Style::default().fg(theme.accent_success)),
            Span::raw("es  "),
            Span::styled("[n]", Style::default().fg(theme.accent_error)),
            Span::raw("o"),
        ]),
    ];

    let block = theme
        .styled_block("Confirm Kill", false)
        .border_style(Style::default().fg(theme.accent_warning));

    f.render_widget(Paragraph::new(lines).block(block), popup);
}

fn draw_confirm_exit_overlay(
    f: &mut Frame,
    app: &App,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    let popup = help::centered_rect(50, 25, area);
    draw_confirm_exit_popup(f, app, popup, theme);
}

fn draw_confirm_exit_popup(
    f: &mut Frame,
    app: &App,
    popup: Rect,
    theme: &crate::tui::theme::Theme,
) {
    use ratatui::widgets::Clear;
    f.render_widget(Clear, popup);

    let active = app.pool.active_count();
    let has_active = active > 0;

    let message = if has_active {
        format!("  {} session(s) still running. Exit anyway?", active)
    } else {
        "  Are you sure you want to exit?".to_string()
    };

    let message_style = if has_active {
        Style::default()
            .fg(theme.accent_warning)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD)
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, message_style)),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [y]", Style::default().fg(theme.accent_success)),
            Span::raw("es  "),
            Span::styled("[n]", Style::default().fg(theme.accent_error)),
            Span::raw("o / "),
            Span::styled("[Esc]", Style::default().fg(theme.accent_error)),
            Span::raw(" / "),
            Span::styled("[Enter]", Style::default().fg(theme.accent_error)),
        ]),
    ];

    let border_color = if has_active {
        theme.accent_warning
    } else {
        theme.border_active
    };

    let block = theme
        .styled_block("Confirm Exit", false)
        .border_style(Style::default().fg(border_color));

    f.render_widget(Paragraph::new(lines).block(block), popup);
}

pub(crate) fn draw_completion_overlay(
    f: &mut Frame,
    summary: &crate::tui::app::CompletionSummaryData,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    use crate::session::types::SessionStatus;
    use ratatui::widgets::Clear;

    let overlay_area = help::centered_rect(70, 70, area);
    f.render_widget(Clear, overlay_area);

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    for sl in &summary.sessions {
        let status_color = match sl.status {
            SessionStatus::Completed => theme.accent_success,
            SessionStatus::Errored => theme.accent_error,
            _ => theme.accent_warning,
        };

        let mut spans = vec![
            Span::raw("  "),
            Span::styled(sl.status.symbol(), Style::default().fg(status_color)),
            Span::raw(" "),
            Span::styled(&sl.label, Style::default().fg(theme.accent_info)),
            Span::raw(" "),
            Span::styled(sl.status.label(), Style::default().fg(status_color)),
            Span::raw(" "),
            Span::styled(
                format!("${:.2}", sl.cost_usd),
                Style::default().fg(theme.accent_success),
            ),
            Span::raw(format!(" {}", sl.elapsed)),
        ];

        if !sl.pr_link.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                &sl.pr_link,
                Style::default()
                    .fg(theme.accent_info)
                    .add_modifier(Modifier::UNDERLINED),
            ));
        }

        lines.push(Line::from(spans));

        if !sl.error_summary.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(&sl.error_summary, Style::default().fg(theme.accent_error)),
            ]));
        }

        for gf in &sl.gate_failures {
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(
                    format!("{} {} ", icons::get(IconId::XCircle), gf.gate),
                    Style::default().fg(theme.accent_warning),
                ),
                Span::styled(&gf.message, Style::default().fg(theme.accent_error)),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  Total: "),
        Span::styled(
            format!("${:.2}", summary.total_cost_usd),
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  ({} session{})",
            summary.session_count,
            if summary.session_count != 1 { "s" } else { "" }
        )),
    ]));

    // Conflict suggestions section
    if !summary.suggestions.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Merge Conflicts:",
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        for (i, suggestion) in summary.suggestions.iter().enumerate() {
            let is_selected = i == summary.selected_suggestion;
            let prefix = if is_selected {
                format!(" {} ", icons::get(IconId::Selector))
            } else {
                "   ".to_string()
            };
            let style = if is_selected {
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.accent_warning)
            };
            let safe_message = crate::tui::screens::sanitize_for_terminal(&suggestion.message);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{}PR #{} — {}", prefix, suggestion.pr_number, safe_message),
                    style,
                ),
            ]));
            if !suggestion.conflicting_files.is_empty() {
                let files_str = suggestion
                    .conflicting_files
                    .iter()
                    .take(3)
                    .map(|f| crate::tui::screens::sanitize_for_terminal(f))
                    .collect::<Vec<_>>()
                    .join(", ");
                let suffix = if suggestion.conflicting_files.len() > 3 {
                    format!(" (+{})", suggestion.conflicting_files.len() - 3)
                } else {
                    String::new()
                };
                lines.push(Line::from(vec![
                    Span::raw("      Files: "),
                    Span::styled(
                        format!("{}{}", files_str, suffix),
                        Style::default().fg(theme.text_muted),
                    ),
                ]));
            }
        }
    }

    lines.push(Line::from(""));

    let failed_gates_mode = summary.has_failed_gates();

    if failed_gates_mode
        && let Some(fl) = summary.first_failed_gates_session()
        && let Some(wt) = fl.worktree_path.as_ref()
    {
        lines.push(Line::from(vec![
            Span::raw("  Worktree retained: "),
            Span::styled(
                wt.display().to_string(),
                Style::default().fg(theme.accent_warning),
            ),
        ]));
        lines.push(Line::from(""));
    }

    let mut keybind_spans = vec![Span::raw("  ")];
    if failed_gates_mode {
        for (k, label) in [
            ("s", "Shell"),
            ("g", "Re-run Gates"),
            ("r", "Resume"),
            ("v", "View Output"),
            ("q", "Close"),
        ] {
            keybind_spans.push(Span::styled(
                format!("[{}]", k),
                Style::default().fg(theme.keybind_key),
            ));
            keybind_spans.push(Span::styled(
                format!(" {}", label),
                Style::default().fg(theme.text_primary),
            ));
            keybind_spans.push(Span::raw("  "));
        }
    } else {
        let km = keymap::mode_keymap(TuiMode::CompletionSummary, None, &[], false);
        if summary.has_needs_review() || summary.has_conflict_suggestions() {
            keybind_spans.push(Span::styled("[f]", Style::default().fg(theme.keybind_key)));
            keybind_spans.push(Span::styled(
                " Fix",
                Style::default().fg(theme.text_primary),
            ));
            keybind_spans.push(Span::raw("  "));
        }
        keybind_spans.extend(keymap::hint_bar_spans(
            km.hints,
            theme.keybind_key,
            theme.text_primary,
        ));
    }

    lines.push(Line::from(keybind_spans));

    let (title, border) = if failed_gates_mode {
        ("Session Failed Gates", theme.accent_warning)
    } else {
        ("Session Complete", theme.accent_success)
    };
    let block = theme
        .styled_block(title, false)
        .border_style(Style::default().fg(border));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, overlay_area);
}

fn draw_queue_execution_overlay(
    f: &mut Frame,
    executor: &crate::work::executor::QueueExecutor,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    use crate::work::executor::{ExecutorPhase, QueueItemState};
    use ratatui::widgets::Clear;

    let overlay_area = help::centered_rect(60, 50, area);
    f.render_widget(Clear, overlay_area);

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    // Queue progress line
    let mut progress_spans = vec![Span::raw("  Queue: ")];
    for item in executor.items() {
        let (symbol, color) = match item.state {
            QueueItemState::Succeeded => (icons::get(IconId::CheckCircle), theme.accent_success),
            QueueItemState::Running => (icons::get(IconId::DotFill), theme.accent_info),
            QueueItemState::Failed => (icons::get(IconId::XCircle), theme.accent_error),
            QueueItemState::Skipped => (icons::get(IconId::Circle), theme.accent_warning),
            QueueItemState::Pending => (icons::get(IconId::Circle), theme.text_muted),
        };
        let label = format!("{} #{}", symbol, item.queued.issue_number);
        progress_spans.push(Span::styled(
            format!("[{}] ", label),
            Style::default().fg(color),
        ));
    }
    lines.push(Line::from(progress_spans));
    lines.push(Line::from(""));

    // Status line
    let status_text = match executor.phase() {
        ExecutorPhase::Idle => "Preparing next session...".to_string(),
        ExecutorPhase::Running { current_index } => {
            let issue_num = executor.items()[current_index].queued.issue_number;
            format!(
                "Running #{} ({}/{})",
                issue_num,
                current_index + 1,
                executor.total_count()
            )
        }
        ExecutorPhase::AwaitingDecision { failed_index } => {
            let issue_num = executor.items()[failed_index].queued.issue_number;
            format!("#{} failed — choose action", issue_num)
        }
        ExecutorPhase::Finished => format!(
            "Queue complete: {}/{} done",
            executor.completed_count(),
            executor.total_count()
        ),
    };
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(status_text, Style::default().fg(theme.accent_info)),
    ]));

    lines.push(Line::from(""));

    // Keybind bar. Conditional r/s/a are kept hand-rolled; unconditional
    // Esc/Back comes from mode_hints.rs so the overlay label cannot drift
    // from the handler or from the top status bar hint.
    let mut keybind_spans = vec![Span::raw("  ")];
    if matches!(executor.phase(), ExecutorPhase::AwaitingDecision { .. }) {
        keybind_spans.extend([
            Span::styled("[r]", Style::default().fg(theme.keybind_key)),
            Span::styled(" Retry", Style::default().fg(theme.text_primary)),
            Span::raw("  "),
            Span::styled("[s]", Style::default().fg(theme.keybind_key)),
            Span::styled(" Skip", Style::default().fg(theme.text_primary)),
            Span::raw("  "),
            Span::styled("[a]", Style::default().fg(theme.keybind_key)),
            Span::styled(" Abort", Style::default().fg(theme.text_primary)),
            Span::raw("  "),
        ]);
    }
    let km = keymap::mode_keymap(TuiMode::QueueExecution, None, &[], false);
    keybind_spans.extend(keymap::hint_bar_spans(
        km.hints,
        theme.keybind_key,
        theme.text_primary,
    ));
    lines.push(Line::from(keybind_spans));

    let title = format!(
        " Queue: {}/{} ",
        executor.completed_count(),
        executor.total_count()
    );
    let block = theme
        .styled_block(&title, false)
        .border_style(Style::default().fg(theme.accent_info));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, overlay_area);
}

fn draw_continuous_pause_overlay(
    f: &mut Frame,
    state: &ContinuousModeState,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    use ratatui::widgets::Clear;

    let overlay_area = help::centered_rect(60, 50, area);
    f.render_widget(Clear, overlay_area);

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    if let Some(failure) = state.current_failure() {
        lines.push(Line::from(vec![
            Span::raw("  Issue: "),
            Span::styled(
                format!("#{}", failure.issue_number),
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(&failure.issue_title, Style::default().fg(theme.accent_info)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  Error: "),
            Span::styled(
                truncate_str(&failure.error_summary, 80),
                Style::default().fg(theme.accent_error),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  Progress: "),
        Span::styled(
            format!("{} completed", state.completed_count),
            Style::default().fg(theme.accent_success),
        ),
        Span::raw(", "),
        Span::styled(
            format!("{} skipped", state.skipped_count),
            Style::default().fg(theme.accent_warning),
        ),
        Span::raw(", "),
        Span::styled(
            format!("{} failed", state.failures.len()),
            Style::default().fg(theme.accent_error),
        ),
    ]));

    lines.push(Line::from(""));
    let km = keymap::mode_keymap(TuiMode::ContinuousPause, None, &[], false);
    let mut keybind_spans = vec![Span::raw("  ")];
    keybind_spans.extend(keymap::hint_bar_spans(
        km.hints,
        theme.keybind_key,
        theme.text_primary,
    ));
    lines.push(Line::from(keybind_spans));

    let block = theme
        .styled_block("Session Failed — Continuous Mode", false)
        .border_style(Style::default().fg(theme.accent_error));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, overlay_area);
}

fn draw_upgrade_banner(
    f: &mut Frame,
    state: &crate::updater::UpgradeState,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    use crate::updater::UpgradeState;

    let spans = match state {
        UpgradeState::Hidden => return,
        // #1257: an available update is the status bar chip, not a banner.
        UpgradeState::Available(_) => return,
        UpgradeState::Downloading { version } => vec![
            Span::styled(
                " DOWNLOADING ",
                Style::default()
                    .fg(theme.branding_fg)
                    .bg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("Downloading v{}...", version),
                Style::default().fg(theme.accent_warning),
            ),
        ],
        UpgradeState::ReadyToRestart { version, .. } => vec![
            Span::styled(
                " READY ",
                Style::default()
                    .fg(theme.branding_fg)
                    .bg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("v{} installed!", version),
                Style::default().fg(theme.accent_success),
            ),
            Span::raw("  Restart now? "),
            Span::styled("[y]es", Style::default().fg(theme.text_secondary)),
            Span::raw("  "),
            Span::styled("[n]o", Style::default().fg(theme.text_secondary)),
        ],
        UpgradeState::Failed(msg) => vec![
            Span::styled(
                " ERROR ",
                Style::default()
                    .fg(theme.branding_fg)
                    .bg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("Upgrade failed: {}", msg),
                Style::default().fg(theme.accent_error),
            ),
            Span::raw("  "),
            Span::styled("[Esc] dismiss", Style::default().fg(theme.text_secondary)),
        ],
    };

    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), area);
}

pub(crate) fn truncate_str(s: &str, max_len: usize) -> std::borrow::Cow<'_, str> {
    if s.chars().count() <= max_len {
        std::borrow::Cow::Borrowed(s)
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        std::borrow::Cow::Owned(format!("{}...", truncated))
    }
}

/// Pick between live (`MinimaxQuotaSnapshots`) and placeholder
/// (`NoQuotaSnapshots`) quota source based on what the App holds (#769).
fn draw_token_dashboard_with_app_quota(
    f: &mut Frame,
    app: &App,
    sessions: &[&crate::session::types::Session],
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    let badges = app.quota_badges();
    match &app.minimax_quota {
        Some(quota) => {
            let snapshots =
                crate::budget::quota_snapshot::MinimaxQuotaSnapshots::new(quota.clone());
            crate::tui::token_dashboard::draw_token_dashboard_with_quota_and_badges(
                f,
                sessions,
                app.total_cost,
                &snapshots,
                &badges,
                area,
                theme,
            );
        }
        None => {
            crate::tui::token_dashboard::draw_token_dashboard_with_badges(
                f,
                sessions,
                app.total_cost,
                &badges,
                area,
                theme,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_panel_gate_sprite_style_requires_room_for_full_canvas() {
        // Sprite panel hides unless the log area is ≥ 44×18 so the mascot can
        // render at its full 32×16 aspect-preserved canvas (landing-splash size).
        let too_short = Rect::new(0, 0, 60, 12);
        assert!(
            !should_show_dashboard_mascot_panel(too_short, true, false, MascotStyle::Sprite),
            "Sprite should hide when the log area is shorter than 18 rows"
        );
        let too_narrow = Rect::new(0, 0, 40, 20);
        assert!(
            !should_show_dashboard_mascot_panel(too_narrow, true, false, MascotStyle::Sprite),
            "Sprite should hide when the log area is narrower than 44 columns"
        );
        let big_enough = Rect::new(0, 0, 60, 20);
        assert!(should_show_dashboard_mascot_panel(
            big_enough,
            true,
            false,
            MascotStyle::Sprite
        ));
    }

    #[test]
    fn dashboard_panel_gate_respects_show_mascot_false() {
        let area = Rect::new(0, 0, 80, 40);
        assert!(!should_show_dashboard_mascot_panel(
            area,
            false,
            false,
            MascotStyle::Sprite
        ));
    }

    #[test]
    fn dashboard_panel_gate_skips_on_dashboard_mode() {
        let area = Rect::new(0, 0, 80, 40);
        assert!(!should_show_dashboard_mascot_panel(
            area,
            true,
            true,
            MascotStyle::Sprite
        ));
    }

    #[test]
    fn dashboard_panel_gate_ascii_passes_at_current_threshold() {
        let area = Rect::new(0, 0, 25, 8);
        assert!(should_show_dashboard_mascot_panel(
            area,
            true,
            false,
            MascotStyle::Ascii
        ));
    }
}
