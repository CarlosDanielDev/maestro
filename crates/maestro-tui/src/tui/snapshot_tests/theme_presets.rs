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

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[tui]\nascii_icons = false\n",
    "[tui.theme]\npreset = \"dark\"\n",
);

fn render_theme_preview(preset: ThemePreset) -> String {
    let config: Config = toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML must parse");
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
    assert_snapshot!(render_theme_preview(ThemePreset::Dark));
}

#[test]
fn theme_preview_light() {
    assert_snapshot!(render_theme_preview(ThemePreset::Light));
}

#[test]
fn theme_preview_retro() {
    assert_snapshot!(render_theme_preview(ThemePreset::Retro));
}

#[test]
fn theme_preview_high_contrast() {
    assert_snapshot!(render_theme_preview(ThemePreset::HighContrast));
}

#[test]
fn theme_preview_no_color() {
    assert_snapshot!(render_theme_preview(ThemePreset::NoColor));
}
