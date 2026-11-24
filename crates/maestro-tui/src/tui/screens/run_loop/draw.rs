//! Draw the run loop: summary line, Run order, decision card, Run bar.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::view::{Flavour, Phase, Row, RowState, RunView};
use crate::tui::app::App;
use crate::tui::ds::fit::ellipsize_line;
use crate::tui::ds::gauge::gauge_level;
use crate::tui::ds::money::money;
use crate::tui::ds::status_chip::status_chip_for_mode;
use crate::tui::icons::{IconId, get_for_mode};
use crate::tui::screens::milestone_rows::row_line;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;

/// The run loop of the app's run in `area`. No run: nothing drawn.
pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let choice = app.screen_state.run_loop.choice;
    if let Some(view) = RunView::from_app(app, choice) {
        draw_view(f, &view, area, theme, crate::icon_mode::use_nerd_font());
    }
}

/// Pure draw of `view` (snapshot tests pass `nerd` = false).
pub fn draw_view(f: &mut Frame, view: &RunView, area: Rect, theme: &Theme, nerd: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    let summary = summary_line(view, theme, nerd);
    f.render_widget(
        Paragraph::new(ellipsize_line(summary, area.width as usize)),
        chunks[0],
    );
    match view.card.as_ref() {
        Some(card) => {
            let rows = (view.rows.len() as u16 + 2)
                .min(chunks[1].height / 3)
                .max(3);
            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(rows), Constraint::Min(3)])
                .split(chunks[1]);
            draw_order(f, view, body[0], theme, nerd, false);
            card.draw(f, body[1], theme);
        }
        None => draw_order(f, view, chunks[1], theme, nerd, true),
    }
    draw_run_bar(f, view, chunks[2], theme, nerd);
}

fn phase_word(phase: Phase, nerd: bool) -> String {
    match phase {
        Phase::Live => format!("{} live", get_for_mode(IconId::DotFill, nerd)),
        Phase::Held => format!("{} held", get_for_mode(IconId::Pause, nerd)),
        Phase::Stopping => format!("{} stopping", get_for_mode(IconId::Pause, nerd)),
        Phase::Waiting(_) => format!("{} needs you", get_for_mode(IconId::ChipNeedsReview, nerd)),
        Phase::Finished => format!("{} finished", get_for_mode(IconId::CheckCircle, nerd)),
    }
}

/// `Queue · 3 run one at a time · ● live            Esc back · keeps running`
fn summary_line(view: &RunView, theme: &Theme, nerd: bool) -> Line<'static> {
    let what = match view.flavour {
        Flavour::Queue => format!(" Queue · {} run one at a time", view.rows.len()),
        Flavour::Continuous => " Continuous run · one issue at a time".to_string(),
    };
    let color = match view.phase {
        Phase::Live => theme.accent_info,
        Phase::Held | Phase::Stopping | Phase::Waiting(_) => theme.accent_warning,
        Phase::Finished => theme.accent_success,
    };
    let mut spans = vec![
        Span::styled(what, Style::default().fg(theme.text_secondary)),
        Span::raw(" · "),
        Span::styled(
            phase_word(view.phase, nerd),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ];
    // A continuous pause waits on its card: Esc does nothing there.
    if view.flavour == Flavour::Queue {
        spans.push(Span::styled(
            "   Esc back · keeps running",
            Style::default().fg(theme.text_muted),
        ));
    }
    Line::from(spans)
}

