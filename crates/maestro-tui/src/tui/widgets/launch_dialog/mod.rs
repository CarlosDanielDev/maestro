//! The shared launch dialog (#1262, design `docs/design-system/index.html`
//! §02.8 "Launch dialog"). It states mode, model, spend against the budget
//! and the worktree before any agent starts, and folds in the pre-spawn
//! budget gate: a projected `Block` disables `Enter` and names the reason.
//!
//! Issues is the first caller; Milestones, Run prompt and Dashboard adopt it
//! in their own issues.

mod draw;
mod sections;
#[cfg(test)]
mod tests;

pub use draw::draw;

use crate::budget::projector::DEFAULT_FLOOR_PCT;
use crate::budget::{BudgetEnforcer, PreSpawnDecision};
use crate::tui::ds::money::money;
use crate::tui::widgets::input::{Input, InputConfig};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// How the launched session runs. Replaces the `Produce PR` and
/// `Interaction` checkboxes: one radio, each option with its consequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchMode {
    #[default]
    Autonomous,
    ChatUntilPr,
    ChatOpen,
}

impl LaunchMode {
    pub const ALL: [LaunchMode; 3] = [
        LaunchMode::Autonomous,
        LaunchMode::ChatUntilPr,
        LaunchMode::ChatOpen,
    ];

    /// `(produce_pr, interaction)`, the flags `SessionConfig` already carries.
    pub fn flags(self) -> (bool, bool) {
        match self {
            LaunchMode::Autonomous => (true, false),
            LaunchMode::ChatUntilPr => (true, true),
            LaunchMode::ChatOpen => (false, true),
        }
    }

    /// `[behavior.launch]` defaults keep working: `(_, false)` is Autonomous.
    pub fn from_defaults((produce_pr, interaction): (bool, bool)) -> Self {
        match (produce_pr, interaction) {
            (_, false) => LaunchMode::Autonomous,
            (true, true) => LaunchMode::ChatUntilPr,
            (false, true) => LaunchMode::ChatOpen,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LaunchMode::Autonomous => "Autonomous → PR",
            LaunchMode::ChatUntilPr => "Chat → PR",
            LaunchMode::ChatOpen => "Chat, stays open",
        }
    }

    /// The consequence line, in words.
    pub fn consequence(self) -> &'static str {
        match self {
            LaunchMode::Autonomous => "works alone in a worktree, runs gates, opens a PR",
            LaunchMode::ChatUntilPr => "opens the chat; you steer it; it opens a PR",
            LaunchMode::ChatOpen => "opens the chat; stays open after a PR until quit",
        }
    }

    fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        let n = Self::ALL.len();
        Self::ALL[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }]
    }
}

/// The money the dialog states, read from App on every frame
/// (`budget_prespawn::launch_budget`). The default is "no budget": no cap,
/// nothing blocks, as `App::pre_spawn_decision` does with no enforcer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LaunchBudget {
    /// The model the launch resolves to.
    pub model: String,
    pub per_session_usd: f64,
    pub total_usd: f64,
    pub alert_threshold_pct: u8,
    /// Spent so far this run.
    pub spent: f64,
}

impl LaunchBudget {
    /// Projected cost of one fresh session: the `DefaultBudgetProjector`
    /// floor, the same number the pre-spawn gate uses.
    pub fn estimate_each(&self) -> f64 {
        self.per_session_usd * DEFAULT_FLOOR_PCT
    }

    /// The pre-spawn gate's decision for the next fresh session. The gate
    /// (`App::pre_spawn_decision`) checks each spawn alone against the spend
    /// so far, so a multi launch uses the same one-session rule.
    pub fn decision(&self) -> PreSpawnDecision {
        BudgetEnforcer::new(
            self.per_session_usd,
            self.total_usd,
            self.alert_threshold_pct,
        )
        .check_pre_spawn(self.spent, self.estimate_each())
    }
}

/// Focus stop inside the dialog. Tab / BackTab switch between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchFocus {
    Instructions,
    Mode,
}

/// What a key in the dialog asks the caller to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchAction {
    None,
    Cancel,
    /// Launch. `None` means no (or whitespace-only) instructions.
    Confirm(Option<String>),
}

