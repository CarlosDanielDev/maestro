//! Input pane, keybind footer, and overlays for the Interaction screen.
//!
//! Renders the multi-line `ratatui-textarea` editor (locked greyed-out while a
//! turn streams, #738), a one-line keybind footer with the `Ctrl+P` chord
//! greyed when `produce_pr` was unchecked, the quit-confirm modal, and the
//! terminated banner. The editor owns the text buffer and cursor logic.

use super::view_state::CloseReason;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use ratatui_textarea::TextArea;
use unicode_width::UnicodeWidthStr;

/// Render the header naming the agent/CLI, model, and issue the chat is bound
/// to, so the user always knows who they are talking to (#738 QA). The text
/// rides the top border of a square box (#987 QA) so the header reads as one
/// framed unit, distinct from the rounded transcript cards below it. Needs a
/// 2-row `area` (top border + bottom border).
pub(super) fn draw_header(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    agent_label: &str,
    model: &str,
    issue_number: u64,
    issue_title: &str,
) {
    let agent = if agent_label.is_empty() {
        "agent"
    } else {
        agent_label
    };
    let model = if model.is_empty() { "default" } else { model };
    let mut spans = vec![
        Span::styled("─ agent ", Style::default().fg(theme.border_inactive)),
        Span::styled(agent.to_string(), Style::default().fg(theme.accent_info)),
        Span::styled("  ·  model ", Style::default().fg(theme.text_secondary)),
        Span::styled(model.to_string(), Style::default().fg(theme.accent_info)),
        Span::styled(
            format!("  ·  #{issue_number} "),
            Style::default().fg(theme.text_secondary),
        ),
    ];
    if !issue_title.is_empty() {
        spans.push(Span::styled(
            issue_title.to_string(),
            Style::default().fg(theme.text_primary),
        ));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.border_inactive))
        .title(Line::from(spans))
        .title_alignment(Alignment::Left);
    f.render_widget(block, area);
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
    wave: &str,
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
            format!("{wave}  Agent is responding — input locked…"),
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
        let ratatui_textarea::DataCursor(row, col) = editor.cursor();
        let x = inner.x + (col as u16).min(inner.width - 1);
        let y = inner.y + (row as u16).min(inner.height - 1);
        f.set_cursor_position(Position::new(x, y));
    }
}

