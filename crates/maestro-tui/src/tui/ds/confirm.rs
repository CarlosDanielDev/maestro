//! Confirms for destructive actions (design §04 `confirm`): the typed
//! confirm (§05 "Killing the wrong agent", #1261) and [`HoldToConfirm`]
//! (§05 "Quitting while agents run", #1260).
//!
//! The dialog states what is kept and lost, then asks you to type a token
//! (the issue number, or a word for a global action). `Esc` cancels and is
//! the default; `⏎` does nothing until the typed text equals the token.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::ds::panel::{Focus, panel};
use crate::tui::theme::Theme;
use crate::tui::widgets::input::{self, Input, InputConfig};

/// Longest token the field accepts.
const MAX_LEN: usize = 16;

/// What a key did to the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// Still open: a key was typed, or `⏎` was pressed before a match.
    Pending,
    /// `⏎` with the typed text equal to the token.
    Confirmed,
    /// `Esc`.
    Cancelled,
}

pub struct TypedConfirm {
    title: String,
    expected: String,
    action: String,
    consequences: Vec<(String, String)>,
    input: Input,
}

impl TypedConfirm {
    /// `consequences` are `(label, text)` rows (`("kept", "the worktree")`);
    /// an empty label draws the text alone. `action` names `⏎` (`kill`).
    pub fn new(
        title: impl Into<String>,
        expected: impl Into<String>,
        action: impl Into<String>,
        consequences: Vec<(String, String)>,
    ) -> Self {
        Self {
            title: title.into(),
            expected: expected.into(),
            action: action.into(),
            consequences,
            input: Input::single_line(InputConfig::new().max_len(MAX_LEN)),
        }
    }

    pub fn expected(&self) -> &str {
        &self.expected
    }

    pub fn typed(&self) -> String {
        self.input.text()
    }

    /// True when `⏎` would confirm.
    pub fn matches(&self) -> bool {
        self.input.text() == self.expected
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ConfirmOutcome {
        match key.code {
            KeyCode::Esc => ConfirmOutcome::Cancelled,
            KeyCode::Enter if self.matches() => ConfirmOutcome::Confirmed,
            KeyCode::Enter => ConfirmOutcome::Pending,
            _ => {
                self.input.input(Event::Key(key));
                ConfirmOutcome::Pending
            }
        }
    }

    /// Draw the dialog centred in `area`: Double (danger) border, the
    /// consequences, the field and the `Esc` / `⏎` row.
    pub fn render(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let w = 64.min(area.width);
        let h = (self.consequences.len() as u16 + 7).min(area.height);
        let popup = Rect::new(
            area.x + area.width.saturating_sub(w) / 2,
            area.y + area.height.saturating_sub(h) / 2,
            w,
            h,
        );
        f.render_widget(Clear, popup);
        let block = panel(&self.title, Focus::Danger, theme);
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        let inner = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let muted = Style::default().fg(theme.text_muted);
        let text = Style::default().fg(theme.text_primary);
        let danger = Style::default()
            .fg(theme.accent_error)
            .add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line> = self
            .consequences
            .iter()
            .map(|(label, value)| {
                if label.is_empty() {
                    Line::from(Span::styled(value.clone(), text))
                } else {
                    Line::from(vec![
                        Span::styled(format!("{label:<7}"), muted),
                        Span::styled(value.clone(), text),
                    ])
                }
            })
            .collect();
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Type ", muted),
            Span::styled(self.expected.clone(), text.add_modifier(Modifier::BOLD)),
            Span::styled(" to confirm:", muted),
        ]));
        let field_row = lines.len() as u16;
        lines.push(Line::from(Span::styled("› ", danger)));
        lines.push(Line::from(""));
        lines.push(self.buttons_line(theme));
        f.render_widget(Paragraph::new(lines), inner);

        if field_row >= inner.height {
            return;
        }
        let y = inner.y + field_row;
        let count = format!(
            "{} of {}",
            self.input.text().chars().count(),
            self.expected.chars().count()
        );
        let count_w = (count.chars().count() as u16 + 2).min(inner.width);
        f.render_widget(
            Paragraph::new(Span::styled(count, muted)).alignment(Alignment::Right),
            Rect::new(inner.x + inner.width - count_w, y, count_w, 1),
        );
        let field_w = inner.width.saturating_sub(2 + count_w);
        input::render(
            &self.input,
            f,
            Rect::new(inner.x + 2, y, field_w, 1),
            theme,
            true,
        );
    }

    /// `Esc` is always the default; `⏎` reads `(locked)` until the text matches.
    fn buttons_line(&self, theme: &Theme) -> Line<'static> {
        let esc = Style::default()
            .fg(theme.accent_info)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD);
        let mut spans = vec![
            Span::styled(" Esc ", esc),
            Span::styled(
                " cancel (default)   ",
                Style::default().fg(theme.text_primary),
            ),
        ];
        if self.matches() {
            let danger = Style::default()
                .fg(theme.accent_error)
                .add_modifier(Modifier::BOLD);
            spans.push(Span::styled(" ⏎ ", danger.add_modifier(Modifier::REVERSED)));
            spans.push(Span::styled(format!(" {}", self.action), danger));
        } else {
            let locked = Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM);
            spans.push(Span::styled(" ⏎ ", locked));
            spans.push(Span::styled(format!(" {} (locked)", self.action), locked));
        }
        Line::from(spans)
    }
}

