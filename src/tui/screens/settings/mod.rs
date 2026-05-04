pub mod caveman_row;
mod draw;
mod input;
mod keymap;
mod tabs;
pub mod types;
pub mod validation;

pub use types::{SettingsField, SettingsScreen, SettingsTab};

use std::collections::HashMap;

use anyhow::Context;

use crate::config::Config;
use crate::flags::store::FeatureFlags;
use crate::settings::CavemanModeState;
use crate::tui::widgets::WidgetKind;

use self::validation::{ValidationFeedback, ValidationSeverity, build_validator_map};

pub(super) const CAVEMAN_LABEL: &str = "caveman_mode";

/// Find a widget in a tab's field slice by its label. Returns the first
/// match. Used by `sync_widgets_to_config` so reordering widgets in
/// `build_*_fields` cannot silently drop a sync.
pub(super) fn widget_by_label<'a>(
    fields: &'a [SettingsField],
    label: &str,
) -> Option<&'a WidgetKind> {
    fields
        .iter()
        .find(|f| f.widget.label() == label)
        .map(|f| &f.widget)
}

impl SettingsScreen {
    pub fn new(config: Config, flags: FeatureFlags) -> Self {
        let fields_per_tab = tabs::build_fields(&config);
        let validators = build_validator_map();
        let mut screen = Self {
            original_config: config.clone(),
            config,
            config_path: None,
            active_tab: 0,
            field_index: 0,
            fields_per_tab,
            scroll_offset: 0,
            confirm_discard: false,
            save_flash: None,
            save_error_flash: None,
            live_preview: false,
            feature_flags: flags,
            flags_selected: 0,
            validators,
            validation_results: HashMap::new(),
            caveman_state: CavemanModeState::Default,
            pending_caveman_toggle: None,
            caveman_status_flash: None,
        };
        screen.run_all_validations();
        screen
    }

    pub fn with_config_path(mut self, path: std::path::PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    pub fn with_caveman_mode(mut self, state: CavemanModeState) -> Self {
        self.set_caveman_state(state);
        self
    }

    pub fn set_caveman_state(&mut self, state: CavemanModeState) {
        let bool_value = state.as_bool().unwrap_or(false);
        self.caveman_state = state;
        if let Some(fields) = self.fields_per_tab.get_mut(11)
            && let Some(field) = fields
                .iter_mut()
                .find(|f| f.widget.label() == CAVEMAN_LABEL)
            && let WidgetKind::Toggle(ref mut toggle) = field.widget
        {
            toggle.value = bool_value;
        }
    }

    pub fn take_pending_caveman_toggle(&mut self) -> Option<bool> {
        self.pending_caveman_toggle.take()
    }

    pub fn show_caveman_status(&mut self, message: impl Into<String>) {
        self.caveman_status_flash = Some((message.into(), std::time::Instant::now()));
    }

    /// Sync the TurboQuant enabled toggle from an external flag change (Ctrl+Q).
    pub fn sync_tq_enabled(&mut self, enabled: bool) {
        self.config.turboquant.enabled = enabled;
        let tq_idx = SettingsTab::ALL
            .iter()
            .position(|t| matches!(t, SettingsTab::TurboQuant));
        if let Some(idx) = tq_idx
            && let Some(fields) = self.fields_per_tab.get_mut(idx)
            && let Some(field) = fields.first_mut()
            && let WidgetKind::Toggle(ref mut toggle) = field.widget
        {
            toggle.value = enabled;
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.config != self.original_config
    }

    fn reset_to_original(&mut self) {
        self.config = self.original_config.clone();
        self.fields_per_tab = tabs::build_fields(&self.config);
    }

    fn save_config(&mut self) -> Result<(), anyhow::Error> {
        let Some(path) = self.config_path.as_ref() else {
            tracing::warn!("Settings save attempted with no config_path resolved");
            anyhow::bail!(
                "No config file resolved — cannot save. Run `maestro init` to create one."
            );
        };
        self.config
            .save(path)
            .with_context(|| format!("saving settings to {}", path.display()))?;
        self.original_config = self.config.clone();
        self.save_flash = Some(std::time::Instant::now());
        self.save_error_flash = None;
        Ok(())
    }

    fn run_all_validations(&mut self) {
        self.validation_results.clear();
        for (&key, validator) in &self.validators {
            let result = validator(&self.config);
            if result.severity != ValidationSeverity::Valid {
                self.validation_results.insert(key, result);
            }
        }
    }

    pub fn has_validation_errors(&self) -> bool {
        self.validation_results.values().any(|v| v.is_error())
    }

    fn validation_error_summary(&self) -> Option<String> {
        let mut errors: Vec<((usize, usize), &ValidationFeedback)> = self
            .validation_results
            .iter()
            .filter(|(_, v)| v.is_error())
            .map(|(k, v)| (*k, v))
            .collect();
        errors.sort_by_key(|&(k, _)| k);
        let first = errors.first()?;
        let ((tab_idx, field_idx), feedback) = *first;
        let tab_label = SettingsTab::ALL
            .get(tab_idx)
            .map(|t| t.label())
            .unwrap_or("?");
        let field_label = self
            .fields_per_tab
            .get(tab_idx)
            .and_then(|fs| fs.get(field_idx))
            .map(|f| f.widget.label())
            .unwrap_or("?");
        let msg = if feedback.message.is_empty() {
            "validation failed"
        } else {
            feedback.message.as_str()
        };
        let mut summary = format!("{}.{}: {}", tab_label, field_label, msg);
        if errors.len() > 1 {
            summary.push_str(&format!(" (+{} more)", errors.len() - 1));
        }
        Some(summary)
    }

    fn feedback_for(&self, tab: usize, field: usize) -> Option<&ValidationFeedback> {
        self.validation_results.get(&(tab, field))
    }

    pub fn active_tab(&self) -> SettingsTab {
        SettingsTab::ALL[self.active_tab]
    }

    fn current_fields(&self) -> &[SettingsField] {
        &self.fields_per_tab[self.active_tab]
    }

    #[allow(dead_code)] // Reason: mutable field access for inline editing
    fn current_fields_mut(&mut self) -> &mut [SettingsField] {
        &mut self.fields_per_tab[self.active_tab]
    }

    fn field_count(&self) -> usize {
        self.current_fields().len()
    }

    fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % SettingsTab::ALL.len();
        self.field_index = 0;
        self.scroll_offset = 0;
        self.flags_selected = 0;
    }

    fn prev_tab(&mut self) {
        self.active_tab = if self.active_tab == 0 {
            SettingsTab::ALL.len() - 1
        } else {
            self.active_tab - 1
        };
        self.field_index = 0;
        self.scroll_offset = 0;
        self.flags_selected = 0;
    }

    fn active_widget_needs_insert(&self) -> bool {
        self.current_fields()
            .get(self.field_index)
            .is_some_and(|f| f.widget.needs_insert_mode())
    }

    /// Synchronize widget values back into the Config struct.
    pub fn sync_widgets_to_config(&mut self) {
        tabs::sync_widgets_to_config(self);
    }
}

#[cfg(test)]
mod tests;
