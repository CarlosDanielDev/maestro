//! Snapshot tests for inline role_overrides ValidationFeedback (#909).
//!
//! Split from `settings_teams_parity.rs` to stay under the 400-LOC guardrail.
//! These tests render the full `SettingsScreen` via the `Screen::draw` trait
//! method so `draw_fields` builds the warnings-by-label lookup and
//! `DynamicMapWidget::draw_with_warnings` threads it into the nested editor.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::Config;
use crate::flags::store::FeatureFlags;
use crate::orchestration::team::{RoleOverride, TeamConfig};
use crate::orchestration::team_role_overrides::{RoleOverrideField, RoleOverrideWarning};
use crate::orchestration::types::Primitive;
use crate::tui::screens::Screen;
use crate::tui::screens::settings::{SettingsScreen, SettingsTab};
use crate::tui::theme::Theme;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
);

fn base_config() -> Config {
    toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML must parse")
}

fn config_with_role_overrides(agent_value: &str, mode_value: &str, fallback_value: &str) -> Config {
    let mut config = base_config();
    let mut bindings = std::collections::HashMap::new();
    bindings.insert(
        "implementer".to_string(),
        toml::Value::String("claude".to_string()),
    );
    let mut role_overrides = std::collections::HashMap::new();
    role_overrides.insert(
        "reviewer".to_string(),
        RoleOverride {
            agent: Some(agent_value.to_string()),
            mode: Some(mode_value.to_string()),
            model_override: None,
            prompt_addendum: None,
            fallback_agent: Some(fallback_value.to_string()),
        },
    );
    config.teams.insert(
        "worker-pool".to_string(),
        TeamConfig {
            extends: String::new(),
            primitive: Some(Primitive::Pipeline),
            min_agents: Some(vec!["claude".to_string()]),
            bindings,
            role_overrides,
        },
    );
    config
}

/// Render the Teams tab through `SettingsScreen::draw` (the `Screen` trait
/// method) so the new build_role_override_lookup → draw_with_warnings path
/// is exercised. The existing `render_tab` helper in
/// `settings_teams_parity.rs` bypasses `draw_fields` and cannot be reused.
fn render_settings_teams_screen(
    screen: &mut SettingsScreen,
    width: u16,
    height: u16,
) -> ratatui::buffer::Buffer {
    screen.jump_to_tab(SettingsTab::Teams);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend must init");
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .expect("draw must succeed");
    terminal.backend().buffer().clone()
}

#[test]
fn nested_role_overrides_one_invalid_renders_inline_warning_80x24() {
    // Inject one Agent warning for the reviewer role. The buffer
    // snapshot is the regression guard — the inline rendering of
    // `unknown agent` next to the TextInput is verified by the unit
    // test `draw_with_warnings_passes_lookup_to_text_input` in
    // `dynamic_map_draw.rs::tests`. Here we drive the full
    // `SettingsScreen::draw` path so the build_role_override_lookup
    // → draw_with_warnings plumbing is end-to-end covered (#909).
    let config = config_with_role_overrides("nonexistent-agent", "review-strict", "claude");
    let mut screen = SettingsScreen::new(config, FeatureFlags::default());
    screen.set_role_override_warnings_for_test(vec![RoleOverrideWarning {
        team_id: "worker-pool".to_string(),
        role_id: "reviewer".to_string(),
        field: RoleOverrideField::Agent,
        value: "nonexistent-agent".to_string(),
    }]);

    let buf = render_settings_teams_screen(&mut screen, 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn nested_role_overrides_all_valid_renders_no_warnings_80x24() {
    // No warnings injected. The rendered buffer must not contain any
    // `unknown agent` / `unknown mode` artefacts.
    let config = config_with_role_overrides("claude", "review-strict", "claude");
    let mut screen = SettingsScreen::new(config, FeatureFlags::default());

    let buf = render_settings_teams_screen(&mut screen, 80, 24);
    let rendered = format!("{buf:?}");
    assert!(
        !rendered.contains("unknown agent") && !rendered.contains("unknown mode"),
        "clean render must not contain warning text, got:\n{rendered}",
    );
    assert_snapshot!(rendered);
}

#[test]
fn nested_role_overrides_empty_value_emits_no_inline_warning_80x24() {
    // Role overrides with empty-string values inherit from bindings —
    // `validate_role_overrides` emits no warnings for these, and the
    // renderer must not paint any warning text either.
    let config = config_with_role_overrides("", "", "");
    let mut screen = SettingsScreen::new(config, FeatureFlags::default());

    let buf = render_settings_teams_screen(&mut screen, 80, 24);
    let rendered = format!("{buf:?}");
    assert!(
        !rendered.contains("unknown"),
        "empty-value render must contain no `unknown` warning text, got:\n{rendered}",
    );
    assert_snapshot!(rendered);
}