/// How long the key must be held (key repeats) to confirm.
pub const HOLD: Duration = Duration::from_millis(500);
/// How long after the first press a second press still confirms.
pub const WINDOW: Duration = Duration::from_secs(2);
/// Cells in the progress bar.
const BAR: usize = 16;

/// Hold-to-confirm (design §04 `confirm`, §05 "Quitting while agents run",
/// #1260). Terminals report no key release unless the kitty protocol is on
/// (maestro does not turn it on), so a hold is a run of key repeats, and a
/// second press inside [`WINDOW`] is the fallback. The first press only
/// arms. `Esc` cancels; `⏎` and every other key do nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldToConfirm {
    code: KeyCode,
    mods: KeyModifiers,
    armed_at: Option<Instant>,
    held: Duration,
}

impl HoldToConfirm {
    /// A hold on `code` with `mods` (`⌃Q` = `Char('q')` + `CONTROL`).
    pub const fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        Self {
            code,
            mods,
            armed_at: None,
            held: Duration::ZERO,
        }
    }

    pub fn reset(&mut self) {
        self.armed_at = None;
        self.held = Duration::ZERO;
    }

    /// True while a second press would confirm.
    pub fn is_armed(&self, now: Instant) -> bool {
        self.armed_at
            .is_some_and(|at| now.saturating_duration_since(at) <= WINDOW)
    }

    /// `0.0` idle, `0.5` armed, rising to `1.0` as the hold nears [`HOLD`].
    pub fn progress(&self, now: Instant) -> f64 {
        if !self.is_armed(now) {
            return 0.0;
        }
        let held = self.held.as_secs_f64() / HOLD.as_secs_f64();
        0.5 + 0.5 * held.min(1.0)
    }

    pub fn handle_key(&mut self, key: &KeyEvent, now: Instant) -> ConfirmOutcome {
        if key.code == KeyCode::Esc {
            return ConfirmOutcome::Cancelled;
        }
        let is_hold_key = key.code == self.code && key.modifiers.contains(self.mods);
        if !is_hold_key || key.kind == KeyEventKind::Release {
            return ConfirmOutcome::Pending;
        }
        match self.armed_at.filter(|_| self.is_armed(now)) {
            Some(at) if key.kind == KeyEventKind::Repeat => {
                self.held = now.saturating_duration_since(at);
                if self.held >= HOLD {
                    ConfirmOutcome::Confirmed
                } else {
                    ConfirmOutcome::Pending
                }
            }
            Some(_) => ConfirmOutcome::Confirmed,
            None => {
                self.armed_at = Some(now);
                self.held = Duration::ZERO;
                ConfirmOutcome::Pending
            }
        }
    }

    /// The hold row and the fallback line: ` ⌃Q   hold to quit  ████░░░░`.
    /// `key` is how the hold key is drawn, `action` what it does.
    pub fn lines(
        &self,
        key: &str,
        action: &str,
        now: Instant,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let danger = Style::default()
            .fg(theme.accent_error)
            .add_modifier(Modifier::BOLD);
        let text = Style::default().fg(theme.text_primary);
        let muted = Style::default().fg(theme.text_muted);
        let filled = (self.progress(now) * BAR as f64).round() as usize;
        let second = if self.is_armed(now) {
            format!("       press {key} again to {action}")
        } else {
            format!("       or press {key} twice within {}s", WINDOW.as_secs())
        };
        vec![
            Line::from(vec![
                Span::styled(format!(" {key} "), danger.add_modifier(Modifier::REVERSED)),
                Span::styled(format!("   hold to {action}  "), text),
                Span::styled("█".repeat(filled), Style::default().fg(theme.accent_error)),
                Span::styled("░".repeat(BAR - filled), muted),
                Span::styled(format!(" {:.1}s", HOLD.as_secs_f64()), muted),
            ]),
            Line::from(Span::styled(second, muted)),
        ]
    }
}

#[cfg(test)]
#[path = "confirm_tests.rs"]
mod tests;
