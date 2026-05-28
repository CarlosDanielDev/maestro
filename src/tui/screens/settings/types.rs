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
    Agents,
    Modes,
    Teams,
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
        Self::Agents,
        Self::Modes,
        Self::Teams,
        Self::Theme,
        Self::Layout,
        Self::Flags,
        Self::TurboQuant,
        Self::Advanced,
    ];

    /// Indices into [`Self::ALL`] in alphabetical-by-VARIANT-NAME order.
    /// Used by the Settings sidebar so the on-screen list is stable while
    /// the canonical enum (and any `active_tab` index already stored or
    /// serialized) is unaffected.
    ///
    /// Note: `Agents.label() == "Providers"` — the sidebar position follows
    /// the variant name, not the rendered label. Pre-existing convention
    /// from before the Agents → Providers label rename.
    pub const ALPHABETICAL_INDICES: &'static [usize] = &[
        14, // Advanced
        7,  // Agents (label "Providers")
        2,  // Budget
        12, // Flags
        5,  // Gates
        3,  // GitHub
        11, // Layout
        8,  // Modes
        4,  // Notifications
        0,  // Project
        6,  // Review
        1,  // Sessions
        9,  // Teams
        10, // Theme
        13, // TurboQuant
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
            Self::Agents => "Providers",
            Self::Modes => "Modes",
            Self::Teams => "Teams",
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
    pub(crate) fields_per_tab: Vec<Vec<SettingsField>>,
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
    /// Substring query that filters the sidebar tab list. Case-insensitive.
    /// Empty = all tabs visible.
    pub(super) sidebar_search: String,
    /// True while the search input owns key events. Toggled by `/` (enter)
    /// and `Esc` / `Enter` (exit). When false, key events route to the
    /// normal settings handler.
    pub(super) sidebar_search_active: bool,
    /// Soft warnings collected from `validate_role_overrides` on each
    /// Save. Surfaced in the Save banner; Save proceeds regardless
    /// (#908 — mirrors the `teams.<id>.extends` validator pattern).
    pub(super) role_override_warnings:
        Vec<crate::orchestration::team_role_overrides::RoleOverrideWarning>,
}
