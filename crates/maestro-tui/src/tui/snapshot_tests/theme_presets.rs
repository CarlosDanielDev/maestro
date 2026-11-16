//! Theme preview per preset (#1247): the Settings Theme tab, drawn through
//! `Screen::draw` with each `ThemePreset`. The buffer `Debug` includes styles,
//! so a colour change in any preset shows up as a snapshot diff.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::Config;
use crate::flags::store::FeatureFlags;
use crate::tui::screens::Screen;
use crate::tui::screens::settings::{SettingsScreen, SettingsTab};
use crate::tui::theme::{Theme, ThemeConfig, ThemePreset};

/// Config without `[tui.theme]`; each case appends its own preset so the
/// preview's `preset:` row names the palette it is drawn with.
const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[tui]\nascii_icons = false\n",
);

fn render_theme_preview(preset: ThemePreset, name: &str) -> String {
    let toml = format!("{MINIMAL_TOML}[tui.theme]\npreset = \"{name}\"\n");
    let config: Config = toml::from_str(&toml).expect("preview config must parse");
    assert_eq!(config.tui.theme.preset, preset, "name must match preset");
    let mut screen = SettingsScreen::new(config, FeatureFlags::default());
    screen.jump_to_tab(SettingsTab::Theme);
    let theme = Theme::from_config(&ThemeConfig {
        preset,
        ..Default::default()
    });
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("backend must init");
    terminal
        .draw(|f| screen.draw(f, f.area(), &theme))
        .expect("draw must succeed");
    format!("{:?}", terminal.backend().buffer())
}

#[test]
fn theme_preview_dark() {
    assert_snapshot!(render_theme_preview(ThemePreset::Dark, "dark"));
}

#[test]
fn theme_preview_light() {
    assert_snapshot!(render_theme_preview(ThemePreset::Light, "light"));
}

#[test]
fn theme_preview_retro() {
    assert_snapshot!(render_theme_preview(ThemePreset::Retro, "retro"));
}

#[test]
fn theme_preview_high_contrast() {
    assert_snapshot!(render_theme_preview(
        ThemePreset::HighContrast,
        "high-contrast"
    ));
}

#[test]
fn theme_preview_no_color() {
    assert_snapshot!(render_theme_preview(ThemePreset::NoColor, "no-color"));
}