/// One-line keybind footer rendered as a titled border line (#987 QA): the
/// chords ride a `─` rule like the header box, instead of floating on a blank
/// row. The `Ctrl+P` chord is greyed when the pushup action is unavailable
/// (launched without Produce PR or mid-stream).
pub(super) fn draw_keybar(f: &mut Frame, area: Rect, theme: &Theme, pushup_enabled: bool) {
    let active = Style::default().fg(theme.accent_success);
    let muted = Style::default().fg(theme.text_secondary);
    let border = Style::default().fg(theme.border_inactive);
    let pushup_style = if pushup_enabled { active } else { muted };

    // Lead the chords with a short rule so they sit on a border line.
    let mut spans = vec![
        Span::styled("─ ", border),
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
        Span::styled("[Ctrl+G]", active),
        Span::raw(" Clear input  "),
        Span::styled("[Ctrl+W]", active),
        Span::raw(" Quit  "),
        Span::styled("[Esc]", active),
        Span::raw(" Back  "),
        Span::styled("[Up/Dn/PgUp/PgDn]", active),
        Span::raw(" Scroll  "),
        Span::styled("[End]", active),
        Span::raw(" Latest "),
    ]);

    // Fill the rest of the row with the rule so the chords read as a border.
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let fill = (area.width as usize).saturating_sub(used);
    if fill > 0 {
        spans.push(Span::styled("─".repeat(fill), border));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

impl super::InteractionScreen {
    /// Lines of the `Ctrl+W` quit-confirm modal (#1242). The copy keys off
    /// the same predicate `begin_quit_teardown` uses (a non-empty trusted
    /// `worktree_root`), so it names the wipe exactly when `y` runs it.
    pub(super) fn quit_modal_lines(&self) -> Vec<String> {
        if self.quit_loss_acknowledged {
            // RC4: this session has unpushed work and no linked PR. Quitting
            // deletes the worktree branch — say so plainly and require a
            // deliberate second confirm.
            return vec![
                "Unpushed work will be LOST — no PR was created. Press y again to discard, N to keep working."
                    .to_string(),
            ];
        }
        if self.worktree_root.as_os_str().is_empty() {
            return vec![
                "Quit interaction? No isolated worktree, so nothing is removed.".to_string(),
                "Press y to quit, N to keep working.".to_string(),
            ];
        }
        // The path gets its own line so it wraps alone at narrow widths
        // instead of pushing the branch and the way out off the modal.
        let mut lines = vec![
            "Quit interaction? Removes worktree".to_string(),
            self.worktree_path.display().to_string(),
        ];
        if !self.branch.is_empty() {
            lines.push(format!("and local branch {}.", self.branch));
        }
        lines.push("Unpushed commits are lost; any remote branch or PR stays.".to_string());
        lines.push("Press y to quit, N to keep working.".to_string());
        lines
    }
}

/// Rows `text` takes when greedily word-wrapped to `width` columns. Errs on
/// the tall side for words wider than `width`, so the modal never clips.
fn wrapped_rows(text: &str, width: usize) -> usize {
    let width = width.max(1);
    let (mut rows, mut col) = (1, 0);
    for word in text.split(' ') {
        let w = word.width();
        if col > 0 && col + 1 + w <= width {
            col += 1 + w;
            continue;
        }
        if col > 0 {
            rows += 1;
        }
        rows += w.div_ceil(width).max(1) - 1;
        col = w - (w.div_ceil(width).max(1) - 1) * width;
    }
    rows
}

/// Render the `Ctrl+W` quit-confirm modal centered over `area`. Lines wrap
/// to the modal width, and the height follows the wrapped row count, so
/// every line stays readable down to the 80x24 floor (#1242).
pub(super) fn draw_quit_modal(f: &mut Frame, area: Rect, theme: &Theme, lines: &[String]) {
    let widest = lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16;
    let width = widest.saturating_add(6).min(area.width);
    let wrap_width = usize::from(width.saturating_sub(2));
    let rows: usize = lines.iter().map(|l| wrapped_rows(l, wrap_width)).sum();
    let modal = centered_rect(area, width, (rows as u16).saturating_add(2));
    f.render_widget(Clear, modal);
    let block = theme.styled_block("Confirm", true);
    let inner = block.inner(modal);
    f.render_widget(block, modal);
    let style = Style::default().fg(theme.text_primary);
    f.render_widget(
        Paragraph::new(
            lines
                .iter()
                .map(|l| Line::from(Span::styled(l.clone(), style)))
                .collect::<Vec<_>>(),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true }),
        inner,
    );
}

/// Render the one-row status banner shown once the session settles (#950):
/// how the underlying run ended (`settled_from`) plus any linked PR. Sits
/// between the transcript and the keybar; the chat input stays active below.
pub(super) fn draw_settled_banner(f: &mut Frame, area: Rect, theme: &Theme, text: &str) {
    let line = Line::from(vec![
        Span::styled("─ ", Style::default().fg(theme.border_inactive)),
        Span::styled(text.to_string(), Style::default().fg(theme.accent_info)),
        Span::raw(" "),
    ]);
    let used: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    let fill = (area.width as usize).saturating_sub(used);
    let mut spans = line.spans;
    if fill > 0 {
        spans.push(Span::styled(
            "─".repeat(fill),
            Style::default().fg(theme.border_inactive),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
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

/// Render the async-teardown in-flight banner in place of the input pane
/// (#941). The wipe runs off-thread; this keeps the wait visible (and the UI
/// responsive) instead of freezing the frame.
pub(super) fn draw_teardown_banner(f: &mut Frame, area: Rect, theme: &Theme, spinner_tick: usize) {
    let nerd = crate::icon_mode::use_nerd_font();
    let spinner = crate::tui::spinner::graph_node_frame(spinner_tick / 3, nerd);
    let block = theme.styled_block("Closing", true);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{spinner} quitting — wiping worktree…"),
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
