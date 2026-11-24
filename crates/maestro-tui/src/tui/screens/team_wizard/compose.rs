//! Compose flow — Start from → Shape → Roles → Review & save (#1286).
//!
//! Step transitions and validation gates. The armed write lives in
//! `compose_save.rs`; drawing in `draw_compose.rs`.

use super::types::{ComposeSource, ComposeStep, roles_to_bind};
use super::{ScreenAction, TeamWizardMode, TeamWizardScreen};
use crate::orchestration::types::Primitive;
use crossterm::event::{Event, KeyCode};

const PRIMITIVES: &[Primitive] = &[
    Primitive::Pipeline,
    Primitive::FanOut,
    Primitive::SinglePass,
    Primitive::VerdictOnly,
];

impl TeamWizardScreen {
    pub(super) fn handle_compose(&mut self, event: &Event, code: KeyCode) -> ScreenAction {
        if self.compose_step() == ComposeStep::Save {
            return self.handle_compose_save(event, code);
        }
        if matches!(code, KeyCode::Esc) {
            return self.handle_compose_back();
        }
        match (self.compose_step(), code) {
            (ComposeStep::Source, KeyCode::Up | KeyCode::Char('k')) => self.compose_source_dec(),
            (ComposeStep::Source, KeyCode::Down | KeyCode::Char('j')) => self.compose_source_inc(),
            (ComposeStep::Source, KeyCode::Enter) => self.compose_commit_source(),
            (ComposeStep::Primitive, KeyCode::Up | KeyCode::Char('k')) => {
                self.compose_primitive_dec()
            }
            (ComposeStep::Primitive, KeyCode::Down | KeyCode::Char('j')) => {
                self.compose_primitive_inc()
            }
            (ComposeStep::Primitive, KeyCode::Enter) => self.compose_commit_primitive(),
            (ComposeStep::Roles, KeyCode::Up | KeyCode::Char('k')) => self.compose_role_focus_dec(),
            (ComposeStep::Roles, KeyCode::Down | KeyCode::Char('j')) => {
                self.compose_role_focus_inc()
            }
            (ComposeStep::Roles, KeyCode::Left | KeyCode::Char('h')) => {
                self.compose_agent_focus_dec()
            }
            (ComposeStep::Roles, KeyCode::Right | KeyCode::Char('l')) => {
                self.compose_agent_focus_inc()
            }
            (ComposeStep::Roles, KeyCode::Char(' ')) => self.compose_bind_focused_role(),
            (ComposeStep::Roles, KeyCode::Enter) => {
                self.try_advance();
            }
            _ => {}
        }
        ScreenAction::None
    }

    /// True when the chosen shape has a role to bind (Roles is skipped
    /// otherwise). No shape yet: keep the step.
    pub(super) fn compose_has_roles(&self) -> bool {
        self.compose
            .primitive
            .is_none_or(|p| !roles_to_bind(p).is_empty())
    }

    pub(super) fn handle_compose_back(&mut self) -> ScreenAction {
        if self.compose_step.is_first() {
            self.switch_mode(TeamWizardMode::Home);
        } else {
            self.retreat();
        }
        ScreenAction::None
    }

    pub(super) fn validate_compose_step(&self) -> Option<&'static str> {
        match self.compose_step {
            ComposeStep::Source => {
                if self.compose.source.is_some() {
                    None
                } else {
                    Some("Pick Blank or an existing preset")
                }
            }
            ComposeStep::Primitive => {
                if self.compose.primitive.is_some() {
                    None
                } else {
                    Some("Select a primitive")
                }
            }
            ComposeStep::Roles => {
                let primitive = self.compose.primitive?;
                if roles_to_bind(primitive)
                    .iter()
                    .all(|r| self.compose.bindings.contains_key(r))
                {
                    None
                } else {
                    Some("Bind every required role")
                }
            }
            ComposeStep::Save => self.validate_save_name(),
        }
    }

    fn compose_source_inc(&mut self) {
        let max = self.compose_source_options().len().saturating_sub(1);
        if self.compose.source_focus < max {
            self.compose.source_focus += 1;
        }
    }

    fn compose_source_dec(&mut self) {
        self.compose.source_focus = self.compose.source_focus.saturating_sub(1);
    }

    pub(super) fn compose_source_options(&self) -> Vec<ComposeSourceOption> {
        let mut out = vec![ComposeSourceOption::Blank];
        let mut names: Vec<&str> = self.resolved_teams.keys().map(String::as_str).collect();
        names.sort();
        for n in names {
            out.push(ComposeSourceOption::Extends(n.to_string()));
        }
        out
    }

    fn compose_commit_source(&mut self) {
        let opts = self.compose_source_options();
        if let Some(opt) = opts.get(self.compose.source_focus) {
            self.compose.source = Some(match opt {
                ComposeSourceOption::Blank => ComposeSource::Blank,
                ComposeSourceOption::Extends(name) => ComposeSource::Extends(name.clone()),
            });
            self.try_advance();
        }
    }

    fn compose_primitive_inc(&mut self) {
        let max = PRIMITIVES.len().saturating_sub(1);
        if self.compose.primitive_focus < max {
            self.compose.primitive_focus += 1;
        }
    }

    fn compose_primitive_dec(&mut self) {
        self.compose.primitive_focus = self.compose.primitive_focus.saturating_sub(1);
    }

    fn compose_commit_primitive(&mut self) {
        if let Some(p) = PRIMITIVES.get(self.compose.primitive_focus) {
            self.compose.primitive = Some(*p);
            self.try_advance();
        }
    }

    fn compose_role_focus_inc(&mut self) {
        let Some(p) = self.compose.primitive else {
            return;
        };
        let max = roles_to_bind(p).len().saturating_sub(1);
        if self.compose.role_focus < max {
            self.compose.role_focus += 1;
            self.compose.agent_focus = 0;
        }
    }

    fn compose_role_focus_dec(&mut self) {
        if self.compose.role_focus > 0 {
            self.compose.role_focus -= 1;
            self.compose.agent_focus = 0;
        }
    }

    fn compose_agent_focus_inc(&mut self) {
        let agents = self.healthy_agents();
        let max = agents.len().saturating_sub(1);
        if self.compose.agent_focus < max {
            self.compose.agent_focus += 1;
        }
    }

    fn compose_agent_focus_dec(&mut self) {
        self.compose.agent_focus = self.compose.agent_focus.saturating_sub(1);
    }

    fn compose_bind_focused_role(&mut self) {
        let Some(p) = self.compose.primitive else {
            return;
        };
        let roles = roles_to_bind(p);
        let Some(role) = roles.get(self.compose.role_focus).copied() else {
            return;
        };
        let agents = self.healthy_agents();
        let Some(agent_id) = agents.get(self.compose.agent_focus) else {
            return;
        };
        self.compose.bindings.insert(role, (*agent_id).to_string());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ComposeSourceOption {
    Blank,
    Extends(String),
}

pub(super) const PRIMITIVE_LIST: &[Primitive] = PRIMITIVES;

/// The step after `step`, skipping Roles when there is nothing to bind.
pub(super) fn next_compose_step(step: ComposeStep, has_roles: bool) -> Option<ComposeStep> {
    match step.next()? {
        ComposeStep::Roles if !has_roles => ComposeStep::Roles.next(),
        next => Some(next),
    }
}

/// The step before `step`, skipping Roles when there is nothing to bind.
pub(super) fn previous_compose_step(step: ComposeStep, has_roles: bool) -> Option<ComposeStep> {
    match step.previous()? {
        ComposeStep::Roles if !has_roles => ComposeStep::Roles.previous(),
        prev => Some(prev),
    }
}
