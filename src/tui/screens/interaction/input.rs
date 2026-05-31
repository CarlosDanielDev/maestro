//! Input pane, keybind footer, and overlays for the Interaction screen.
//!
//! Renders the multi-line `tui-textarea` editor (locked greyed-out while a
//! turn streams, #738), a one-line keybind footer with the `Ctrl+P` chord
//! greyed when `produce_pr` was unchecked, the quit-confirm modal, and the
//! terminated banner. The editor owns the text buffer and cursor logic.

use crate::session::interaction::CloseReason;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use std::path::Path;
use tui_textarea::TextArea;

/// Render a one-line header naming the agent/CLI, model, and issue the chat
/// is bound to, so the user always knows who they are talking to (#738 QA).
pub(super) fn draw_header(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    agent_label: &str,
    model: &str,
    issue_number: u64,
) {
    let agent = if agent_label.is_empty() {
        "agent"
    } else {
        agent_label
    };
    let model = if model.is_empty() { "default" } else { model };
    let spans = vec![
        Span::styled(" agent ", Style::default().fg(theme.text_secondary)),
        Span::styled(agent.to_string(), Style::default().fg(theme.accent_info)),
        Span::styled("  ·  model ", Style::default().fg(theme.text_secondary)),
        Span::styled(model.to_string(), Style::default().fg(theme.accent_info)),
        Span::styled(
            format!("  ·  issue #{issue_number}"),
            Style::default().fg(theme.text_secondary),
        ),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the input editor into `area`. While `locked` (a turn is streaming)
/// the title flags the lock and the placeholder reflects it.
pub(super) fn draw_input(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    editor: &TextArea<'static>,
    locked: bool,
    spinner: char,
) {
    let title = if locked {
        format!("Message ({spinner} agent responding…)")
    } else {
        "Message".to_string()
    };
    let block = theme.styled_block(&title, true);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = editor.lines();
    let is_empty = lines.len() == 1 && lines[0].is_empty();
    let paragraph = if locked {
        Paragraph::new(Line::from(Span::styled(
            format!("{spinner} Agent is responding — input locked…"),
            Style::default().fg(theme.accent_warning),
        )))
    } else if is_empty {
        Paragraph::new(Line::from(Span::styled(
            "Type a prompt to begin…",
            Style::default().fg(theme.text_secondary),
        )))
    } else {
        let rendered: Vec<Line> = lines
            .iter()
            .map(|l| {
                Line::from(Span::styled(
                    l.clone(),
                    Style::default().fg(theme.text_primary),
                ))
            })
            .collect();
        Paragraph::new(rendered)
    };
    f.render_widget(paragraph, inner);

    // Place the real terminal cursor at the editor caret so the user can see
    // where they are typing. Skipped while locked (input is ignored anyway).
    // `cursor()` is a (row, col) char index; clamp into the inner viewport.
    if !locked && inner.width > 0 && inner.height > 0 {
        let (row, col) = editor.cursor();
        let x = inner.x + (col as u16).min(inner.width - 1);
        let y = inner.y + (row as u16).min(inner.height - 1);
        f.set_cursor_position(Position::new(x, y));
    }
}

/// One-line keybind footer. The `Ctrl+P` chord is greyed when the pushup
/// action is unavailable (launched without Produce PR or mid-stream).
pub(super) fn draw_keybar(f: &mut Frame, area: Rect, theme: &Theme, pushup_enabled: bool) {
    let active = Style::default().fg(theme.accent_success);
    let muted = Style::default().fg(theme.text_secondary);
    let pushup_style = if pushup_enabled { active } else { muted };

    let mut spans = vec![
        Span::styled("[Enter]", active),
        Span::raw(" Send  "),
        Span::styled("[Shift+Enter]", active),
        Span::raw(" Newline  "),
        Span::styled("[Ctrl+P]", pushup_style),
    ];
    if pushup_enabled {
        spans.push(Span::raw(" /pushup  "));
    } else {
        spans.push(Span::styled(" /pushup (off)  ", muted));
    }
    spans.extend([
        Span::styled("[Ctrl+L]", active),
        Span::raw(" Clear  "),
        Span::styled("[Ctrl+Q]", active),
        Span::raw(" Quit  "),
        Span::styled("[Esc]", active),
        Span::raw(" Back"),
    ]);
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the `Ctrl+Q` quit-confirm modal centered over `area`.
pub(super) fn draw_quit_modal(f: &mut Frame, area: Rect, theme: &Theme, worktree: &Path) {
    let text = format!(
        "Quit interaction? Worktree at {} kept for manual inspection. [y/N]",
        worktree.display()
    );
    let modal = centered_rect(area, (text.len() as u16 + 6).min(area.width), 3);
    f.render_widget(Clear, modal);
    let block = theme.styled_block("Confirm", true);
    let inner = block.inner(modal);
    f.render_widget(block, modal);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(theme.text_primary),
        )))
        .alignment(Alignment::Center),
        inner,
    );
}

/// Render the terminated banner in place of the input pane.
pub(super) fn draw_terminated_banner(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    reason: Option<&CloseReason>,
) {
    let detail = match reason {
        Some(CloseReason::UserQuit) => "user quit",
        Some(CloseReason::PrCreated { .. }) => "PR created",
        Some(CloseReason::AgentFailure { .. }) => "agent failure",
        None => "closed",
    };
    let block = theme.styled_block("Terminated", true);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("Session terminated ({detail}). Press any key to return to Issues."),
            Style::default().fg(theme.text_secondary),
        )))
        .alignment(Alignment::Center),
        inner,
    );
}

/// A centered rectangle of `w` x `h` clamped inside `area`.
fn centered_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}
