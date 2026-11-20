//! Command palette (design §04 `palette`, §06 ⌃K, #1255): a fuzzy list
//! over the keymap registry plus the palette-only commands, drawn on
//! `surface_overlay` over a dimmed screen.

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use super::bar_row::key_style;
use super::panel::{Focus, panel_plain};
use crate::tui::app::TuiMode;
use crate::tui::navigation::registry::{
    Action, Binding, Chord, Danger, PALETTE_COMMANDS, PaletteCommand, actions_on,
};
use crate::tui::theme::Theme;
use crate::tui::widgets::input::{self, Input, InputConfig};

/// The box, as in the design mock (§04 `palette`).
const WIDTH: u16 = 64;
const HEIGHT: u16 = 12;
/// Rows above the box.
const TOP: u16 = 4;
/// Query row, separator, footer.
const CHROME_ROWS: u16 = 3;
/// Columns for the key at the right of a row.
const KEY_COLS: usize = 5;

/// What an entry runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// A registry binding: dispatch runs it as if its key was pressed.
    Binding(Action, Chord),
    /// A palette-only command.
    Command(PaletteCommand),
}

/// One palette row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub label: &'static str,
    /// The key that runs it outside the palette (`""` for none).
    pub key: &'static str,
    pub run: Run,
    pub destructive: bool,
    /// Why it cannot run now (shown with `⊘`).
    pub disabled: Option<&'static str>,
}

/// The entries for `mode`: every action [`actions_on`] returns, then the
/// palette-only commands. `disabled` gives the reason an entry cannot run.
pub fn entries(
    bindings: &[Binding],
    mode: TuiMode,
    disabled: impl Fn(Run) -> Option<&'static str>,
) -> Vec<Entry> {
    let keyed = actions_on(bindings, mode).into_iter().map(|b| {
        let run = Run::Binding(b.action, b.chord);
        Entry {
            label: b.label,
            key: b.chord.label(),
            run,
            destructive: b.danger == Danger::Destructive,
            disabled: disabled(run),
        }
    });
    let commands = PALETTE_COMMANDS.iter().map(|&(label, command, key)| {
        let run = Run::Command(command);
        Entry {
            label,
            key: match key {
                Some((key, on)) if on == mode => key,
                _ => "",
            },
            run,
            // Retry PR writes a PR, like its Overview key `P` (#1265).
            destructive: command == PaletteCommand::RetryPr,
            disabled: disabled(run),
        }
    });
    keyed.chain(commands).collect()
}

/// Char indices of `query` in `label` as a case-insensitive subsequence;
/// `None` when it does not match. An empty query matches with no hits.
pub fn fuzzy_hits(query: &str, label: &str) -> Option<Vec<usize>> {
    let mut hits = Vec::new();
    let mut chars = label.chars().enumerate();
    for q in query.chars() {
        let q = q.to_lowercase().next()?;
        let (i, _) = chars.find(|(_, c)| c.to_lowercase().next() == Some(q))?;
        hits.push(i);
    }
    Some(hits)
}

/// What a key did to the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Still open.
    Open,
    /// Esc: closed, nothing runs.
    Close,
    /// ⏎ on an enabled entry: closed, this runs.
    Run(Run),
}

/// The open palette: its query field and the selected match.
pub struct PaletteState {
    query: Input,
    selected: usize,
    entries: Vec<Entry>,
}

