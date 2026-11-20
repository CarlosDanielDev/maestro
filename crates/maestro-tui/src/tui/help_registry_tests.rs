//! #1398: Help is built from the keymap registry. Every bound key has a
//! row with its name, Home names its action letters, and search finds the
//! global chords. Drawn at 80x24 and 80x27 (below 80x24 the SizeGuard
//! swallows keys).

use crossterm::event::KeyCode;
use ratatui::{Terminal, backend::TestBackend};

use super::tests::drawn::{SIZES, draw, inner_rows, press};
use crate::tui::app::{App, TuiMode};
use crate::tui::navigation::keymap::mode_keymap;
use crate::tui::navigation::registry::{Scope, registry};
use crate::tui::screens::LandingScreen;
use crate::tui::screens::home::{HomeScreen, ProjectInfo};

async fn help_open(mode: TuiMode, width: u16, height: u16) -> (App, Terminal<TestBackend>) {
    let mut app = crate::tui::make_test_app("help-1398");
    app.screen_state.landing_screen = Some(LandingScreen::new());
    app.screen_state.home_screen = Some(HomeScreen::new(
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: None,
        },
        vec![],
        vec![],
    ));
    app.tui_mode = mode;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    draw(&mut app, &mut terminal);
    press(&mut app, KeyCode::Char('?')).await;
    assert!(app.help_state.is_some(), "? opens Help on {mode:?}");
    (app, terminal)
}

/// Every Help row, page by page with `j`, until the scroll stops.
async fn all_rows(app: &mut App, terminal: &mut Terminal<TestBackend>) -> Vec<String> {
    let (w, h) = (
        terminal.size().unwrap().width,
        terminal.size().unwrap().height,
    );
    let mut rows = inner_rows(&draw(app, terminal), w, h);
    loop {
        let before = app.help_state.as_ref().unwrap().scroll;
        press(app, KeyCode::Char('j')).await;
        let page = inner_rows(&draw(app, terminal), w, h);
        if app.help_state.as_ref().unwrap().scroll == before {
            return rows;
        }
        rows.push(page.last().unwrap().clone());
    }
}

/// A row whose key column is `key` and whose text contains `name`.
fn has_row(rows: &[String], key: &str, name: &str) -> bool {
    rows.iter().any(|r| {
        let t = r.trim_start();
        t.starts_with(&format!("{key} ")) && t.contains(name)
    })
}

#[tokio::test]
async fn help_home_lists_action_letters_and_title_home() {
    for (w, h) in SIZES {
        let (mut app, mut terminal) = help_open(TuiMode::Landing, w, h).await;
        let rows = all_rows(&mut app, &mut terminal).await;
        let text = rows.join("\n");
        assert!(rows[0].contains("Home"), "{w}x{h}: title {:?}", rows[0]);
        assert!(!rows[0].contains("Welcome"), "{w}x{h}: title {:?}", rows[0]);
        assert!(
            has_row(&rows, "t", "Teams"),
            "{w}x{h}: no t Teams row\n{text}"
        );
        assert!(has_row(&rows, "f", "Fleet Board"), "{w}x{h}\n{text}");
        assert!(
            !rows.iter().any(|r| r.contains("1-9")),
            "{w}x{h}: a 1-9 row is left\n{text}"
        );
    }
}

#[test]
fn help_lists_every_registry_binding_for_mode() {
    let nil = uuid::Uuid::nil();
    let modes = [
        TuiMode::Landing,
        TuiMode::Dashboard,
        TuiMode::Overview,
        TuiMode::LogViewer(nil),
        TuiMode::SessionSummary,
        TuiMode::ConfirmKill(nil),
        TuiMode::Fullscreen(nil),
        TuiMode::AgentGraph,
    ];
    for mode in modes {
        let km = mode_keymap(mode, None, &[], false);
        let rows: Vec<_> = km.help_groups.iter().flat_map(|g| &g.bindings).collect();
        for b in registry()
            .iter()
            .filter(|b| b.scope == Scope::Global || Scope::Screen(mode).encloses(b.scope))
        {
            let key = b.chord.label();
            assert!(
                rows.iter().any(
                    |r| r.key.split(" / ").any(|k| k == key) && r.description.contains(b.label)
                ),
                "{mode:?}: no Help row for {key} {:?} ({:?})",
                b.label,
                b.action
            );
        }
    }
}

#[tokio::test]
async fn help_search_finds_global_chords() {
    for mode in [TuiMode::Landing, TuiMode::Dashboard] {
        for query in ["team", "setting", "bypass", "turbo"] {
            let (w, h) = (80, 24);
            let (mut app, mut terminal) = help_open(mode, w, h).await;
            press(&mut app, KeyCode::Char('/')).await;
            for c in query.chars() {
                press(&mut app, KeyCode::Char(c)).await;
            }
            let rows = inner_rows(&draw(&mut app, &mut terminal), w, h);
            let found = rows
                .iter()
                .filter(|r| !r.contains("Search:"))
                .filter(|r| r.to_lowercase().contains(query))
                .count();
            assert!(
                found >= 1,
                "{mode:?}: /{query} found no row\n{}",
                rows.join("\n")
            );
        }
    }
}

#[tokio::test]
async fn help_dashboard_80x24() {
    let (w, h) = (80, 24);
    let (mut app, mut terminal) = help_open(TuiMode::Dashboard, w, h).await;
    let first_page = inner_rows(&draw(&mut app, &mut terminal), w, h);
    insta::assert_snapshot!(first_page.join("\n"));

    let rows = all_rows(&mut app, &mut terminal).await;
    let text = rows.join("\n");
    assert!(has_row(&first_page, "S", "Settings"), "S on page 1\n{text}");
    for (key, name) in [
        ("s", "Status"),
        ("c", "Cost"),
        ("t", "Token"),
        ("Q", "TurboQuant"),
        ("M", "complete modal"),
        ("u", "upgrade"),
    ] {
        assert!(has_row(&rows, key, name), "no {key} {name} row\n{text}");
    }
}
