//! #1411: Settings › Providers (the Agents tab) offers `bypassPermissions`
//! per agent. Choosing it goes through the same typed-CONFIRM warning as the
//! Sessions toggle (#1239), and a Ctrl+S save never turns bypass on by
//! itself. Every key goes through the real `handle_key` path and the screen
//! is drawn at 100x30 (above the 80x24 SizeGuard minimum).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::paths_tests::{chip_drawn, last_bypass_log};
use crate::config::{Config, LoadedConfig};
use crate::session::worktree::MockWorktreeManager;
use crate::setup::setup_app_for_agent;
use crate::state::store::StateStore;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::screens::settings::schema_tab::widgets::dynamic_map::MapFocus;
use crate::tui::screens::settings::{SettingsScreen, SettingsTab};
use crate::tui::ui;
use crate::tui::widgets::WidgetKind;

const BYPASS: &str = "bypassPermissions";
/// The Settings warning's own scope line (#1239), not the ⌃B one.
const SETTINGS_SCOPE: &str = "maestro.toml (committed)";

fn toml_body(claude_mode: &str) -> String {
    format!(
        r#"[project]
repo = "owner/repo"
base_branch = "main"
[sessions]
permission_mode = "default"
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[notifications]
[agents]
default = "claude"
[agents.claude]
kind = "claude"
command = "claude"
permission_mode = "{claude_mode}"
"#
    )
}

/// The App starts from a config whose agent says `default`; the open
/// Settings screen holds `screen_mode` for the agent (as if read from a
/// file the user did not write) and saves to a temp `maestro.toml`.
pub(super) fn settings_app(screen_mode: &str) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("maestro.toml");
    std::fs::write(&path, toml_body(screen_mode)).expect("write maestro.toml");
    let start: Config = toml::from_str(&toml_body("default")).expect("config parses");
    let mut app = setup_app_for_agent(
        LoadedConfig {
            config: start,
            path: path.clone(),
        },
        StateStore::new(dir.path().join("state-1411.json")),
        Box::new(MockWorktreeManager::new()),
        None,
        false,
        true,
        None,
    );
    app.show_mascot = false;
    app.gh_auth_ok = true;
    app.no_splash = true;
    let config: Config = toml::from_str(&toml_body(screen_mode)).expect("config parses");
    let mut screen = SettingsScreen::new(config, app.flags.clone()).with_config_path(path);
    screen.jump_to_tab(SettingsTab::Agents);
    app.screen_state.settings_screen = Some(screen);
    app.tui_mode = TuiMode::Settings;
    (app, dir)
}

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

pub(super) async fn ctrl_s(app: &mut App) {
    handle_key(
        app,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
    )
    .await;
}

pub(super) async fn type_confirm(app: &mut App) {
    for c in "CONFIRM".chars() {
        handle_key(app, KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)).await;
    }
    press(app, KeyCode::Enter).await;
}

fn drawn(app: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    t.draw(|f| ui::draw(f, app)).expect("draw");
    let buf = t.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// The agent's Permission Mode dropdown as the widget holds it, and
/// whether the map's focus sits on it.
fn agent_dropdown(app: &App) -> (String, bool) {
    let screen = app.screen_state.settings_screen.as_ref().expect("settings");
    let map = (screen.fields_per_tab[7].iter())
        .find_map(|f| match &f.widget {
            WidgetKind::DynamicMap(m) => Some(m),
            _ => None,
        })
        .expect("agents map");
    let entry = map.active_entry().expect("claude entry");
    let idx = (entry.fields.iter())
        .position(|f| f.widget.label().ends_with(".permission_mode"))
        .expect("permission_mode field");
    let WidgetKind::Dropdown(d) = &entry.fields[idx].widget else {
        panic!("permission_mode is a dropdown");
    };
    let focused = *map.focus() == MapFocus::EntryField(idx);
    (d.selected_value().to_string(), focused)
}

fn staged_agent_mode(app: &App) -> Option<String> {
    let screen = app.screen_state.settings_screen.as_ref().expect("settings");
    screen.config.agents.entries["claude"]
        .permission_mode
        .clone()
}

/// Walk focus down to the agent's Permission Mode row, then press → until
/// it reads `bypassPermissions` or the warning opens.
pub(super) async fn choose_agent_bypass(app: &mut App) {
    for _ in 0..40 {
        if agent_dropdown(app).1 {
            break;
        }
        press(app, KeyCode::Down).await;
    }
    assert!(agent_dropdown(app).1, "focus reached Permission Mode");
    for _ in 0..6 {
        if app.tui_mode == TuiMode::BypassWarning || agent_dropdown(app).0 == BYPASS {
            break;
        }
        press(app, KeyCode::Right).await;
    }
}

#[tokio::test]
async fn settings_agents_bypass_requires_typed_confirm() {
    let (mut app, _dir) = settings_app("default");
    choose_agent_bypass(&mut app).await;
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
    let screen = drawn(&mut app);
    assert!(
        screen.contains("enable bypass for every future"),
        "{screen}"
    );
    assert!(
        screen.contains(SETTINGS_SCOPE),
        "same scope text as #1239: {screen}"
    );
    assert!(!app.bypass_active);

    press(&mut app, KeyCode::Esc).await;

    assert_eq!(app.tui_mode, TuiMode::Settings);
    let screen = drawn(&mut app);
    assert!(!screen.contains(SETTINGS_SCOPE), "{screen}");
    assert!(
        !screen.contains(BYPASS),
        "the old value is drawn back: {screen}"
    );
    assert_eq!(agent_dropdown(&app).0, "default", "old value back");
    assert_eq!(staged_agent_mode(&app).as_deref(), Some("default"));
    assert!(!app.bypass_active);
    assert!(!app.bypass_warning_acknowledged);

    // Nothing staged: a save after the cancel writes no bypass.
    ctrl_s(&mut app).await;
    assert!(!app.bypass_active);
    assert_eq!(app.pool.permission_mode(), "default");
}

#[tokio::test]
async fn settings_save_never_enables_bypass_without_confirm() {
    // The screen already holds an agent bypass that no CONFIRM in this
    // flow answered (for example a file from a cloned repo).
    let (mut app, _dir) = settings_app(BYPASS);
    ctrl_s(&mut app).await;

    assert!(!app.bypass_active, "a save alone must not turn bypass on");
    assert!(!app.bypass_warning_acknowledged);
    assert_eq!(
        app.pool.permission_mode(),
        "default",
        "pool agrees with chip"
    );
    assert!(!chip_drawn(&mut app));
    assert!(last_bypass_log(&app).is_none(), "no enable line");
    let refused = (app.activity_log.entries().iter())
        .any(|e| e.message.contains("typed CONFIRM") && e.message.contains("not turned on"));
    let all: Vec<_> = app
        .activity_log
        .entries()
        .iter()
        .map(|e| e.message.clone())
        .collect();
    assert!(refused, "the save says why bypass stayed off: {all:?}");

    // The typed CONFIRM answered in this save's flow is what turns it on.
    let (mut app, _dir) = settings_app("default");
    choose_agent_bypass(&mut app).await;
    type_confirm(&mut app).await;
    assert_eq!(app.tui_mode, TuiMode::Settings);
    ctrl_s(&mut app).await;

    assert!(app.bypass_active);
    assert!(app.bypass_warning_acknowledged);
    assert_eq!(app.pool.permission_mode(), BYPASS);
    assert!(chip_drawn(&mut app));
    let log = last_bypass_log(&app).expect("enable line");
    assert!(log.contains("settings (typed CONFIRM)"), "got: {log}");
}
