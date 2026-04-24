use crate::continuous::ContinuousModeState;
use crate::mascot::MascotStyle;
use crate::mascot::animator::SystemClock;
use crate::mascot::frames::{MASCOT_ROWS_ASCII, MASCOT_WIDTH_ASCII};
use crate::mascot::widget::MascotWidget;
use crate::tui::app::{App, TuiMode};
use crate::tui::cost_dashboard;
use crate::tui::dep_graph;
use crate::tui::detail;
use crate::tui::fullscreen;
use crate::tui::help;
use crate::tui::icons::{self, IconId};
use crate::tui::navigation::keymap::{self, KeyBindingGroup, ModeKeyMap, fit_fkeys_to_width};
use crate::tui::screens::Screen;
use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Render the entire TUI.
pub fn draw(f: &mut Frame, app: &mut App) {
    // Advance spinner animation on each draw cycle
    app.spinner_tick = app.spinner_tick.wrapping_add(1);

    // Decrement transition flash counters (#202)
    app.pool.tick_flash_counters();

    let clock = SystemClock;
    app.mascot_animator.tick(&clock);
    let derived = crate::mascot::derive_dashboard_mascot_state(app.pool.all_statuses());
    app.mascot_animator.set_state(derived, &clock);

    let log_height = if app.show_activity_log {
        app.config
            .as_ref()
            .map(|c| {
                let pct = c.tui.layout.activity_log_height.clamp(10, 50);
                let total = f.area().height;
                ((total as u32 * pct as u32) / 100).max(4) as u16
            })
            .unwrap_or(10)
    } else {
        0
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),          // status bar (includes inline hints)
            Constraint::Min(10),            // main content
            Constraint::Length(log_height), // activity log
            Constraint::Length(1),          // F-key bar
        ])
        .split(f.area());

    // Use preview theme if active, otherwise base theme
    let theme = app.active_theme().clone();

    let selected_status = {
        let sessions = app.pool.all_sessions();
        let idx = app.panel_view.selected_index();
        sessions.get(idx).map(|s| s.status)
    };
    let cache_key = (app.tui_mode, selected_status);
    if app.cached_mode_km.is_none() || app.cached_mode_km_key != cache_key {
        let screen_bindings = active_screen_bindings(app);
        app.cached_mode_km = Some(keymap::mode_keymap(
            app.tui_mode,
            selected_status,
            &screen_bindings,
        ));
        app.cached_mode_km_key = cache_key;
    }
    let mode_km = app.cached_mode_km.as_ref().unwrap().clone();

    draw_status_bar(f, app, mode_km.clone(), chunks[0]);

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
            if let Some(ref mut screen) = app.home_screen {
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
            if let Some(ref mut screen) = app.landing_screen {
                screen.set_mascot(
                    app.mascot_animator.state(),
                    app.mascot_animator.frame_index(),
                    app.mascot_style,
                );
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::IssueWizard => {
            if let Some(ref mut screen) = app.issue_wizard_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::ProjectStats => {
            if let Some(ref mut screen) = app.project_stats_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::MilestoneWizard => {
            if let Some(ref mut screen) = app.milestone_wizard_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::IssueBrowser => {
            if let Some(ref mut screen) = app.issue_browser_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::MilestoneView => {
            if let Some(ref mut screen) = app.milestone_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::PromptInput => {
            if let Some(ref mut screen) = app.prompt_input_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::QueueConfirmation => {
            if let Some(ref mut screen) = app.queue_confirmation_screen {
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
            crate::tui::token_dashboard::draw_token_dashboard(
                f,
                &sessions,
                app.total_cost,
                chunks[1],
                &theme,
            );
        }
        TuiMode::Sanitize => {
            if let Some(ref mut screen) = app.sanitize_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
        TuiMode::Settings => {
            if let Some(ref mut screen) = app.settings_screen {
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
            if let Some(ref sw) = app.session_switcher {
                let sessions = app.pool.all_sessions();
                sw.draw(f, chunks[1], &sessions, &theme);
            }
        }
        TuiMode::AdaptWizard => {
            if let Some(ref mut screen) = app.adapt_screen {
                screen.tick();
                screen.draw(f, chunks[1], &theme);
            }
        }
        TuiMode::PrReview => {
            if let Some(ref mut screen) = app.pr_review_screen {
                screen.tick();
                screen.draw(f, chunks[1], &theme);
            }
        }
        TuiMode::ReleaseNotes => {
            if let Some(ref mut screen) = app.release_notes_screen {
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
                    if let Some(ref mut screen) = app.home_screen {
                        screen.draw(f, chunks[1], &theme);
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
                    crate::tui::token_dashboard::draw_token_dashboard(
                        f,
                        &sessions,
                        app.total_cost,
                        chunks[1],
                        &theme,
                    );
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
            if let Some(ref mut screen) = app.hollow_retry_screen {
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
            if let Some(ref mut screen) = app.adapt_follow_up_screen {
                screen.draw(f, chunks[1], &app.theme);
            }
        }
    }

    // Only render activity log area when visible
    if app.show_activity_log {
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
            app.activity_log.draw(f, activity_chunks[1], &app.theme);
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
                app.activity_log.draw(f, h_split[0], &app.theme);
                draw_mascot_block(
                    f,
                    app.mascot_animator.state(),
                    app.mascot_animator.frame_index(),
                    app.mascot_style,
                    h_split[1],
                    &theme,
                );
            } else {
                app.activity_log.draw(f, log_area, &app.theme);
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

    // Draw upgrade banner if visible (overlays status bar)
    draw_upgrade_banner(f, &app.upgrade_state, chunks[0], &app.theme);

    draw_fkey_bar(f, &mode_km, chunks[3], &theme);

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
pub(super) fn active_screen(app: &App) -> Option<&dyn Screen> {
    match app.tui_mode {
        TuiMode::Dashboard => app.home_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::Landing => app.landing_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::IssueBrowser => app.issue_browser_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::IssueWizard => app.issue_wizard_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::MilestoneView => app.milestone_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::MilestoneWizard => app
            .milestone_wizard_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::ProjectStats => app.project_stats_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::PromptInput => app.prompt_input_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::QueueConfirmation => app
            .queue_confirmation_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::Sanitize => app.sanitize_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::Settings => app.settings_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::PrReview => app.pr_review_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::HollowRetry => app.hollow_retry_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::AdaptFollowUp => app
            .adapt_follow_up_screen
            .as_ref()
            .map(|s| s as &dyn Screen),
        TuiMode::AdaptWizard => app.adapt_screen.as_ref().map(|s| s as &dyn Screen),
        TuiMode::ReleaseNotes => app.release_notes_screen.as_ref().map(|s| s as &dyn Screen),
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

fn draw_status_bar(f: &mut Frame, app: &mut App, mode_km: ModeKeyMap, area: Rect) {
    let theme = app.active_theme().clone();
    draw_status_bar_inner(f, app, &theme, &mode_km, area);
}

fn draw_status_bar_inner(
    f: &mut Frame,
    app: &mut App,
    theme: &crate::tui::theme::Theme,
    mode_km: &ModeKeyMap,
    area: Rect,
) {
    let elapsed = Utc::now() - app.start_time;
    let elapsed_str = format!(
        "{:02}:{:02}:{:02}",
        elapsed.num_hours(),
        elapsed.num_minutes() % 60,
        elapsed.num_seconds() % 60
    );

    let active = app.active_count();
    let total = app.pool.total_count();

    let cost = app.total_cost + 0.0; // normalize -0.0 → 0.0
    let budget_display = match &app.budget_enforcer {
        Some(enforcer) => format!("{cost:.2}/{:.2}", enforcer.total_limit()),
        None => format!("{cost:.2} spent"),
    };

    let budget_color = match &app.budget_enforcer {
        Some(enforcer) => {
            let pct = if enforcer.total_limit() > 0.0 {
                ((app.total_cost / enforcer.total_limit()) * 100.0) as u8
            } else {
                0
            };
            theme.budget_color(pct)
        }
        None => theme.accent_warning,
    };

    let sep = Span::styled(
        format!(" {} ", icons::get(IconId::SeparatorH)),
        Style::default().fg(theme.border_inactive),
    );

    let mut spans = vec![
        Span::styled(
            concat!(" MAESTRO v", env!("CARGO_PKG_VERSION"), " "),
            Style::default()
                .fg(theme.branding_fg)
                .bg(theme.branding_bg)
                .add_modifier(Modifier::BOLD),
        ),
        sep.clone(),
    ];

    // Breadcrumb trail
    {
        let crumb_sep = format!(" {} ", icons::get(IconId::ChevronRight));
        for mode in app.nav_stack.breadcrumbs() {
            spans.push(Span::styled(
                mode.breadcrumb_label(),
                Style::default().fg(theme.text_muted),
            ));
            spans.push(Span::styled(
                crumb_sep.clone(),
                Style::default().fg(theme.border_inactive),
            ));
        }
        spans.push(Span::styled(
            app.tui_mode.breadcrumb_label(),
            Style::default()
                .fg(theme.accent_info)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(sep.clone());
    }

    spans.push(Span::styled(
        format!(
            "{} {} agent{} ({} active)",
            icons::get(IconId::Agents),
            total,
            if total != 1 { "s" } else { "" },
            active
        ),
        Style::default().fg(theme.accent_info),
    ));
    spans.push(sep.clone());
    spans.push(Span::styled(
        format!("{}{}", icons::get(IconId::Cost), budget_display),
        Style::default().fg(budget_color),
    ));
    spans.push(sep.clone());
    spans.push(Span::styled(
        format!("{} {}", icons::get(IconId::Clock), elapsed_str),
        Style::default().fg(theme.text_primary),
    ));

    // RAM widget
    {
        let snap = app.resource_monitor.snapshot();
        let rss_mb = snap.rss_mb();
        let ram_color = {
            let pct = snap.memory_pct();
            if pct > 80.0 {
                theme.accent_error
            } else if pct > 50.0 {
                theme.accent_warning
            } else {
                theme.accent_success
            }
        };
        let ram_text = if let Some(delta) = snap.tq_delta_pct() {
            format!("RAM: {:.0}MB (↓{:.0}% TQ)", rss_mb, delta)
        } else {
            format!("RAM: {:.0}MB", rss_mb)
        };
        spans.push(sep.clone());
        spans.push(Span::styled(ram_text, Style::default().fg(ram_color)));
    }

    // TQ badge
    {
        spans.push(sep.clone());
        let tq_enabled = app.flags.is_enabled(crate::flags::Flag::TurboQuant);
        spans.push(if tq_enabled {
            Span::styled(
                "TQ:ON",
                Style::default()
                    .fg(theme.branding_fg)
                    .bg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("TQ:OFF", Style::default().fg(theme.text_secondary))
        });
    }

    if let Some(ref cont) = app.continuous_mode {
        spans.push(sep.clone());
        spans.push(Span::styled(
            format!(
                "CONTINUOUS: {}/{} done",
                cont.completed_count,
                cont.total_attempted()
            ),
            Style::default()
                .fg(theme.branding_fg)
                .bg(theme.accent_info)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Walk the spans once for the hint-fitting check; reuse the result
    // for the marquee fingerprint below (this is hot — runs every frame).
    let inner_width = area.width.saturating_sub(2);
    let status_used: usize = spans.iter().map(|s| s.width()).sum();
    let remaining = inner_width.saturating_sub(status_used as u16);
    if remaining > 10 && !mode_km.hints.is_empty() {
        let fitted = keymap::fit_hints_to_width(mode_km.hints, remaining.saturating_sub(4));
        if !fitted.is_empty() {
            spans.push(sep.clone());
            for (i, (key, action)) in fitted.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw("  "));
                }
                spans.push(Span::styled(
                    format!("[{}]", key),
                    Style::default().fg(theme.accent_success),
                ));
                spans.push(Span::styled(
                    format!(" {}", action),
                    Style::default().fg(theme.text_secondary),
                ));
                // Account for the appended hint width incrementally so we
                // don't re-walk the whole vec for the marquee check.
            }
        }
    }

    let block = theme
        .styled_block_plain(false)
        .border_style(Style::default().fg(theme.border_active));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Recompute total_width because hint-spans may have been appended.
    let total_width: usize = spans.iter().map(|s| s.width()).sum();
    if total_width != app.status_bar_marquee_fingerprint {
        app.status_bar_marquee.reset();
        app.status_bar_marquee_fingerprint = total_width;
    }

    let viewport_width = inner_area.width as usize;
    if total_width <= viewport_width {
        app.status_bar_marquee.reset();
        f.render_widget(Paragraph::new(Line::from(spans)), inner_area);
    } else {
        let overflow = total_width.saturating_sub(viewport_width);
        app.status_bar_marquee
            .advance(overflow, &crate::tui::marquee::MarqueeConfig::default());
        let windowed = crate::tui::marquee::visible_spans(
            &spans,
            app.status_bar_marquee.offset,
            viewport_width,
        );
        f.render_widget(Paragraph::new(Line::from(windowed)), inner_area);
    }
}

fn draw_fkey_bar(
    f: &mut Frame,
    mode_km: &ModeKeyMap,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    let fitted = fit_fkeys_to_width(&mode_km.fkeys, area.width);

    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, label, active)) in fitted.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        // Key badge: amber bg, black fg (dimmed if inactive)
        let badge_style = if *active {
            Style::default()
                .fg(theme.fkey_badge_fg)
                .bg(theme.fkey_badge_bg)
        } else {
            Style::default()
                .fg(theme.text_muted)
                .bg(theme.keybind_label_bg)
        };
        spans.push(Span::styled(*key, badge_style));
        // Action label (if present)
        if let Some(lbl) = label {
            let label_color = if *active {
                theme.keybind_label_fg
            } else {
                theme.text_muted
            };
            spans.push(Span::styled(
                format!(" {}", lbl),
                Style::default().fg(label_color),
            ));
        }
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
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

fn draw_confirm_kill_overlay(
    f: &mut Frame,
    session_id: uuid::Uuid,
    app: &App,
    area: Rect,
    theme: &crate::tui::theme::Theme,
) {
    use ratatui::widgets::Clear;
    let popup = help::centered_rect(40, 20, area);
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
    use ratatui::widgets::Clear;
    let popup = help::centered_rect(50, 25, area);
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
        theme.border_focused
    };

    let block = theme
        .styled_block("Confirm Exit", false)
        .border_style(Style::default().fg(border_color));

    f.render_widget(Paragraph::new(lines).block(block), popup);
}

fn draw_completion_overlay(
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

    // Hint bar: single source of truth is `mode_hints.rs`. The conditional
    // `[f] Fix` is prepended here because it only applies when the summary
    // actually has suggestions — static hint tables can't encode that.
    let km = keymap::mode_keymap(TuiMode::CompletionSummary, None, &[]);
    let mut keybind_spans = vec![Span::raw("  ")];
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

    lines.push(Line::from(keybind_spans));

    let block = theme
        .styled_block("Session Complete", false)
        .border_style(Style::default().fg(theme.accent_success));

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
    let km = keymap::mode_keymap(TuiMode::QueueExecution, None, &[]);
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
    let km = keymap::mode_keymap(TuiMode::ContinuousPause, None, &[]);
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
        UpgradeState::Available(info) => vec![
            Span::styled(
                " UPDATE ",
                Style::default()
                    .fg(theme.branding_fg)
                    .bg(theme.accent_info)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("New version {} available", info.tag),
                Style::default().fg(theme.accent_info),
            ),
            Span::raw("  "),
            Span::styled("[u]pgrade", Style::default().fg(theme.text_secondary)),
            Span::raw("  "),
            Span::styled("[Esc] dismiss", Style::default().fg(theme.text_secondary)),
        ],
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
