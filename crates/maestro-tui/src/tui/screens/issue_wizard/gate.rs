//! #1282 Create gate on Preview: the only place the wizard starts a GitHub
//! write. Blocked while `validation_errors()` has an error or
//! `lint::placeholders` finds a `[TODO:`; then the
//! first ⏎ arms (`ds::armed::ArmedEnter`) and a second ⏎ within 3s calls
//! `begin_create`, which stays the only path to `TuiCommand::CreateIssue`.

use super::{IssueWizardScreen, lint};
use crate::tui::ds::armed::{ArmedEnter, ArmedState, EnterOutcome};
use crossterm::event::KeyCode;

impl IssueWizardScreen {
    pub fn create_gate(&self) -> &ArmedEnter {
        &self.create_gate
    }

    /// Block the gate while a required field fails validation (an
    /// accepted AI section can empty one after Details passed) or a
    /// placeholder is left; unblock once both are fixed. Called on entering
    /// Preview and before every Enter there.
    pub(super) fn refresh_gate(&mut self) {
        let reason = match self.validation_errors().first() {
            Some((field, err)) => Some(format!("{} {err} · ⏎ goes to it", field.label())),
            None => match lint::total(&self.payload) {
                0 => None,
                left => {
                    let s = if left == 1 { "" } else { "s" };
                    Some(format!(
                        "{left} [TODO: placeholder{s} left · ⏎ jumps to the first"
                    ))
                }
            },
        };
        match reason {
            Some(reason) if *self.create_gate.state() != ArmedState::Blocked(reason.clone()) => {
                self.create_gate.block(reason);
            }
            None if matches!(self.create_gate.state(), ArmedState::Blocked(_)) => {
                self.create_gate.reset();
            }
            _ => {}
        }
    }

    /// The field ⏎ on a blocked gate goes to: the first invalid one, else
    /// the first placeholder.
    fn first_blocker(&self) -> Option<super::FieldId> {
        self.validation_errors()
            .first()
            .map(|(f, _)| *f)
            .or_else(|| lint::placeholders(&self.payload).first().map(|p| p.field))
    }

    /// The exact write the armed bar shows.
    pub fn create_write_description(&self) -> String {
        format!(
            "creates 1 issue on GitHub · labels {}",
            self.render_labels().join(", ")
        )
    }

    pub(super) fn handle_preview(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter => {
                self.refresh_gate();
                if let Some(field) = self.first_blocker() {
                    self.touched.insert(field);
                    self.focus_field(field);
                    return;
                }
                let now = self.now();
                if self.create_gate.on_enter(now) == EnterOutcome::Commit {
                    // The dispatch layer's `tick_wizard_step_hooks` queues
                    // `TuiCommand::CreateIssue` once the wizard is `Creating`.
                    self.begin_create();
                }
            }
            KeyCode::Esc if matches!(self.create_gate.state(), ArmedState::Armed(_)) => {
                self.create_gate.disarm();
            }
            KeyCode::Esc => {
                self.create_gate.disarm();
                self.retreat();
            }
            _ => self.create_gate.disarm(),
        }
    }
}
