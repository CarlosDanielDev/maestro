//! Manage flow — list user-tier presets, edit (jumps to Compose), delete.

use super::types::{ManageStep, filter_by_tier};
use super::{ScreenAction, TeamWizardMode, TeamWizardScreen};
use crate::orchestration::team::{ResolvedTeam, SourceTier};
use crossterm::event::KeyCode;

impl TeamWizardScreen {
    pub(super) fn handle_manage(&mut self, code: KeyCode) -> ScreenAction {
        if matches!(code, KeyCode::Esc) {
            return self.handle_manage_back();
        }
        match (self.manage_step(), code) {
            (ManageStep::List, KeyCode::Up) => self.manage_focus_dec(),
            (ManageStep::List, KeyCode::Down) => self.manage_focus_inc(),
            (ManageStep::List, KeyCode::Char('e')) => self.manage_jump_to_edit(),
            (ManageStep::List, KeyCode::Char('d')) => self.manage_open_delete_confirm(),
            (ManageStep::DeleteConfirm, KeyCode::Char('y')) => self.manage_attempt_delete(),
            (ManageStep::DeleteConfirm, KeyCode::Char('n')) => self.manage_cancel_delete(),
            (ManageStep::DeleteSuccess, KeyCode::Enter) => self.manage_after_delete(),
            (ManageStep::DeleteFailed, KeyCode::Char('r')) => self.manage_retry_delete(),
            _ => {}
        }
        ScreenAction::None
    }

    pub(super) fn handle_manage_back(&mut self) -> ScreenAction {
        if self.manage_step.is_first() {
            self.switch_mode(TeamWizardMode::Home);
        } else {
            self.manage.pending_delete = None;
            self.manage_step = ManageStep::List;
        }
        ScreenAction::None
    }

    /// User-tier presets only. Built-ins and project-tier are read-only.
    pub fn manage_list_teams(&self) -> Vec<&ResolvedTeam> {
        filter_by_tier(&self.resolved_teams, SourceTier::User)
    }

    fn manage_focus_inc(&mut self) {
        let max = self.manage_list_teams().len().saturating_sub(1);
        if self.manage.selected_index < max {
            self.manage.selected_index += 1;
        }
    }

    fn manage_focus_dec(&mut self) {
        self.manage.selected_index = self.manage.selected_index.saturating_sub(1);
    }

    fn manage_jump_to_edit(&mut self) {
        let teams = self.manage_list_teams();
        let Some(target) = teams.get(self.manage.selected_index) else {
            return;
        };
        let parent_name = target.name.clone();
        self.jump_to_edit(&parent_name);
    }

    fn manage_open_delete_confirm(&mut self) {
        let name = {
            let teams = self.manage_list_teams();
            let Some(t) = teams.get(self.manage.selected_index) else {
                return;
            };
            t.name.clone()
        };
        self.manage.pending_delete = Some(name);
        self.manage_step = ManageStep::DeleteConfirm;
    }

    fn manage_cancel_delete(&mut self) {
        self.manage.pending_delete = None;
        self.manage_step = ManageStep::List;
    }

    fn manage_attempt_delete(&mut self) {
        self.manage_step = ManageStep::DeleteSuccess;
    }

    fn manage_after_delete(&mut self) {
        if let Some(name) = self.manage.pending_delete.take() {
            self.resolved_teams.remove(&name);
            self.manage.selected_index = 0;
        }
        self.manage_step = ManageStep::List;
    }

    fn manage_retry_delete(&mut self) {
        if self.manage.pending_delete.is_some() {
            self.manage_step = ManageStep::DeleteConfirm;
            self.manage.last_error = None;
        }
    }

    pub fn apply_delete_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.manage_step = ManageStep::DeleteSuccess;
                self.manage.last_error = None;
            }
            Err(e) => {
                self.manage_step = ManageStep::DeleteFailed;
                self.manage.last_error = Some(e);
            }
        }
    }
}
