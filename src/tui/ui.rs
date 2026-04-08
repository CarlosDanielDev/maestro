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
    widgets::{Block, Borders, Paragraph},
};

/// Render the entire TUI.
pub fn draw(f: &mut Frame, app: &mut App) {
    // Advance spinner animation on each draw cycle
    app.spinner_tick = app.spinner_tick.wrapping_add(1);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // status bar
            Constraint::Min(10),    // main content
            Constraint::Length(10), // activity log
            Constraint::Length(1),  // help bar
        ])
        .split(f.area());

    draw_status_bar(f, app, chunks[0]);

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
                &app.theme,
                spinner_tick,
            );
        }
        TuiMode::Detail(idx) => {
            let sessions = app.pool.all_sessions();
            if let Some(session) = sessions.get(idx) {
                detail::draw_detail_with_claims(
                    f,
                    session,
                    &app.progress_tracker,
                    Some(&app.pool.file_claims),
                    chunks[1],
                    &app.theme,
                );
            } else {
                app.panel_view.draw_with_claims(
                    f,
                    &sessions,
                    Some(&app.pool.file_claims),
                    chunks[1],
                    &app.theme,
                    spinner_tick,
                );
            }
        }
        TuiMode::DependencyGraph => {
            dep_graph::draw_dep_graph(f, app.work_assigner.as_ref(), chunks[1], &app.theme);
        }
        TuiMode::Fullscreen(idx) => {
            let sessions = app.pool.all_sessions();
            if let Some(session) = sessions.get(idx) {
                fullscreen::draw_fullscreen(
                    f,
                    session,
                    &app.progress_tracker,
                    chunks[1],
                    &app.theme,
                    spinner_tick,
                );
            } else {
                app.panel_view
                    .draw(f, &sessions, chunks[1], &app.theme, spinner_tick);
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
                &app.theme,
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
        TuiMode::CompletionSummary => {
            // Draw overview underneath as backdrop
            let sessions = app.pool.all_sessions();
            app.panel_view.draw_with_claims(
                f,
                &sessions,
                Some(&app.pool.file_claims),
                chunks[1],
                &app.theme,
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
                &app.theme,
                spinner_tick,
            );
            if let Some(ref cont) = app.continuous_mode {
                draw_continuous_pause_overlay(f, cont, chunks[1], &app.theme);
            }
        }
    }

    // Delegate to activity log widget
    app.activity_log.draw(f, chunks[2], &app.theme);

    // Draw notification banner overlay if any
    let banners = app.notifications.active_banners();
    if !banners.is_empty() {
        draw_notification_banner(f, banners[0], chunks[2], &app.theme);
    }

    // Draw upgrade banner if visible
    draw_upgrade_banner(f, &app.upgrade_state, chunks[3], &app.theme);

    draw_help_bar(f, app, chunks[3]);

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
            TuiMode::CompletionSummary => None,
            TuiMode::ContinuousPause => None,
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
            &app.theme,
        );
    }
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
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

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_active))
        .title_alignment(ratatui::layout::Alignment::Center);

    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
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

fn draw_help_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let mode_label = match app.tui_mode {
        TuiMode::Overview => "Overview",
        TuiMode::Detail(_) => "Detail",
        TuiMode::DependencyGraph => "Dependencies",
        TuiMode::Fullscreen(_) => "Fullscreen",
        TuiMode::CostDashboard => "Costs",
        TuiMode::Dashboard => "Dashboard",
        TuiMode::IssueBrowser => "Issues",
        TuiMode::MilestoneView => "Milestones",
        TuiMode::PromptInput => "Prompt",
        TuiMode::CompletionSummary => "Summary",
        TuiMode::ContinuousPause => "Paused",
    };

    let help = Line::from(vec![
        Span::styled(
            format!(" {} ", mode_label),
            Style::default()
                .fg(theme.keybind_label_fg)
                .bg(theme.keybind_label_bg),
        ),
        Span::raw(" "),
        Span::styled("[q]", Style::default().fg(theme.keybind_key)),
        Span::raw("uit "),
        Span::styled("[Tab]", Style::default().fg(theme.keybind_key)),
        Span::raw("mode "),
        Span::styled("[f]", Style::default().fg(theme.keybind_key)),
        Span::raw("ull "),
        Span::styled("[$]", Style::default().fg(theme.keybind_key)),
        Span::raw("cost "),
        Span::styled("[?]", Style::default().fg(theme.keybind_key)),
        Span::raw("help "),
        Span::styled("[Esc]", Style::default().fg(theme.keybind_key)),
        Span::raw("back "),
        Span::styled("[p]", Style::default().fg(theme.keybind_key)),
        Span::raw("ause "),
        Span::styled("[k]", Style::default().fg(theme.keybind_key)),
        Span::raw("ill "),
        Span::styled("[↑↓]", Style::default().fg(theme.keybind_key)),
        Span::raw("scroll"),
    ]);
    f.render_widget(Paragraph::new(help), area);
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
                    format!("✗ {} ", gf.gate),
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

    lines.push(Line::from(""));

    let mut keybind_spans = vec![Span::raw("  ")];

    if summary.has_needs_review() {
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

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Session Complete ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_style(Style::default().fg(theme.accent_success));

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

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Session Failed — Continuous Mode ")
        .title_alignment(ratatui::layout::Alignment::Center)
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
