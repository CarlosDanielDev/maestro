//! The hollow-run dialog (#1287, design §02.22 "Hollow evidence"): a
//! decision card over the screen you are on, shown when a hollow completion
//! is past its auto-retry limit. It lists the four checks behind "hollow"
//! (`Session::detect_hollow_completion`), the attempts, and two choices:
//! Skip (default, `$0.00`, the only choice that dismisses the dialog for
//! good) and Retry (a paid run). `Esc` decides later; `v` shows the log.

use super::{Screen, ScreenAction};
use crate::session::types::Session;
use crate::tui::app::{SessionView, TuiMode};
use crate::tui::ds::money::money;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crate::tui::widgets::decision_card::{
    Cost, DecisionAction, DecisionCard, DecisionOption, Section,
};
use crossterm::event::{Event, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};
use uuid::Uuid;

/// The four checks of `detect_hollow_completion`, with their values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HollowEvidence {
    pub cost_usd: f64,
    pub files_touched: usize,
    pub tool_calls: bool,
    pub elapsed_secs: Option<i64>,
}

impl HollowEvidence {
    pub fn from_session(s: &Session) -> Self {
        Self {
            cost_usd: s.cost_usd,
            files_touched: s.files_touched.len(),
            tool_calls: s.has_tool_calls(),
            elapsed_secs: s.elapsed().map(|d| d.num_seconds()),
        }
    }
}

/// Screen shown when a hollow completion exceeds auto-retry limits.
pub struct HollowRetryScreen {
    pub session_id: Uuid,
    card: DecisionCard,
}

impl HollowRetryScreen {
    /// `retries` is `(used, max)`; `retry_cost` is what Retry costs
    /// (`Cost::retry(&budget_prespawn::launch_budget(app))`).
    pub fn new(
        session_id: Uuid,
        session_label: String,
        retries: (u32, u32),
        evidence: HollowEvidence,
        retry_cost: Cost,
    ) -> Self {
        let card = card(&session_label, retries, &evidence, retry_cost);
        Self { session_id, card }
    }
}

fn card(
    label: &str,
    (used, max): (u32, u32),
    e: &HollowEvidence,
    retry_cost: Cost,
) -> DecisionCard {
    let limit = Session::HOLLOW_DURATION_THRESHOLD_SECS;
    let ran = e.elapsed_secs.map_or("unknown".to_string(), |s| {
        format!("{s} s (under {limit} s)")
    });
    let files = match e.files_touched {
        0 => "none touched".to_string(),
        n => format!("{n} touched"),
    };
    let past = if used >= max {
        " · past the retry limit"
    } else {
        ""
    };
    DecisionCard {
        title: format!("{label} finished without doing any work"),
        tag: "hollow".into(),
        sections: vec![
            Section {
                title: "WHY HOLLOW".into(),
                rows: vec![
                    ("cost".into(), money(Some(e.cost_usd))),
                    ("files".into(), files),
                    (
                        "tool calls".into(),
                        if e.tool_calls { "some" } else { "none" }.into(),
                    ),
                    ("ran".into(), ran),
                ],
            },
            Section {
                title: "ATTEMPTS".into(),
                rows: vec![(
                    "retries".into(),
                    format!("{used} of {max} auto-retries used"),
                )],
            },
        ],
        options: vec![
            DecisionOption {
                label: "Skip".into(),
                note: "the issue stays open, nothing is marked done".into(),
                cost: Cost::Free,
            },
            DecisionOption {
                label: "Retry".into(),
                note: format!("a working run is a paid run{past}"),
                cost: retry_cost,
            },
        ],
        selected: 0,
        later: true,
        evidence: true,
    }
}

impl KeymapProvider for HollowRetryScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Hollow run",
            bindings: vec![
                KeyBinding {
                    key: "↑↓",
                    description: "Pick Skip or Retry",
                },
                KeyBinding {
                    key: "Enter",
                    description: "Confirm the picked option",
                },
                KeyBinding {
                    key: "v",
                    description: "View logs",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Decide later",
                },
            ],
        }]
    }
}

impl Screen for HollowRetryScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        let Event::Key(
            key @ KeyEvent {
                kind: KeyEventKind::Press,
                ..
            },
        ) = event
        else {
            return ScreenAction::None;
        };
        match self.card.handle_key(key) {
            DecisionAction::Confirm(0) => ScreenAction::SkipHollow(self.session_id),
            DecisionAction::Confirm(_) => ScreenAction::RetryHollow(self.session_id),
            DecisionAction::Evidence => ScreenAction::Push(TuiMode::Session {
                id: self.session_id,
                view: SessionView::Detail,
            }),
            DecisionAction::Later => ScreenAction::Pop,
            DecisionAction::Pick(_) | DecisionAction::None => ScreenAction::None,
        }
    }

    fn footer_hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("↑↓", "Pick"), ("Enter", "Confirm"), ("v", "View Logs")]
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.card.draw_dialog(f, area, theme);
    }
}

#[cfg(test)]
#[path = "hollow_retry_tests.rs"]
mod tests;
