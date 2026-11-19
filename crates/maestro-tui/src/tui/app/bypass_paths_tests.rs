//! #1411: every path that sets the session permission mode (start,
//! `maestro run`, a mode's own `permission_mode`) shows `▲ BYPASS` and logs
//! the source when the result is bypass. Each App comes from the real setup
//! function and the status bar is drawn at 100x30 (above the 80x24
//! SizeGuard minimum).

use ratatui::{Terminal, backend::TestBackend};

use crate::config::{Config, LoadedConfig};
use crate::session::types::Session;
use crate::session::worktree::MockWorktreeManager;
use crate::setup::setup_app_for_agent;
use crate::state::store::StateStore;
use crate::tui::app::App;
use crate::tui::ui;

const BYPASS: &str = "bypassPermissions";

/// `[sessions]` says `default`. `claude` is the default agent; `worker` is
/// a second agent that `maestro run --agent worker` can pick. The command
/// is not a built-in binary, so an untrusted workspace never spawns it.
fn config(claude_mode: &str, worker_mode: &str, mode_bypass: bool) -> Config {
    toml::from_str(&config_body(claude_mode, worker_mode, mode_bypass)).expect("test config parses")
}

fn config_body(claude_mode: &str, worker_mode: &str, mode_bypass: bool) -> String {
    let modes = if mode_bypass {
        "[modes.yolo]\npermission_mode = \"bypassPermissions\"\n"
    } else {
        ""
    };
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
command = "/nonexistent/agent-1411"
permission_mode = "{claude_mode}"
[agents.worker]
kind = "claude"
command = "/nonexistent/agent-1411"
permission_mode = "{worker_mode}"
{modes}"#
    )
}

/// The App the way `maestro run` builds it for `agent` (`None` = start).
fn run_app(config: Config, bypass_review: bool, agent: Option<&str>) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut app = setup_app_for_agent(
        LoadedConfig {
            config,
            path: dir.path().join("maestro.toml"),
        },
        StateStore::new(dir.path().join("state-1411.json")),
        Box::new(MockWorktreeManager::new()),
        None,
        bypass_review,
        false,
        agent,
    );
    app.show_mascot = false;
    app.gh_auth_ok = true;
    app.no_splash = true;
    (app, dir)
}

pub(super) fn chip_drawn(app: &mut App) -> bool {
    let mut t = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    t.draw(|f| ui::draw(f, app)).expect("draw");
    let buf = t.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
    }
    out.contains("▲ BYPASS")
}

pub(super) fn last_bypass_log(app: &App) -> Option<String> {
    (app.activity_log.entries().iter().rev())
        .find(|e| e.message.starts_with("Bypass mode enabled"))
        .map(|e| e.message.clone())
}

#[test]
fn run_path_bypass_sets_chip_and_logs_source() {
    // The run's agent asks for bypass, `[sessions]` does not.
    let (mut app, _dir) = run_app(config("default", BYPASS, false), false, Some("worker"));
    assert_eq!(app.pool.permission_mode(), BYPASS);
    assert!(app.bypass_active);
    assert!(chip_drawn(&mut app), "▲ BYPASS must be drawn");
    let log = last_bypass_log(&app).expect("a [BYPASS] log line");
    assert!(log.contains("(agent worker)"), "got: {log}");
    assert!(!app.bypass_warning_acknowledged);

    // `--bypass-review` wins over an agent mode of `default`.
    let (mut app, _dir) = run_app(config("default", "default", false), true, Some("worker"));
    assert_eq!(app.pool.permission_mode(), BYPASS, "pool stays in bypass");
    assert!(app.bypass_active);
    assert!(chip_drawn(&mut app));
    let log = last_bypass_log(&app).expect("a [BYPASS] log line");
    assert!(log.contains("(cli)"), "got: {log}");

    // The run's agent says `default` while the default agent says bypass:
    // the run's agent is the one sessions get.
    let (mut app, _dir) = run_app(config(BYPASS, "default", false), false, Some("worker"));
    assert_eq!(app.pool.permission_mode(), "default");
    assert!(!app.bypass_active);
    assert!(!chip_drawn(&mut app));
}

/// A session whose mode carries `permission_mode = "bypassPermissions"`.
fn yolo_session(config: &Config) -> Session {
    let mode_config = crate::modes::resolve_session_mode_config("yolo", Some(config));
    Session::new("p".into(), "opus".into(), "yolo".into(), Some(7), None)
        .with_mode_config(mode_config)
}