fn draw_order(f: &mut Frame, view: &RunView, area: Rect, theme: &Theme, nerd: bool, focus: bool) {
    let title = match view.flavour {
        Flavour::Queue => format!("Run order · {} of {} done", view.done, view.rows.len()),
        Flavour::Continuous => "Run order · stopped here".to_string(),
    };
    let block = theme.styled_block(&title, focus);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = inner.width as usize;
    let mut lines: Vec<Line> = view
        .rows
        .iter()
        .map(|r| row(r, theme, nerd, width))
        .collect();
    if lines.is_empty() {
        let text = match &view.tally {
            Some(tally) => format!("Working on the next ready issue · {tally}"),
            None => "Nothing queued.".to_string(),
        };
        lines.push(Line::from(Span::styled(
            text,
            Style::default().fg(theme.text_muted),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Live state as glyph + word, number, title, note.
fn row(r: &Row, theme: &Theme, nerd: bool, width: usize) -> Line<'static> {
    let word = |icon, word: &str, color| {
        Span::styled(
            format!("{} {word}", get_for_mode(icon, nerd)),
            Style::default().fg(color),
        )
    };
    let state = match r.state {
        RowState::Session(status) => status_chip_for_mode(status, theme, nerd),
        RowState::Next => word(IconId::Circle, "next", theme.text_secondary),
        RowState::NotRun => word(IconId::Circle, "not run", theme.text_muted),
        RowState::Done => word(IconId::CheckCircle, "done", theme.accent_success),
        RowState::Failed => word(IconId::XCircle, "failed", theme.accent_error),
        RowState::Skipped => word(IconId::Skip, "skipped", theme.accent_warning),
        RowState::NeedsYou => word(IconId::ChipNeedsReview, "needs you", theme.accent_warning),
    };
    let left = vec![
        Span::raw("  "),
        Span::styled(
            format!("{:<14}", state.content),
            state.style.add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("#{} ", r.issue),
            Style::default().fg(theme.accent_identifier),
        ),
        Span::styled(
            sanitize_for_terminal(&r.title),
            Style::default().fg(theme.text_primary),
        ),
    ];
    let note = Span::styled(r.note.clone(), Style::default().fg(theme.text_secondary));
    row_line(left, vec![note], width)
}

/// Progress, spend vs the approved worst case, and the queue controls.
fn draw_run_bar(f: &mut Frame, view: &RunView, area: Rect, theme: &Theme, nerd: bool) {
    let muted = Style::default().fg(theme.text_secondary);
    let key = Style::default()
        .fg(theme.keybind_key)
        .add_modifier(Modifier::REVERSED | Modifier::BOLD);
    let text = Style::default().fg(theme.text_primary);
    let progress = match view.flavour {
        Flavour::Queue => format!(
            " {} running · {} not started · ",
            view.running, view.not_started,
        ),
        Flavour::Continuous => format!(" {} · ", view.tally.as_deref().unwrap_or_default()),
    };
    // Spend uses the ds::gauge thresholds against the approved worst case.
    let spend = if view.approved > 0.0 {
        gauge_level(view.spent / view.approved).style(theme)
    } else {
        muted
    };
    let mut first = vec![
        Span::styled(progress, Style::default().fg(theme.accent_info)),
        Span::styled(format!("spent {}", money(Some(view.spent))), spend),
    ];
    if view.flavour == Flavour::Queue && view.cap > 0.0 {
        first.push(Span::styled(
            format!(
                " · worst case {} (approved {})",
                money(Some(view.worst)),
                money(Some(view.approved))
            ),
            text,
        ));
    }
    let second = match (view.flavour, view.phase) {
        (Flavour::Continuous, _) => vec![Span::styled(
            " one at a time · you are asked only when a session fails",
            muted,
        )],
        (Flavour::Queue, Phase::Finished) => vec![Span::styled(
            format!(
                " {} run finished · Esc back",
                get_for_mode(IconId::CheckCircle, nerd)
            ),
            muted,
        )],
        (Flavour::Queue, Phase::Stopping) => vec![Span::styled(
            " stopping · the running session finishes, nothing new starts",
            Style::default().fg(theme.accent_warning),
        )],
        (Flavour::Queue, Phase::Held) => vec![
            Span::styled(" h ", key),
            Span::styled(" resume queue ", text),
            Span::styled(
                " ‖ held · running sessions continue   ",
                Style::default().fg(theme.accent_warning),
            ),
            Span::styled(" S ", key),
            Span::styled(" stop queue…", muted),
        ],
        (Flavour::Queue, _) => vec![
            Span::styled(" h ", key),
            Span::styled(" hold queue", text),
            Span::styled("  start nothing new, running ones finish   ", muted),
            Span::styled(" S ", key),
            Span::styled(" stop queue…", muted),
        ],
    };
    let lines: Vec<Line> = [Line::from(first), Line::from(second)]
        .into_iter()
        .map(|l| ellipsize_line(l, area.width as usize))
        .collect();
    f.render_widget(Paragraph::new(lines), area);
}