impl PaletteState {
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            query: Input::single_line(InputConfig::new().placeholder("type a command")),
            selected: 0,
            entries,
        }
    }

    /// The entries matching the query, with their hit indices.
    pub fn matches(&self) -> Vec<(&Entry, Vec<usize>)> {
        let query = self.query.text();
        let query = query.trim();
        (self.entries.iter())
            .filter_map(|e| fuzzy_hits(query, e.label).map(|hits| (e, hits)))
            .collect()
    }

    /// Esc closes, ⏎ runs the selected enabled entry, ↑↓ pick; every other
    /// key edits the query (through `widgets::input::feed`).
    pub fn handle_key(&mut self, key: &KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Enter => {
                let matches = self.matches();
                return match matches.get(self.selected) {
                    Some((e, _)) if e.disabled.is_none() => Outcome::Run(e.run),
                    _ => Outcome::Open,
                };
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                let last = self.matches().len().saturating_sub(1);
                self.selected = (self.selected + 1).min(last);
            }
            _ => {
                if self.query.input(Event::Key(*key)) {
                    self.selected = 0;
                }
            }
        }
        Outcome::Open
    }

    /// Dim the whole frame, then draw the palette on top.
    pub fn draw(&self, f: &mut Frame, theme: &Theme) {
        let full = f.area();
        f.buffer_mut()
            .set_style(full, Style::default().add_modifier(Modifier::DIM));
        let w = WIDTH.min(full.width.saturating_sub(2));
        let top = TOP.min(full.height.saturating_sub(HEIGHT) / 2);
        let h = HEIGHT.min(full.height.saturating_sub(top));
        let area = Rect::new(
            full.x + full.width.saturating_sub(w) / 2,
            full.y + top,
            w,
            h,
        );
        let bg = Style::default().bg(theme.surface_overlay);
        let info = Style::default()
            .fg(theme.accent_info)
            .add_modifier(Modifier::BOLD);
        let block = panel_plain(Focus::Overlay, theme)
            .style(bg)
            .title(Span::styled(" Command palette ", info))
            .title_top(
                Line::from(Span::styled(
                    " INSERT ",
                    info.add_modifier(Modifier::REVERSED),
                ))
                .right_aligned(),
            );
        let inner = block.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(block, area);
        if inner.height < CHROME_ROWS + 1 || inner.width < 8 {
            return;
        }
        let row = |y: u16| Rect::new(inner.x, inner.y + y, inner.width, 1);

        f.render_widget(Paragraph::new(Span::styled("› ", info)), row(0));
        let field = row(0);
        let field = Rect::new(field.x + 2, field.y, field.width - 2, 1);
        input::render(&self.query, f, field, theme, true);
        let rule = "─".repeat(inner.width as usize);
        f.render_widget(
            Paragraph::new(Span::styled(rule, Style::default().fg(theme.text_muted))),
            row(1),
        );

        let matches = self.matches();
        let rows = (inner.height - CHROME_ROWS) as usize;
        let skip = self.selected.saturating_sub(rows.saturating_sub(1));
        for (i, (entry, hits)) in matches.iter().enumerate().skip(skip).take(rows) {
            let line = entry_line(entry, hits, i == self.selected, inner.width as usize, theme);
            f.render_widget(Paragraph::new(line), row(2 + (i - skip) as u16));
        }

        let key = key_style(theme);
        let muted = Style::default().fg(theme.text_muted);
        let footer = Line::from(vec![
            Span::styled("⏎", key),
            Span::styled(" run  ", muted),
            Span::styled("↑↓", key),
            Span::styled(" pick  ", muted),
            Span::styled("Esc", key),
            Span::styled(
                format!(
                    " close · {} of {} commands",
                    matches.len(),
                    self.entries.len()
                ),
                muted,
            ),
        ]);
        f.render_widget(Paragraph::new(footer), row(inner.height - 1));
    }
}

/// One row: selection bar, label with UNDERLINED hits, the reason or
/// nothing, and the key at the right edge.
fn entry_line(
    entry: &Entry,
    hits: &[usize],
    selected: bool,
    width: usize,
    theme: &Theme,
) -> Line<'static> {
    let base = if selected {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
    } else {
        Style::default().fg(theme.text_primary)
    };
    let label = match (entry.destructive, entry.disabled) {
        (_, Some(_)) => base.fg(theme.text_muted),
        (true, None) => base.fg(theme.accent_error),
        (false, None) => base,
    };
    let hit = label
        .fg(theme.accent_info)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let mut spans = vec![Span::styled(
        if selected { "▌ " } else { "  " },
        base.add_modifier(Modifier::BOLD),
    )];
    let mut used = 2;
    if entry.disabled.is_some() {
        spans.push(Span::styled("⊘ ", label));
        used += 2;
    }
    for (i, c) in entry.label.chars().enumerate() {
        spans.push(Span::styled(
            c.to_string(),
            if hits.contains(&i) { hit } else { label },
        ));
        used += 1;
    }
    let note = entry.disabled.unwrap_or(if entry.destructive {
        "asks to confirm"
    } else {
        ""
    });
    let note_at = 30.max(used + 1);
    let room = width.saturating_sub(note_at + KEY_COLS + 1);
    let note: String = note.chars().take(room).collect();
    spans.push(Span::styled(" ".repeat(note_at - used), base));
    used = note_at + note.chars().count();
    spans.push(Span::styled(note, base.fg(theme.text_muted)));
    let key: String = entry.key.chars().take(KEY_COLS).collect();
    let pad = width.saturating_sub(used + key.chars().count() + 1);
    spans.push(Span::styled(" ".repeat(pad), base));
    spans.push(Span::styled(key, base.patch(key_style(theme))));
    spans.push(Span::styled(" ", base));
    Line::from(spans)
}