#[tokio::test]
async fn bypass_chip_matches_pool_mode_on_every_path() {
    // Start, no agent picked: the default agent's mode.
    for (claude_mode, on) in [("default", false), (BYPASS, true)] {
        let (mut app, _dir) = run_app(config(claude_mode, "default", false), false, None);
        assert_eq!(
            app.pool.permission_mode() == BYPASS,
            on,
            "start {claude_mode}"
        );
        assert_eq!(app.bypass_active, on, "start {claude_mode}");
        assert_eq!(chip_drawn(&mut app), on, "start {claude_mode}");
    }
    let (app, _dir) = run_app(config(BYPASS, "default", false), false, None);
    let log = last_bypass_log(&app).expect("start logs its source");
    assert!(log.contains("(agent claude)"), "got: {log}");

    // `maestro run` setup, every agent/flag mix.
    for (worker_mode, flag) in [("default", false), (BYPASS, false), ("default", true)] {
        let (mut app, _dir) = run_app(config("default", worker_mode, false), flag, Some("worker"));
        let pool_bypass = app.pool.permission_mode() == BYPASS;
        assert_eq!(
            app.bypass_active, pool_bypass,
            "run {worker_mode} flag={flag}"
        );
        assert_eq!(
            chip_drawn(&mut app),
            pool_bypass,
            "run {worker_mode} flag={flag}"
        );
    }

    // A mode's own permission_mode: the session it spawns runs in bypass,
    // so the chip shows and the log names the mode.
    let cfg = config("default", "default", true);
    let (mut app, _dir) = run_app(cfg.clone(), false, None);
    let session = yolo_session(&cfg);
    let id = session.id;
    app.add_session(session).await.expect("enqueue");
    let managed = app.pool.get_active_mut(id).expect("promoted");
    assert_eq!(managed.permission_mode.as_deref(), Some(BYPASS));
    assert!(app.bypass_active, "a bypass session must turn the chip on");
    assert!(chip_drawn(&mut app));
    let log = last_bypass_log(&app).expect("a [BYPASS] log line");
    assert!(log.contains("(mode yolo)"), "got: {log}");
    assert!(!app.bypass_warning_acknowledged);

    // After a Settings save: a save with no CONFIRM, then one with it.
    use super::agents_tests::{choose_agent_bypass, ctrl_s, settings_app, type_confirm};
    let (mut app, _dir) = settings_app(BYPASS);
    ctrl_s(&mut app).await;
    let pool_bypass = app.pool.permission_mode() == BYPASS;
    assert!(
        !pool_bypass,
        "unconfirmed save keeps the pool out of bypass"
    );
    assert_eq!(app.bypass_active, pool_bypass);
    assert_eq!(chip_drawn(&mut app), pool_bypass);

    let (mut app, _dir) = settings_app("default");
    choose_agent_bypass(&mut app).await;
    type_confirm(&mut app).await;
    ctrl_s(&mut app).await;
    let pool_bypass = app.pool.permission_mode() == BYPASS;
    assert!(pool_bypass, "confirmed save puts the pool in bypass");
    assert_eq!(app.bypass_active, pool_bypass);
    assert_eq!(chip_drawn(&mut app), pool_bypass);
}

/// Open Settings on the App with `screen_body` staged, saving to a temp
/// `maestro.toml` inside `dir`.
fn open_settings(app: &mut App, dir: &tempfile::TempDir, screen_body: &str) {
    use crate::tui::app::TuiMode;
    use crate::tui::screens::settings::SettingsScreen;
    let path = dir.path().join("maestro.toml");
    std::fs::write(&path, screen_body).expect("write maestro.toml");
    let config: Config = toml::from_str(screen_body).expect("config parses");
    let screen = SettingsScreen::new(config, app.flags.clone()).with_config_path(path);
    app.screen_state.settings_screen = Some(screen);
    app.tui_mode = TuiMode::Settings;
}

