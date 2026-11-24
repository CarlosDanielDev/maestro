//! #1282 Create gate on Preview: the only place the wizard starts a GitHub
//! write. Blocked while `lint::placeholders` finds a `[TODO:`; then the
//! first ⏎ arms (`ds::armed::ArmedEnter`) and a second ⏎ within 3s calls
//! `begin_create`, which stays the only path to `TuiCommand::CreateIssue`.

use super::{IssueWizardScreen, lint};
use crate::tui::ds::armed::{ArmedEnter, ArmedState, EnterOutcome};
use crossterm::event::KeyCode;

impl IssueWizardScreen {
    pub fn create_gate(&self) -> &ArmedEnter {
        &self.create_gate
    }

    /// Block the gate while placeholders are left; unblock once they are
    /// gone. Called on entering Preview and before every Enter there.
    pub(super) fn refresh_gate(&mut self) {
        let left = lint::placeholders(&self.payload).len();
        if left > 0 {
            let s = if left == 1 { "" } else { "s" };
            let reason = format!("{left} [TODO: placeholder{s} left · ⏎ jumps to the first");
            if *self.create_gate.state() != ArmedState::Blocked(reason.clone()) {
                self.create_gate.block(reason);
            }
        } else if matches!(self.create_gate.state(), ArmedState::Blocked(_)) {
            self.create_gate.reset();
        }
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
                if let Some(first) = lint::placeholders(&self.payload).first() {
                    let field = first.field;
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
