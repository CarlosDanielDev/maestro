use std::collections::HashMap;

use crate::config::Config;
use crate::flags::store::FeatureFlags;
use crate::settings::CavemanModeState;
use crate::tui::widgets::WidgetKind;

use super::validation::{FieldKey, ValidationFeedback, ValidatorFn};

/// Tab sections in the settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Project,
    Sessions,
    Budget,
    GitHub,
    Notifications,
    Gates,
    Review,
    Theme,
    Layout,
    Flags,
    TurboQuant,
    Advanced,
}

impl SettingsTab {
    pub const ALL: &'static [SettingsTab] = &[
        Self::Project,
        Self::Sessions,
        Self::Budget,
        Self::GitHub,
        Self::Notifications,
        Self::Gates,
        Self::Review,
        Self::Theme,
        Self::Layout,
        Self::Flags,
        Self::TurboQuant,
        Self::Advanced,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Sessions => "Sessions",
            Self::Budget => "Budget",
            Self::GitHub => "GitHub",
            Self::Notifications => "Notifications",
            Self::Gates => "Gates",
            Self::Review => "Review",
            Self::Theme => "Theme",
            Self::Layout => "Layout",
            Self::Flags => "Flags",
            Self::TurboQuant => "TurboQuant",
            Self::Advanced => "Advanced",
        }
    }
}

/// A single field in a settings tab, pairing a label with a widget.
pub struct SettingsField {
    pub widget: WidgetKind,
}

pub struct SettingsScreen {
    pub config: Config,
    pub(super) original_config: Config,
    pub config_path: Option<std::path::PathBuf>,
    pub(super) active_tab: usize,
    pub(super) field_index: usize,
    pub(super) fields_per_tab: Vec<Vec<SettingsField>>,
    pub(super) scroll_offset: usize,
    pub(super) confirm_discard: bool,
    pub(super) save_flash: Option<std::time::Instant>,
    pub(super) save_error_flash: Option<(String, std::time::Instant)>,
    pub live_preview: bool,
    pub(super) feature_flags: FeatureFlags,
    pub(super) flags_selected: usize,
    pub(super) validators: HashMap<FieldKey, ValidatorFn>,
    pub(super) validation_results: HashMap<FieldKey, ValidationFeedback>,
    pub(super) caveman_state: CavemanModeState,
    pub(super) pending_caveman_toggle: Option<bool>,
    pub(super) caveman_status_flash: Option<(String, std::time::Instant)>,
}