/// #1411 (judge round): a mode's bypass lights the chip but must never
/// reach the pool, also not through a later, unrelated Ctrl+S.
#[tokio::test]
async fn mode_bypass_never_reaches_the_pool_through_a_save() {
    use super::agents_tests::ctrl_s;
    for claude_on_screen in ["default", BYPASS] {
        let body = config_body("default", "default", true);
        let cfg: Config = toml::from_str(&body).expect("config parses");
        let (mut app, dir) = run_app(cfg.clone(), false, None);
        app.add_session(yolo_session(&cfg)).await.expect("enqueue");
        assert!(app.bypass_active, "setup: the mode lit the chip");

        // A save the user did not confirm bypass in (the second one even
        // stages an unconfirmed default-agent bypass).
        let screen_body = config_body(claude_on_screen, "default", true);
        open_settings(&mut app, &dir, &screen_body);
        ctrl_s(&mut app).await;
        assert_eq!(
            app.pool.permission_mode(),
            "default",
            "screen {claude_on_screen}"
        );
        // The refusal must say what ⌃B really does now: the chip is on, so
        // ⌃B turns bypass off, it does not turn it on.
        let refusal = (app.activity_log.entries().iter())
            .find(|e| e.message.starts_with("Bypass not turned on"))
            .map(|e| e.message.clone());
        if claude_on_screen == BYPASS {
            let refusal = refusal.expect("the save says why");
            assert!(!refusal.contains("turns it on"), "got: {refusal}");
            assert!(
                refusal.contains("Ctrl+B turns bypass off"),
                "got: {refusal}"
            );
        }

        let plain = Session::new(
            "p".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(8),
            None,
        );
        let id = plain.id;
        app.add_session(plain).await.expect("enqueue");
        let managed = app.pool.get_active_mut(id).expect("promoted");
        assert_eq!(managed.permission_mode.as_deref(), Some("default"));
    }
}

/// How the queued session gets promoted once a slot frees.
#[derive(Clone, Copy, Debug)]
enum Promote {
    AddSession,
    Tick,
}

async fn press_ctrl_b(app: &mut App) {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    app.tui_mode = crate::tui::app::TuiMode::Landing;
    let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    crate::tui::input_handler::handle_key(app, ctrl_b).await;
}

/// #1411 (judge round 2): pool bypass is on (⌃B + CONFIRM), a mode-bypass
/// session is queued, ⌃B turns bypass off while it waits, then a slot
/// frees. The session spawns in bypass, so the chip must come back on and
/// the log must name its mode, for both promotion paths.
#[tokio::test]
async fn queued_mode_bypass_lights_the_chip_when_it_spawns() {
    use super::agents_tests::type_confirm;
    for promote in [Promote::AddSession, Promote::Tick] {
        let mut cfg = config("default", "default", true);
        cfg.sessions.max_concurrent = 1;
        let (mut app, _dir) = run_app(cfg.clone(), false, None);

        press_ctrl_b(&mut app).await;
        type_confirm(&mut app).await;
        assert!(app.bypass_active, "{promote:?}: setup: ⌃B + CONFIRM on");
        assert_eq!(app.pool.permission_mode(), BYPASS);

        let first = Session::new(
            "a".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1),
            None,
        );
        let first_id = first.id;
        app.add_session(first).await.expect("enqueue");
        let queued = yolo_session(&cfg);
        let queued_id = queued.id;
        app.add_session(queued).await.expect("enqueue");
        assert!(
            app.pool.get_active_mut(queued_id).is_none(),
            "{promote:?}: yolo is queued"
        );

        press_ctrl_b(&mut app).await;
        assert!(
            !app.bypass_active,
            "{promote:?}: ⌃B turned bypass off while queued"
        );
        assert!(!chip_drawn(&mut app));
        let before = app.activity_log.entries().len();

        app.pool.finalize_retain_worktree(first_id);
        match promote {
            Promote::AddSession => {
                let third = Session::new(
                    "c".into(),
                    "opus".into(),
                    "orchestrator".into(),
                    Some(3),
                    None,
                );
                app.add_session(third).await.expect("enqueue");
            }
            Promote::Tick => app.check_completions().await.expect("tick"),
        }

        let managed = app.pool.get_active_mut(queued_id).expect("yolo promoted");
        assert_eq!(managed.permission_mode.as_deref(), Some(BYPASS));
        assert!(
            app.bypass_active,
            "{promote:?}: a bypass session spawning lights the chip"
        );
        assert!(chip_drawn(&mut app), "{promote:?}");
        let fresh = &app.activity_log.entries()[before..];
        assert!(
            (fresh.iter()).any(|e| e.message.starts_with("Bypass mode enabled")
                && e.message.contains("(mode yolo)")),
            "{promote:?}: a fresh [BYPASS] line names the mode"
        );
        assert_eq!(
            app.pool.permission_mode(),
            "default",
            "{promote:?}: the pool stays out"
        );
    }
}
