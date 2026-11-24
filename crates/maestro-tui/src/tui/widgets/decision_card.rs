//! Decision card (#1287, design `docs/design-system/index.html` §02.22
//! "Decision card"): the one component for every "the loop stopped and
//! needs you" moment: a queue failure, a continuous pause, a hollow run.
//!
//! A heading, evidence sections, then CHOOSE: a radio list where each
//! option has a one-line result and its price on the right. `↑↓` picks,
//! `⏎` confirms the picked option, `v` asks for the evidence, `Esc` means
//! "decide later". Old single-key choices (`r` `s` `a`) are not bound: a
//! choice that may spend is picked first and confirmed second.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::ds::money::money;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::screens::milestone_rows::row_line;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;

/// What an option costs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cost {
    /// Starts no paid work: `$0.00`.
    Free,
    /// Starts a paid session: `est ≥ $0.50 · stops at $5.00`.
    Paid { est: f64, cap: f64 },
}

impl Cost {
    /// A new paid session at the launch budget: the pre-spawn floor
    /// estimate and the per-session cap.
    pub fn retry(budget: &crate::tui::widgets::launch_dialog::LaunchBudget) -> Self {
        Cost::Paid {
            est: budget.estimate_each(),
            cap: budget.per_session_usd,
        }
    }

    pub fn label(self) -> String {
        match self {
            Cost::Free => money(Some(0.0)),
            Cost::Paid { est, cap } if cap > 0.0 => {
                format!("est ≥ {} · stops at {}", money(Some(est)), money(Some(cap)))
            }
            Cost::Paid { est, .. } => format!("est ≥ {} · no cap", money(Some(est))),
        }
    }
}

/// One evidence block: a heading and `(label, value)` rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

/// One choice: its label, what it does, and its price.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionOption {
    pub label: String,
    pub note: String,
    pub cost: Cost,
}

/// What a key did to the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionAction {
    /// A key the card does not take.
    None,
    /// `↑↓` moved the pick.
    Pick(usize),
    /// `⏎` on the picked option.
    Confirm(usize),
    /// `v`: show the evidence (the session's log).
    Evidence,
    /// `Esc`: decide later.
    Later,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionCard {
    pub title: String,
    /// Short state word drawn before the title (`needs you`, `hollow`).
    pub tag: String,
    pub sections: Vec<Section>,
    /// The first option is the default: it never spends outside the plan.
    pub options: Vec<DecisionOption>,
    pub selected: usize,
    /// `Esc` means "decide later" (false: the loop cannot wait, `Esc` does
    /// nothing and no hint says otherwise).
    pub later: bool,
    /// `v` shows the session's log (false: no session to show).
    pub evidence: bool,
}

