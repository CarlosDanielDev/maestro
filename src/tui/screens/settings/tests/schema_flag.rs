//! Flag-wiring tests for `schema_driven_settings`.
//!
//! The flag is registered, parsed, default-off, and produces no observable
//! change in `SettingsScreen` field counts whether on or off — no tab is
//! migrated to the schema-driven renderer in this issue.

use std::collections::HashMap;

use super::make_config;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::tui::screens::settings::SettingsScreen;

#[test]
fn flag_schema_driven_settings_is_in_all_flags() {
    let all = Flag::all();
    assert!(
        all.contains(&Flag::SchemaDrivenSettings),
        "Flag::SchemaDrivenSettings must be in ALL_FLAGS"
    );
}

#[test]
fn flag_schema_driven_settings_default_is_false() {
    assert!(
        !Flag::SchemaDrivenSettings.default_enabled(),
        "default must be false — no behavior change ships in this issue"
    );
}

#[test]
fn flag_schema_driven_settings_name_is_snake_case() {
    assert_eq!(Flag::SchemaDrivenSettings.name(), "schema_driven_settings");
}

#[test]
fn feature_flags_parse_flag_schema_driven_settings_round_trips() {
    let mut config_flags = HashMap::new();
    config_flags.insert("schema_driven_settings".to_string(), true);
    let flags = FeatureFlags::new(config_flags, vec![], vec![]);
    assert!(
        flags.is_enabled(Flag::SchemaDrivenSettings),
        "parse_flag must wire the snake_case key to the variant"
    );
}

#[test]
fn settings_screen_new_flag_off_preserves_existing_tab_count() {
    let flags = FeatureFlags::default();
    let screen = SettingsScreen::new(make_config(), flags);
    assert_eq!(screen.fields_per_tab.len(), 12);
    assert!(
        screen.fields_per_tab[9].is_empty(),
        "Flags tab (index 9) must remain empty"
    );
}

#[test]
fn settings_screen_new_flag_on_produces_same_fields_as_off() {
    let mut flags_on = FeatureFlags::default();
    flags_on.set_enabled(Flag::SchemaDrivenSettings, true);
    let screen_on = SettingsScreen::new(make_config(), flags_on);

    let flags_off = FeatureFlags::default();
    let screen_off = SettingsScreen::new(make_config(), flags_off);

    assert_eq!(
        screen_on.fields_per_tab.len(),
        screen_off.fields_per_tab.len()
    );
    for i in 0..screen_off.fields_per_tab.len() {
        assert_eq!(
            screen_on.fields_per_tab[i].len(),
            screen_off.fields_per_tab[i].len(),
            "tab {i} field count must not change when flag is on"
        );
    }
}
