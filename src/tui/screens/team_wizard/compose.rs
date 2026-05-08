//! Compose flow — Source → Primitive → Roles → Overrides → Save.
//!
//! Step transitions and validation gates. Drawing lives in `draw.rs`.

use super::types::{ComposeSource, ComposeStep, ComposeTier, role_label};
use super::{ScreenAction, TeamWizardMode, TeamWizardScreen};
use crate::orchestration::loader::Loader;
use crate::orchestration::team::TeamConfig;
use crate::orchestration::types::Primitive;
use crossterm::event::KeyCode;
use std::collections::HashMap;

const PRIMITIVES: &[Primitive] = &[
    Primitive::Pipeline,
    Primitive::FanOut,
    Primitive::SinglePass,
    Primitive::VerdictOnly,
];

impl TeamWizardScreen {
    pub(super) fn handle_compose(&mut self, code: KeyCode) -> ScreenAction {
        if matches!(code, KeyCode::Esc) {
            return self.handle_compose_back();
        }
        if matches!(
            (self.compose_step(), code),
            (ComposeStep::SaveSuccess, KeyCode::Enter)
        ) {
            return ScreenAction::Pop;
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
            (ComposeStep::Overrides, KeyCode::Enter) => {
                self.try_advance();
            }
            (ComposeStep::Save, KeyCode::Backspace) => {
                self.compose.name.pop();
            }
            (ComposeStep::Save, KeyCode::Char(c))
                if !c.is_control() && c != ' ' && self.compose.name.len() < 64 =>
            {
                self.compose.name.push(c);
            }
            (ComposeStep::Save, KeyCode::Tab) => self.compose_toggle_tier(),
            (ComposeStep::Save, KeyCode::Enter) => self.compose_attempt_save(),
            (ComposeStep::SaveFailed, KeyCode::Char('r')) => {
                self.compose_step = ComposeStep::Save;
                self.failure_reason = None;
            }
            _ => {}
        }
        ScreenAction::None
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
                let required = primitive.required_roles();
                if required
                    .iter()
                    .all(|r| self.compose.bindings.contains_key(r))
                {
                    None
                } else {
                    Some("Bind every required role")
                }
            }
            ComposeStep::Save => self.validate_save_name(),
            _ => None,
        }
    }

    fn validate_save_name(&self) -> Option<&'static str> {
        let n = self.compose.name.trim();
        if n.is_empty() {
            return Some("Name cannot be empty");
        }
        if n.len() > 64 {
            return Some("Name too long (max 64)");
        }
        if crate::util::validation::validate_slug(n).is_err() {
            return Some("Name must be alphanumeric, hyphen, or underscore");
        }
        if n.starts_with('-') || n.starts_with('_') {
            return Some("Name cannot start with - or _");
        }
        if crate::util::validation::is_windows_reserved_stem(n) {
            return Some("Name is a reserved system filename");
        }
        None
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
        let max = p.required_roles().len().saturating_sub(1);
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
        let required = p.required_roles();
        let Some(role) = required.get(self.compose.role_focus).copied() else {
            return;
        };
        let agents = self.healthy_agents();
        let Some(agent_id) = agents.get(self.compose.agent_focus) else {
            return;
        };
        self.compose.bindings.insert(role, (*agent_id).to_string());
    }

    fn compose_toggle_tier(&mut self) {
        self.compose.tier = match self.compose.tier {
            ComposeTier::User => ComposeTier::Project,
            ComposeTier::Project => ComposeTier::User,
        };
    }

    /// Validate, write the preset TOML to disk, and transition. Synchronous
    /// disk IO is acceptable here because user-tier and project-tier dirs are
    /// always local; on error we transition to `SaveFailed` and surface the
    /// message in the UI rather than panicking.
    fn compose_attempt_save(&mut self) {
        if self.validate_save_name().is_some() {
            return;
        }
        match self.persist_compose() {
            Ok(()) => self.apply_save_result(Ok(())),
            Err(e) => self.apply_save_result(Err(e)),
        }
    }

    fn persist_compose(&mut self) -> Result<(), String> {
        let primitive = self
            .compose
            .primitive
            .ok_or_else(|| "primitive not set".to_string())?;
        let extends = match &self.compose.source {
            None | Some(ComposeSource::Blank) => String::new(),
            Some(ComposeSource::Extends(name)) => name.clone(),
        };

        let mut bindings: HashMap<String, toml::Value> = HashMap::new();
        for (role, agent) in &self.compose.bindings {
            bindings.insert(
                role_label(*role).to_string(),
                toml::Value::String(agent.clone()),
            );
        }
        let team_config = TeamConfig {
            extends,
            primitive: Some(primitive),
            min_agents: Some(vec!["claude".to_string()]),
            bindings,
            role_overrides: HashMap::new(),
        };
        let toml_text = toml::to_string_pretty(&team_config).map_err(|e| e.to_string())?;

        let dir = match self.compose.tier {
            ComposeTier::User => Loader::user_tier_default()
                .ok_or_else(|| "cannot determine user config dir".to_string())?,
            ComposeTier::Project => {
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                Loader::project_tier_default(&cwd)
            }
        };
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let name = self.compose.name.trim();
        let path = dir.join(format!("{name}.toml"));
        std::fs::write(&path, toml_text).map_err(|e| e.to_string())?;

        // Refresh in-memory cache so the same wizard session sees the new
        // preset on subsequent Source / Manage views.
        let user_dir = Loader::user_tier_default();
        let project_dir = std::env::current_dir()
            .ok()
            .map(|p| Loader::project_tier_default(&p));
        let loader = Loader::new(user_dir, project_dir);
        if let Ok(resolved) = loader.resolve() {
            self.apply_resolved_teams(resolved.into_values().collect());
        }
        Ok(())
    }

    pub fn apply_save_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.compose_step = ComposeStep::SaveSuccess;
                self.failure_reason = None;
            }
            Err(e) => {
                self.compose_step = ComposeStep::SaveFailed;
                self.failure_reason = Some(e);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ComposeSourceOption {
    Blank,
    Extends(String),
}

pub(super) const PRIMITIVE_LIST: &[Primitive] = PRIMITIVES;