/// The dialog state. Built from a list of issues; the caller owns it.
pub struct LaunchDialog {
    /// Optional instructions, added to the prompt.
    pub(crate) editor: Input,
    /// `(issue_number, issue_title)`, at least one.
    pub(crate) selected_issues: Vec<(u64, String)>,
    /// Multi-issue only: one unified PR instead of one per issue (Ctrl+U).
    pub(crate) unified_pr: bool,
    pub(crate) focus: LaunchFocus,
    pub(crate) mode: LaunchMode,
    pub(crate) budget: LaunchBudget,
}

impl LaunchDialog {
    /// With 2+ issues only `Autonomous` is offered: those launches go to the
    /// queue or one unified session, which never open the chat.
    pub fn new(selected_issues: Vec<(u64, String)>, mode: LaunchMode) -> Self {
        let mode = if selected_issues.len() > 1 {
            LaunchMode::Autonomous
        } else {
            mode
        };
        Self {
            editor: Input::multi_line(InputConfig::new().placeholder("Type your prompt here...")),
            selected_issues,
            unified_pr: false,
            focus: LaunchFocus::Instructions,
            mode,
            budget: LaunchBudget::default(),
        }
    }

    pub fn set_budget(&mut self, budget: LaunchBudget) {
        self.budget = budget;
    }

    /// The instructions, logical lines joined with `\n`.
    pub fn editor_text(&self) -> String {
        self.editor.text()
    }

    /// The footer hints while the dialog is open (Issues, Run queue).
    pub fn footer_hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.is_multi() {
            vec![
                ("Enter", "Launch all"),
                ("Tab", "Instructions/Mode"),
                ("↑/↓", "Mode"),
                ("Ctrl+U", "Unified PR"),
                ("Shift+Enter", "New line"),
                ("Esc", "Cancel"),
            ]
        } else {
            vec![
                ("Enter", "Launch"),
                ("Tab", "Instructions/Mode"),
                ("↑/↓", "Mode"),
                ("Shift+Enter", "New line"),
                ("Esc", "Cancel"),
            ]
        }
    }

    pub fn is_multi(&self) -> bool {
        self.selected_issues.len() > 1
    }

    /// How many sessions `Enter` starts: one per issue, or one unified.
    pub fn sessions(&self) -> usize {
        if self.is_multi() && !self.unified_pr {
            self.selected_issues.len()
        } else {
            1
        }
    }

    /// Why launch is locked, or `None` when `Enter` launches. A projected
    /// `Block` from the pre-spawn gate locks it: Block stays Block.
    pub fn block_reason(&self) -> Option<String> {
        match self.budget.decision() {
            PreSpawnDecision::Block {
                projected_total,
                limit,
            } => Some(format!(
                "over the cap ({} of {})",
                money(Some(projected_total)),
                money(Some(limit)),
            )),
            _ => None,
        }
    }

    pub fn handle_input(&mut self, event: &Event) -> LaunchAction {
        if let Event::Paste(_) = event {
            if self.focus == LaunchFocus::Instructions {
                self.editor.input(event.clone());
            }
            return LaunchAction::None;
        }
        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        else {
            return LaunchAction::None;
        };
        let instructions = self.focus == LaunchFocus::Instructions;
        match code {
            KeyCode::Esc => LaunchAction::Cancel,
            // Shift+Enter inserts a newline while editing.
            KeyCode::Enter if modifiers.contains(KeyModifiers::SHIFT) => {
                if instructions {
                    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
                    self.editor.input(Event::Key(enter));
                }
                LaunchAction::None
            }
            // Enter launches from any focus stop, unless the budget locks it.
            KeyCode::Enter if self.block_reason().is_some() => LaunchAction::None,
            KeyCode::Enter => {
                let trimmed = self.editor_text().trim().to_string();
                LaunchAction::Confirm((!trimmed.is_empty()).then_some(trimmed))
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if instructions {
                    LaunchFocus::Mode
                } else {
                    LaunchFocus::Instructions
                };
                LaunchAction::None
            }
            KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                if self.is_multi() {
                    self.unified_pr = !self.unified_pr;
                }
                LaunchAction::None
            }
            KeyCode::Up | KeyCode::Char('k') if !instructions => {
                if !self.is_multi() {
                    self.mode = self.mode.step(false);
                }
                LaunchAction::None
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char(' ') if !instructions => {
                if !self.is_multi() {
                    self.mode = self.mode.step(true);
                }
                LaunchAction::None
            }
            _ => {
                if instructions {
                    self.editor.input(event.clone());
                }
                LaunchAction::None
            }
        }
    }
}
