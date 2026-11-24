//! #1282 AI improve, decided per section. Each changed text field of the
//! candidate gets its own decision: `a` accepts it, `d` keeps the user's
//! text. ⏎ merges only the accepted sections; undecided ones keep the
//! user's text. Only the 8 text fields are merged, so the trusted seats
//! re-stamped by `parse_improve_response` never move.

use super::{FieldId, IssueCreationPayload, IssueWizardScreen};
use crossterm::event::KeyCode;

/// What the user decided for one section of the AI candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Decision {
    /// Not decided yet: keeps the user's text.
    #[default]
    Pending,
    Accepted,
    KeptMine,
}

/// Indexes into `FieldId::ALL` whose text differs in `candidate`.
pub(super) fn changed(
    original: &IssueCreationPayload,
    candidate: &IssueCreationPayload,
) -> Vec<usize> {
    FieldId::ALL
        .iter()
        .enumerate()
        .filter(|(_, f)| f.payload_ref(original) != f.payload_ref(candidate))
        .map(|(i, _)| i)
        .collect()
}

impl IssueWizardScreen {
    pub fn decisions(&self) -> &[Decision; 8] {
        &self.decisions
    }

    /// The focused section, as an index into `FieldId::ALL`.
    pub fn section_focus(&self) -> usize {
        self.section_focus
    }

    /// Changed sections of the current candidate (empty without one).
    pub fn changed_sections(&self) -> Vec<usize> {
        self.improve_candidate
            .as_ref()
            .map(|c| changed(&self.payload, c))
            .unwrap_or_default()
    }

    /// Set the focused section's decision.
    pub fn decide_section(&mut self, decision: Decision) {
        if self.changed_sections().contains(&self.section_focus) {
            self.decisions[self.section_focus] = decision;
        }
    }

    /// Merge the accepted sections into the payload and close the diff.
    pub fn apply_improve_decisions(&mut self) {
        let Some(candidate) = self.improve_candidate.take() else {
            return;
        };
        for (i, field) in FieldId::ALL.iter().enumerate() {
            if self.decisions[i] == Decision::Accepted {
                *field.payload_ref_mut(&mut self.payload) =
                    field.payload_ref(&candidate).to_string();
            }
        }
        self.decisions = [Decision::Pending; 8];
    }

    fn move_section(&mut self, down: bool) {
        let changed = self.changed_sections();
        let Some(pos) = changed.iter().position(|i| *i == self.section_focus) else {
            self.section_focus = changed.first().copied().unwrap_or(0);
            return;
        };
        let next = if down {
            (pos + 1).min(changed.len() - 1)
        } else {
            pos.saturating_sub(1)
        };
        self.section_focus = changed[next];
    }

    /// Keys while the candidate diff is shown.
    pub(super) fn handle_improve_diff(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('a') => self.decide_section(Decision::Accepted),
            KeyCode::Char('d') => self.decide_section(Decision::KeptMine),
            KeyCode::Char('j') | KeyCode::Down => self.move_section(true),
            KeyCode::Char('k') | KeyCode::Up => self.move_section(false),
            KeyCode::Enter => self.apply_improve_decisions(),
            KeyCode::Char('r') => {
                self.discard_improve();
                self.begin_improve();
            }
            KeyCode::Esc => self.discard_improve(),
            _ => {}
        }
    }
}
