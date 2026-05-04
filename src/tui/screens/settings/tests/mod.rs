use super::*;
use crate::config::Config;
use crate::flags::store::FeatureFlags as TestFeatureFlags;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::KeymapProvider;
use crate::tui::screens::{Screen, ScreenAction, test_helpers::key_event};
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

fn make_flags() -> TestFeatureFlags {
    TestFeatureFlags::default()
}

fn make_config() -> Config {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write;
    write!(f, "{}", MINIMAL_SETTINGS_TOML).unwrap();
    Config::load(f.path()).unwrap()
}

const MINIMAL_SETTINGS_TOML: &str = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n";

/// Construct a `SettingsScreen` backed by a real tempfile so `Ctrl+s`
/// actually writes. The `NamedTempFile` must be kept alive by the caller
/// for the duration of the test — dropping it deletes the backing file.
fn screen_with_config_path() -> (SettingsScreen, tempfile::NamedTempFile) {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(f, "{}", MINIMAL_SETTINGS_TOML).unwrap();
    let config = Config::load(f.path()).unwrap();
    let screen = SettingsScreen::new(config, make_flags()).with_config_path(f.path().to_path_buf());
    (screen, f)
}

mod basic;
mod dirty;
mod flags_validation;
mod integration;
mod keybinds_save_caveman;
