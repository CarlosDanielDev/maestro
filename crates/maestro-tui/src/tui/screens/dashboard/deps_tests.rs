//! #1292: Dashboard Deps and its two lenses, through the real key path
//! and `ui::draw` at 120x36 (above the 80x24 SizeGuard).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::{Column, Lens};
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::screens::dashboard::usage::View;

fn config(enabled: bool) -> crate::config::Config {
    let toml = format!(
        "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n\
         per_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
         [github]\n[notifications]\n[views]\nagent_graph_enabled = {enabled}\n"
    );
    toml::from_str(&toml).expect("test config")
}

fn app(enabled: bool) -> (App, Vec<uuid::Uuid>) {
    let mut app = crate::tui::make_test_app("deps-1292");
    app.config = Some(config(enabled));
    app.screen_state.home_screen = Some(crate::tui::screens::HomeScreen::new(
        crate::tui::screens::home::ProjectInfo {
            repo: "owner/repo".into(),
            branch: "main".into(),
            username: None,
        },
        vec![],
        vec![],
    ));
    let mut ids = Vec::new();
    for issue in [407, 409] {
        let mut s = Session::new(
            format!("task {issue}"),
            "claude-opus-4-5".into(),
            "orchestrator".into(),
            Some(issue),
            None,
        );
        s.status = SessionStatus::Running;
        ids.push(s.id);
        app.pool.enqueue(s);
    }
    app.tui_mode = TuiMode::Dashboard;
    draw(&mut app);
    (app, ids)
}

fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

fn shown(app: &App) -> (TuiMode, View, Lens) {
    let dash = &app.screen_state.dashboard;
    let lens = super::lens(dash, app.is_agent_graph_enabled());
    (app.tui_mode, dash.view, lens)
}

#[tokio::test]
async fn deps_lens_toggle() {
    let (mut app, _) = app(true);
    let depth = app.nav_stack.depth();
    press(&mut app, KeyCode::Char('g')).await;
    assert_eq!(shown(&app), (TuiMode::Dashboard, View::Deps, Lens::Issues));
    let out = draw(&mut app);
    assert!(out.contains("Dependency Graph"), "issues lens:\n{out}");
    press(&mut app, KeyCode::Char('g')).await;
    assert_eq!(shown(&app).2, Lens::Files, "g again switches the lens");
    let out = draw(&mut app);
    assert!(
        out.contains("Files · worktrees merged"),
        "files lens:\n{out}"
    );
    assert!(out.contains("files edited"), "strip counts:\n{out}");
    press(&mut app, KeyCode::Char('g')).await;
    assert_eq!(shown(&app).2, Lens::Issues, "and back");
    assert_eq!(
        app.nav_stack.depth(),
        depth,
        "a lens switch is not navigation"
    );

    // The old Agent graph lands on the files lens.
    press(&mut app, KeyCode::Char('o')).await;
    app.navigate_to(TuiMode::AgentGraph);
    assert_eq!(shown(&app), (TuiMode::Dashboard, View::Deps, Lens::Files));
    assert!(draw(&mut app).contains("Files · worktrees merged"));
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(shown(&app).1, View::Ops, "Esc leaves Deps for Ops");
}

/// With `views.agent_graph_enabled = false`, `g` stays on issues, the
/// strip says why, and `navigate_to(AgentGraph)` keeps its redirect.
#[tokio::test]
async fn deps_files_lens_off_stays_on_issues() {
    let (mut app, _) = app(false);
    press(&mut app, KeyCode::Char('g')).await;
    press(&mut app, KeyCode::Char('g')).await;
    assert_eq!(shown(&app), (TuiMode::Dashboard, View::Deps, Lens::Issues));
    let out = draw(&mut app);
    assert!(out.contains("⊘ off in [views]"), "strip:\n{out}");
    assert!(!out.contains("Files · worktrees merged"));
    app.navigate_to(TuiMode::AgentGraph);
    draw(&mut app);
    assert_ne!(shown(&app).2, Lens::Files, "the flag gates the lens");

    // Turned off while the files lens is shown (a config reload): the
    // drawn lens is issues at once.
    let (mut app, _) = self::app(true);
    app.navigate_to(TuiMode::AgentGraph);
    app.config = Some(config(false));
    let out = draw(&mut app);
    assert!(
        !out.contains("Files · worktrees merged"),
        "lens off:\n{out}"
    );
    assert!(out.contains("Dependency Graph"), "issues lens:\n{out}");
}

/// Every key on the files lens is its own: none reaches the launcher
/// letters or the Overview keys, and kill acts only on the drawn row.
#[tokio::test]
async fn deps_files_lens_keys_do_not_fall_through() {
    let (mut app, ids) = app(true);
    app.navigate_to(TuiMode::AgentGraph);
    draw(&mut app);
    let log = app.show_activity_log;
    for c in ['p', 'r', 'D', 'd', 'l', 'n', 'm'] {
        press(&mut app, KeyCode::Char(c)).await;
        assert_eq!(
            shown(&app),
            (TuiMode::Dashboard, View::Deps, Lens::Files),
            "{c}"
        );
    }
    assert_eq!(app.pool.all_sessions().len(), 2, "D dismissed nothing");
    assert_eq!(app.show_activity_log, log, "d toggled the log");
    // j selects an agent row; → moves to the files column, where X is off.
    press(&mut app, KeyCode::Char('j')).await;
    let dash = &app.screen_state.dashboard;
    assert!(ids.contains(&dash.list.selected_session().unwrap()));
    press(&mut app, KeyCode::Right).await;
    assert_eq!(app.screen_state.dashboard.deps.column, Column::Files);
    press(&mut app, KeyCode::Char('X')).await;
    assert!(
        !draw(&mut app).contains("Type"),
        "X on a file row opens no kill"
    );
}
