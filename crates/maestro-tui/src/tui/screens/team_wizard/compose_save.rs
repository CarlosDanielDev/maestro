//! Compose › Review & save (#1286): the name field (`widgets::input`), the
//! tier, and the armed write. The first ⏎ arms and the screen shows the exact
//! path and TOML; a second ⏎ within `ARM_WINDOW` writes that text. Esc or any
//! edit disarms. Saved / Failed are states of the write bar.

use super::types::{ComposeSource, ComposeStep, ComposeTier, role_label};
use super::{ScreenAction, TeamWizardMode, TeamWizardScreen};
use crate::orchestration::loader::Loader;
use crate::orchestration::team::{ResolvedTeam, RoleBinding, TeamConfig};
use crate::orchestration::types::TeamRole;
use crate::tui::ds::armed::{ArmedState, EnterOutcome};
use crossterm::event::{Event, KeyCode};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// Longest preset name (the loader's cap).
pub(super) const NAME_MAX: usize = 64;

impl TeamWizardScreen {
    pub(super) fn handle_compose_save(&mut self, event: &Event, code: KeyCode) -> ScreenAction {
        match code {
            KeyCode::Esc if matches!(self.compose.save.state(), ArmedState::Armed(_)) => {
                self.compose.save.disarm();
            }
            KeyCode::Esc => return self.handle_compose_back(),
            KeyCode::Enter if *self.compose.save.state() == ArmedState::Done => {
                return self.leave_compose_after_save();
            }
            KeyCode::Enter => self.compose_save_enter(),
            _ if *self.compose.save.state() == ArmedState::Done => {}
            KeyCode::Tab => {
                self.compose.save.reset();
                self.compose.tier = match self.compose.tier {
                    ComposeTier::User => ComposeTier::Project,
                    ComposeTier::Project => ComposeTier::User,
                };
            }
            _ => {
                self.compose.save.reset();
                self.name_input.input(event.clone());
                self.compose.name = self.name_input.text();
            }
        }
        ScreenAction::None
    }

    /// Saved: back to Manage when editing from there, else pop the wizard.
    fn leave_compose_after_save(&mut self) -> ScreenAction {
        if self.editing_existing {
            self.editing_existing = false;
            self.switch_mode(TeamWizardMode::Manage);
            return ScreenAction::None;
        }
        ScreenAction::Pop
    }

    /// ⏎: arm, or write when armed. A bad name never arms; a failed write
    /// re-arms on the next ⏎ (retry).
    fn compose_save_enter(&mut self) {
        if self.validate_save_name().is_some() {
            return;
        }
        if matches!(self.compose.save.state(), ArmedState::Failed(_)) {
            self.compose.save.reset();
        }
        if self.compose.save.on_enter(self.now()) == EnterOutcome::Commit {
            let result = self.persist_compose();
            self.compose.save.finish(result);
        }
    }

    pub(super) fn now(&self) -> Instant {
        self.clock.unwrap_or_else(Instant::now)
    }

    pub fn compose_save_state(&self) -> &ArmedState {
        self.compose.save.state()
    }

    pub(super) fn validate_save_name(&self) -> Option<&'static str> {
        let n = self.compose.name.trim();
        if n.is_empty() {
            return Some("Name cannot be empty");
        }
        if n.len() > NAME_MAX {
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

    /// The directory the chosen tier writes to.
    pub(super) fn compose_target_dir(&self) -> Result<PathBuf, String> {
        match self.compose.tier {
            ComposeTier::User => self
                .user_dir
                .clone()
                .ok_or_else(|| "cannot determine user config dir".to_string()),
            ComposeTier::Project => self
                .project_root
                .as_deref()
                .map(Loader::project_tier_default)
                .ok_or_else(|| "cannot determine the project directory".to_string()),
        }
    }

    fn compose_team_config(&self) -> Result<TeamConfig, String> {
        let primitive = self
            .compose
            .primitive
            .ok_or_else(|| "primitive not set".to_string())?;
        let extends = match &self.compose.source {
            None | Some(ComposeSource::Blank) => String::new(),
            Some(ComposeSource::Extends(name)) => name.clone(),
        };
        let bindings = self
            .compose
            .bindings
            .iter()
            .map(|(role, agent)| {
                (
                    role_label(*role).to_string(),
                    toml::Value::String(agent.clone()),
                )
            })
            .collect();
        Ok(TeamConfig {
            extends,
            primitive: Some(primitive),
            min_agents: Some(vec!["claude".to_string()]),
            bindings,
            role_overrides: HashMap::new(),
        })
    }

    /// The exact file and text the write will produce. Keys are sorted
    /// (`toml::Table`), so what is shown is byte-for-byte what is written.
    pub fn compose_preview(&self) -> Result<(PathBuf, String), String> {
        let cfg = self.compose_team_config()?;
        let value = toml::Value::try_from(&cfg).map_err(|e| e.to_string())?;
        let text = toml::to_string_pretty(&value).map_err(|e| e.to_string())?;
        let name = self.compose.name.trim();
        Ok((
            self.compose_target_dir()?.join(format!("{name}.toml")),
            text,
        ))
    }

    /// Write the previewed TOML. Synchronous: both tiers are local dirs. The
    /// name is re-checked here so no path can escape the tier directory.
    fn persist_compose(&mut self) -> Result<(), String> {
        if let Some(reason) = self.validate_save_name() {
            return Err(reason.to_string());
        }
        let primitive = self
            .compose
            .primitive
            .ok_or_else(|| "primitive not set".to_string())?;
        let (path, text) = self.compose_preview()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("creating {dir:?}: {e}"))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("writing {path:?}: {e}"))?;

        // The just-saved preset is fully described locally; no re-read.
        let bindings: HashMap<TeamRole, RoleBinding> = self
            .compose
            .bindings
            .iter()
            .map(|(role, agent)| {
                (
                    *role,
                    RoleBinding {
                        agent: agent.clone(),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let name = self.compose.name.trim().to_string();
        let resolved = ResolvedTeam {
            name: name.clone(),
            primitive,
            min_agents: vec!["claude".to_string()],
            bindings,
            source_tier: self.compose.tier.as_source_tier(),
        };
        self.resolved_teams.insert(name, resolved);
        Ok(())
    }

    /// Entering or leaving Review & save starts the write bar fresh.
    pub(super) fn set_compose_step(&mut self, step: ComposeStep) {
        self.compose_step = step;
        self.compose.save.reset();
    }
}
