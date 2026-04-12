use crate::continuous::ContinuousModeState;
use crate::tui::app::{App, TuiMode};
use crate::tui::cost_dashboard;
use crate::tui::dep_graph;
use crate::tui::detail;
use crate::tui::fullscreen;
use crate::tui::help;
use crate::tui::navigation::keymap::KeymapProvider;
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

    let log_height = app
        .config
        .as_ref()
        .map(|c| {
            let pct = c.tui.layout.activity_log_height.clamp(10, 50);
            let total = f.area().height;
            ((total as u32 * pct as u32) / 100).max(4) as u16
        })
        .unwrap_or(10);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),          // status bar
            Constraint::Min(10),            // main content
            Constraint::Length(log_height), // activity log
            Constraint::Length(1),          // info bar
            Constraint::Length(1),          // F-key bar
        ])
        .split(f.area());

    draw_status_bar(f, app, chunks[0]);

    // Use preview theme if active, otherwise base theme
    let theme = app.active_theme().clone();

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
            dep_graph::draw_dep_graph(f, app.work_assigner.as_ref(), chunks[1], &app.theme);
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
    }

    // Conditionally split activity area for CI monitor
    let has_ci_checks = app.gh_auth_ok && !app.ci_check_details.is_empty();
    if has_ci_checks {
        let ci_pr_count = app.ci_check_details.len() as u16;
        let ci_height = (ci_pr_count * 6).min(chunks[2].height / 2).max(4);
        let activity_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(ci_height), Constraint::Min(3)])
            .split(chunks[2]);

        // Render CI monitor(s)
        let ci_area = activity_chunks[0];
        if app.ci_check_details.len() == 1 {
            let (&pr_number, details) = app.ci_check_details.iter().next().unwrap();
            let widget =
                crate::tui::widgets::CiMonitorWidget::new(details, &app.theme).pr_number(pr_number);
            ratatui::widgets::Widget::render(widget, ci_area, f.buffer_mut());
        } else {
            let constraints: Vec<Constraint> = app
                .ci_check_details
                .keys()
                .map(|_| Constraint::Ratio(1, app.ci_check_details.len() as u32))
                .collect();
            let pr_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(constraints)
                .split(ci_area);
            for (i, (&pr_number, details)) in app.ci_check_details.iter().enumerate() {
                let widget = crate::tui::widgets::CiMonitorWidget::new(details, &app.theme)
                    .pr_number(pr_number)
                    .max_visible_rows(3);
                ratatui::widgets::Widget::render(widget, pr_chunks[i], f.buffer_mut());
            }
        }
        app.activity_log.draw(f, activity_chunks[1], &app.theme);
    } else {
        app.activity_log.draw(f, chunks[2], &app.theme);
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

    // Draw info bar (#218)
    draw_info_bar(f, app, chunks[3]);

    // Draw upgrade banner if visible (overlays info bar)
    draw_upgrade_banner(f, &app.upgrade_state, chunks[3], &app.theme);

    // Draw F-key bar (#218)
    draw_fkey_bar(f, app, chunks[4]);

    // Draw help overlay on top of everything if active
    if app.show_help {
        use crate::tui::navigation::InputMode;
        let (screen_bindings, input_mode) = match app.tui_mode {
            TuiMode::Dashboard => app
                .home_screen
                .as_ref()
                .map(|s| (s.keybindings(), s.desired_input_mode())),
            TuiMode::IssueBrowser => app
                .issue_browser_screen
                .as_ref()
                .map(|s| (s.keybindings(), s.desired_input_mode())),
            TuiMode::MilestoneView => app
                .milestone_screen
                .as_ref()
                .map(|s| (s.keybindings(), s.desired_input_mode())),
            TuiMode::PromptInput => app
                .prompt_input_screen
                .as_ref()
                .map(|s| (s.keybindings(), s.desired_input_mode())),
            TuiMode::QueueConfirmation => app
                .queue_confirmation_screen
                .as_ref()
                .map(|s| (s.keybindings(), s.desired_input_mode())),
            TuiMode::QueueExecution => None,
            TuiMode::CompletionSummary => None,
            TuiMode::ContinuousPause => None,
            TuiMode::Sanitize => app
                .sanitize_screen
                .as_ref()
                .map(|s| (s.keybindings(), s.desired_input_mode())),
            TuiMode::Settings => app
                .settings_screen
                .as_ref()
                .map(|s| (s.keybindings(), s.desired_input_mode())),
            TuiMode::PrReview => app
                .pr_review_screen
                .as_ref()
                .map(|s| (s.keybindings(), s.desired_input_mode())),
            TuiMode::SessionSwitcher => None,
            _ => None,
        }
        .map(|(b, m)| (b, m.unwrap_or(InputMode::Normal)))
        .unwrap_or_default();
        help::draw_help_overlay(
            f,
            f.area(),
            &screen_bindings,
            input_mode,
            app.help_scroll,
            &theme,
        );
    }
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.active_theme();
    let elapsed = Utc::now() - app.start_time;
    let elapsed_str = format!(
        "{:02}:{:02}:{:02}",
        elapsed.num_hours(),
        elapsed.num_minutes() % 60,
        elapsed.num_seconds() % 60
    );

    let active = app.active_count();
    let total = app.pool.total_count();

    let budget_display = match &app.budget_enforcer {
        Some(enforcer) => format!(" ${:.2}/${:.2} ", app.total_cost, enforcer.total_limit()),
        None => format!(" ${:.2} spent ", app.total_cost),
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

    let mut spans = vec![
        Span::styled(
            concat!(" MAESTRO v", env!("CARGO_PKG_VERSION"), " "),
            Style::default()
                .fg(theme.branding_fg)
                .bg(theme.branding_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                " {} agent{} ({} active) ",
                total,
                if total != 1 { "s" } else { "" },
                active
            ),
            Style::default().fg(theme.accent_info),
        ),
        Span::raw("  "),
        Span::styled(budget_display, Style::default().fg(budget_color)),
        Span::raw("  "),
        Span::styled(
            format!(" {} ", elapsed_str),
            Style::default().fg(theme.text_primary),
        ),
    ];

    if let Some(ref cont) = app.continuous_mode {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(
                " CONTINUOUS: {}/{} done ",
                cont.completed_count,
                cont.total_attempted()
            ),
            Style::default()
                .fg(theme.branding_fg)
                .bg(theme.accent_info)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let text = Line::from(spans);

    let block = theme
        .styled_block_plain(false)
        .border_style(Style::default().fg(theme.border_active));

    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_info_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.active_theme();
    let elapsed = Utc::now() - app.start_time;
    let elapsed_str = format!(
        "{:02}:{:02}:{:02}",
        elapsed.num_hours(),
        elapsed.num_minutes() % 60,
        elapsed.num_seconds() % 60
    );

    let active = app.active_count();
    let total = app.pool.total_count();

    let budget_display = match &app.budget_enforcer {
        Some(enforcer) => format!("${:.2}/${:.2}", app.total_cost, enforcer.total_limit()),
        None => format!("${:.2} spent", app.total_cost),
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
        " \u{2550}\u{2550} ",
        Style::default().fg(theme.border_inactive),
    );

    let spans = vec![
        Span::styled(
            format!(
                " {} agent{} ({} active)",
                total,
                if total != 1 { "s" } else { "" },
                active
            ),
            Style::default().fg(theme.accent_info),
        ),
        sep.clone(),
        Span::styled(budget_display, Style::default().fg(budget_color)),
        sep,
        Span::styled(elapsed_str, Style::default().fg(theme.text_primary)),
    ];

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_fkey_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.active_theme();

    let selected_status = {
        let sessions = app.pool.all_sessions();
        let idx = app.panel_view.selected_index();
        sessions.get(idx).map(|s| s.status)
    };
    let ctx = HelpBarContext::from_status(selected_status);
    let entries = FKeyEntry::build_entries(&ctx);
    let fitted = FKeyEntry::fit_to_width(&entries, area.width);

    let mut spans: Vec<Span> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let (key, label) = match fitted.get(i) {
            Some(f) => f,
            None => break,
        };
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        // Key badge: amber bg, black fg (dimmed if inactive)
        let badge_style = if entry.active {
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
            let label_color = if entry.active {
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
                    format!("\u{f467} {} ", gf.gate),
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
            let prefix = if is_selected { " \u{25b8} " } else { "   " };
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

    let mut keybind_spans = vec![Span::raw("  ")];

    if summary.has_needs_review() || summary.has_conflict_suggestions() {
        keybind_spans.push(Span::styled("[f]", Style::default().fg(theme.keybind_key)));
        keybind_spans.push(Span::raw(" Fix  "));
    }

    keybind_spans.extend([
        Span::styled("[i]", Style::default().fg(theme.keybind_key)),
        Span::raw(" Browse  "),
        Span::styled("[r]", Style::default().fg(theme.keybind_key)),
        Span::raw(" New  "),
        Span::styled("[l]", Style::default().fg(theme.keybind_key)),
        Span::raw(" Logs  "),
        Span::styled("[q]", Style::default().fg(theme.keybind_key)),
        Span::raw(" Quit  "),
        Span::styled("[Esc]", Style::default().fg(theme.keybind_key)),
        Span::raw(" Dashboard"),
    ]);

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
            QueueItemState::Succeeded => ("\u{f42e}", theme.accent_success),
            QueueItemState::Running => ("\u{f444}", theme.accent_info),
            QueueItemState::Failed => ("\u{f467}", theme.accent_error),
            QueueItemState::Skipped => ("\u{f4a3}", theme.accent_warning),
            QueueItemState::Pending => ("\u{f4a3}", theme.text_muted),
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

    // Keybind bar
    let mut keybind_spans = vec![Span::raw("  ")];
    if matches!(executor.phase(), ExecutorPhase::AwaitingDecision { .. }) {
        keybind_spans.extend([
            Span::styled("[r]", Style::default().fg(theme.keybind_key)),
            Span::raw(" Retry  "),
            Span::styled("[s]", Style::default().fg(theme.keybind_key)),
            Span::raw(" Skip  "),
            Span::styled("[a]", Style::default().fg(theme.keybind_key)),
            Span::raw(" Abort  "),
        ]);
    }
    keybind_spans.extend([
        Span::styled("[Esc]", Style::default().fg(theme.keybind_key)),
        Span::raw(" View logs  "),
        Span::styled("[q]", Style::default().fg(theme.keybind_key)),
        Span::raw(" Quit"),
    ]);
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
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("[s]", Style::default().fg(theme.keybind_key)),
        Span::raw(" Skip  "),
        Span::styled("[r]", Style::default().fg(theme.keybind_key)),
        Span::raw(" Retry  "),
        Span::styled("[q]", Style::default().fg(theme.keybind_key)),
        Span::raw(" Stop"),
    ]));

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

fn truncate_str(s: &str, max_len: usize) -> std::borrow::Cow<'_, str> {
    if s.chars().count() <= max_len {
        std::borrow::Cow::Borrowed(s)
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        std::borrow::Cow::Owned(format!("{}...", truncated))
    }
}

/// F-key entry for the DOS-style function key bar (#218).
struct FKeyEntry {
    key: &'static str,
    label: &'static str,
    active: bool,
}

impl FKeyEntry {
    /// Build the full list of F-key entries with context-aware dimming.
    fn build_entries(ctx: &HelpBarContext) -> Vec<FKeyEntry> {
        vec![
            FKeyEntry {
                key: "F1",
                label: "Help",
                active: true,
            },
            FKeyEntry {
                key: "F2",
                label: "Summary",
                active: true,
            },
            FKeyEntry {
                key: "F3",
                label: "Full",
                active: ctx.full_active,
            },
            FKeyEntry {
                key: "F4",
                label: "Costs",
                active: true,
            },
            FKeyEntry {
                key: "F5",
                label: "Tokens",
                active: true,
            },
            FKeyEntry {
                key: "F6",
                label: "Deps",
                active: true,
            },
            FKeyEntry {
                key: "F9",
                label: "Pause",
                active: ctx.pause_active,
            },
            FKeyEntry {
                key: "F10",
                label: "Kill",
                active: ctx.kill_active,
            },
            FKeyEntry {
                key: "Alt-X",
                label: "Exit",
                active: true,
            },
        ]
    }

    /// Filter entries to fit within the given terminal width.
    /// Returns entries that fit; if width < 40, labels are dropped.
    fn fit_to_width(entries: &[FKeyEntry], width: u16) -> Vec<(&str, Option<&str>)> {
        let mut result = Vec::new();
        let mut used = 0u16;

        for entry in entries {
            let entry_width = if width < 40 {
                // Badge-only mode: key + 1 space gap
                entry.key.len() as u16 + 1
            } else {
                // Full mode: key + space + label + 2-space gap
                entry.key.len() as u16 + 1 + entry.label.len() as u16 + 2
            };

            if used + entry_width > width {
                break;
            }

            let label = if width < 40 { None } else { Some(entry.label) };
            result.push((entry.key, label));
            used += entry_width;
        }

        result
    }
}

#[cfg_attr(not(test), allow(dead_code))]
struct HelpBarContext {
    kill_active: bool,
    dismiss_active: bool,
    pause_active: bool,
    logs_active: bool,
    full_active: bool,
}

impl HelpBarContext {
    fn from_status(status: Option<crate::session::types::SessionStatus>) -> Self {
        use crate::session::types::SessionStatus;

        let has_session = status.is_some();
        let is_terminal = status.is_some_and(|s| s.is_terminal());
        let is_running = matches!(status, Some(SessionStatus::Running));

        Self {
            kill_active: has_session && !is_terminal,
            dismiss_active: has_session && is_terminal,
            pause_active: is_running,
            logs_active: has_session,
            full_active: has_session,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionStatus;

    #[test]
    fn help_bar_context_none_status_all_inactive() {
        let ctx = HelpBarContext::from_status(None);
        assert!(!ctx.kill_active);
        assert!(!ctx.dismiss_active);
        assert!(!ctx.pause_active);
        assert!(!ctx.logs_active);
        assert!(!ctx.full_active);
    }

    #[test]
    fn help_bar_context_running_status() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::Running));
        assert!(ctx.kill_active);
        assert!(!ctx.dismiss_active);
        assert!(ctx.pause_active);
        assert!(ctx.logs_active);
        assert!(ctx.full_active);
    }

    #[test]
    fn help_bar_context_completed_status() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::Completed));
        assert!(!ctx.kill_active);
        assert!(ctx.dismiss_active);
        assert!(!ctx.pause_active);
        assert!(ctx.logs_active);
        assert!(ctx.full_active);
    }

    #[test]
    fn help_bar_context_killed_status() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::Killed));
        assert!(!ctx.kill_active);
        assert!(ctx.dismiss_active);
        assert!(!ctx.pause_active);
        assert!(ctx.logs_active);
        assert!(ctx.full_active);
    }

    #[test]
    fn help_bar_context_paused_status() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::Paused));
        assert!(ctx.kill_active);
        assert!(!ctx.dismiss_active);
        assert!(!ctx.pause_active);
        assert!(ctx.logs_active);
        assert!(ctx.full_active);
    }

    #[test]
    fn help_bar_context_spawning_status() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::Spawning));
        assert!(ctx.kill_active);
        assert!(!ctx.dismiss_active);
        assert!(!ctx.pause_active);
        assert!(ctx.logs_active);
        assert!(ctx.full_active);
    }

    #[test]
    fn help_bar_context_queued_status() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::Queued));
        assert!(ctx.kill_active);
        assert!(!ctx.dismiss_active);
        assert!(!ctx.pause_active);
        assert!(ctx.logs_active);
        assert!(ctx.full_active);
    }

    #[test]
    fn help_bar_context_errored_status() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::Errored));
        assert!(ctx.kill_active);
        assert!(!ctx.dismiss_active);
        assert!(!ctx.pause_active);
        assert!(ctx.logs_active);
        assert!(ctx.full_active);
    }

    #[test]
    fn help_bar_context_needs_review_status() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::NeedsReview));
        assert!(!ctx.kill_active);
        assert!(ctx.dismiss_active);
        assert!(!ctx.pause_active);
        assert!(ctx.logs_active);
        assert!(ctx.full_active);
    }

    // --- Issue #218: FKeyEntry tests ---

    #[test]
    fn fkey_entries_builds_all_entries() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::Running));
        let entries = FKeyEntry::build_entries(&ctx);
        assert_eq!(entries.len(), 9);
        assert_eq!(entries[0].key, "F1");
        assert_eq!(entries[0].label, "Help");
        assert_eq!(entries[8].key, "Alt-X");
        assert_eq!(entries[8].label, "Exit");
    }

    #[test]
    fn fkey_entries_full_width_includes_all_labels() {
        let ctx = HelpBarContext::from_status(None);
        let entries = FKeyEntry::build_entries(&ctx);
        let fitted = FKeyEntry::fit_to_width(&entries, 120);
        assert_eq!(fitted.len(), 9);
        for (_, label) in &fitted {
            assert!(
                label.is_some(),
                "all labels should be present at full width"
            );
        }
    }

    #[test]
    fn fkey_entries_narrow_width_truncates() {
        let ctx = HelpBarContext::from_status(None);
        let entries = FKeyEntry::build_entries(&ctx);
        let fitted = FKeyEntry::fit_to_width(&entries, 30);
        // At width 30 in badge-only mode (< 40), should fit several but not all
        assert!(fitted.len() > 0);
        assert!(fitted.len() < 9);
        for (_, label) in &fitted {
            assert!(label.is_none(), "labels should be dropped at narrow width");
        }
    }

    #[test]
    fn fkey_entries_very_narrow_drops_labels() {
        let ctx = HelpBarContext::from_status(None);
        let entries = FKeyEntry::build_entries(&ctx);
        let fitted = FKeyEntry::fit_to_width(&entries, 35);
        for (_, label) in &fitted {
            assert!(label.is_none(), "labels should be None when width < 40");
        }
    }

    #[test]
    fn fkey_context_dimming_matches_help_bar() {
        let ctx = HelpBarContext::from_status(Some(SessionStatus::Running));
        let entries = FKeyEntry::build_entries(&ctx);
        // F3=Full should be active when session is running
        let f3 = entries.iter().find(|e| e.key == "F3").unwrap();
        assert!(f3.active);
        // F9=Pause should be active when running
        let f9 = entries.iter().find(|e| e.key == "F9").unwrap();
        assert!(f9.active);
        // F10=Kill should be active when running
        let f10 = entries.iter().find(|e| e.key == "F10").unwrap();
        assert!(f10.active);
    }

    #[test]
    fn fkey_context_dimming_none_status() {
        let ctx = HelpBarContext::from_status(None);
        let entries = FKeyEntry::build_entries(&ctx);
        let f3 = entries.iter().find(|e| e.key == "F3").unwrap();
        assert!(!f3.active);
        let f9 = entries.iter().find(|e| e.key == "F9").unwrap();
        assert!(!f9.active);
        let f10 = entries.iter().find(|e| e.key == "F10").unwrap();
        assert!(!f10.active);
    }
}