impl DecisionCard {
    pub fn handle_key(&mut self, key: &KeyEvent) -> DecisionAction {
        let last = self.options.len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                DecisionAction::Pick(self.selected)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(last);
                DecisionAction::Pick(self.selected)
            }
            KeyCode::Enter if !self.options.is_empty() => {
                DecisionAction::Confirm(self.selected.min(last))
            }
            KeyCode::Char('v') if self.evidence => DecisionAction::Evidence,
            KeyCode::Esc if self.later => DecisionAction::Later,
            _ => DecisionAction::None,
        }
    }

    /// The card body at `width` columns: sections, CHOOSE, the button row.
    pub fn lines(&self, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        let muted = Style::default().fg(theme.text_secondary);
        let text = Style::default().fg(theme.text_primary);
        let heading = muted.add_modifier(Modifier::BOLD);
        let mut lines = vec![Line::from(vec![
            Span::styled(
                format!(" {} ", self.tag),
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::REVERSED | Modifier::BOLD),
            ),
            Span::styled(format!(" {}", sanitize_for_terminal(&self.title)), text),
        ])];
        for section in &self.sections {
            lines.push(Line::from(Span::styled(section.title.clone(), heading)));
            for (label, value) in &section.rows {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {label:<10}"), muted),
                    Span::styled(sanitize_for_terminal(value), text),
                ]));
            }
        }
        lines.push(Line::from(Span::styled("CHOOSE", heading)));
        for (i, option) in self.options.iter().enumerate() {
            let picked = i == self.selected;
            let (mark, style) = if picked {
                (
                    "▸ ●",
                    text.fg(theme.accent_info).add_modifier(Modifier::BOLD),
                )
            } else {
                ("  ○", text)
            };
            lines.push(row_line(
                vec![Span::styled(format!("{mark} {}", option.label), style)],
                vec![Span::styled(option.cost.label(), muted)],
                width,
            ));
            lines.push(Line::from(Span::styled(
                format!("      {}", option.note),
                muted,
            )));
        }
        if let Some(option) = self.options.get(self.selected) {
            let key = Style::default()
                .fg(theme.accent_info)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD);
            let mut row = vec![
                Span::styled(" ⏎ ", key),
                Span::styled(format!(" {}   ", option.label), text),
            ];
            if self.later {
                row.push(Span::styled(" Esc ", key));
                row.push(Span::styled(" decide later   ", text));
            }
            let hint = if self.evidence {
                "↑↓ pick · v evidence"
            } else {
                "↑↓ pick"
            };
            row.push(Span::styled(hint, muted));
            lines.push(Line::from(row));
        }
        lines
    }

    /// The card in a panel filling `area`.
    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = panel("Decision", Focus::Active, theme);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let inner = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        let lines = self.lines(inner.width as usize, theme);
        f.render_widget(Paragraph::new(lines), inner);
    }

    /// The card as a dialog centred in `area` (the hollow run).
    pub fn draw_dialog(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let w = 72.min(area.width);
        let rows = 2
            + self
                .sections
                .iter()
                .map(|s| s.rows.len() + 1)
                .sum::<usize>()
            + 2 * self.options.len()
            + 3;
        let h = (rows as u16).min(area.height);
        let popup = Rect::new(
            area.x + area.width.saturating_sub(w) / 2,
            area.y + area.height.saturating_sub(h) / 2,
            w,
            h,
        );
        f.render_widget(Clear, popup);
        self.draw(f, popup, theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> DecisionCard {
        DecisionCard {
            title: "#5 failed".into(),
            tag: "needs you".into(),
            sections: vec![],
            options: vec![
                DecisionOption {
                    label: "Skip #5".into(),
                    note: "stays open".into(),
                    cost: Cost::Free,
                },
                DecisionOption {
                    label: "Retry #5".into(),
                    note: "a working run is a paid run".into(),
                    cost: Cost::Paid { est: 0.5, cap: 5.0 },
                },
            ],
            selected: 0,
            later: true,
            evidence: true,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    #[test]
    fn pick_then_confirm() {
        let mut c = card();
        assert_eq!(c.handle_key(&key(KeyCode::Down)), DecisionAction::Pick(1));
        assert_eq!(c.handle_key(&key(KeyCode::Down)), DecisionAction::Pick(1));
        assert_eq!(
            c.handle_key(&key(KeyCode::Enter)),
            DecisionAction::Confirm(1)
        );
        assert_eq!(c.handle_key(&key(KeyCode::Up)), DecisionAction::Pick(0));
        assert_eq!(c.handle_key(&key(KeyCode::Esc)), DecisionAction::Later);
        assert_eq!(
            c.handle_key(&key(KeyCode::Char('v'))),
            DecisionAction::Evidence
        );
        c.later = false;
        c.evidence = false;
        assert_eq!(c.handle_key(&key(KeyCode::Esc)), DecisionAction::None);
        assert_eq!(c.handle_key(&key(KeyCode::Char('v'))), DecisionAction::None);
    }

    #[test]
    fn single_keys_do_not_choose() {
        let mut c = card();
        for k in ['r', 's', 'a'] {
            assert_eq!(c.handle_key(&key(KeyCode::Char(k))), DecisionAction::None);
        }
    }

    #[test]
    fn cost_labels() {
        assert_eq!(Cost::Free.label(), "$0.00");
        assert_eq!(
            Cost::Paid { est: 0.5, cap: 5.0 }.label(),
            "est ≥ $0.50 · stops at $5.00"
        );
        assert_eq!(
            Cost::Paid { est: 0.0, cap: 0.0 }.label(),
            "est ≥ $0.00 · no cap"
        );
    }
}
