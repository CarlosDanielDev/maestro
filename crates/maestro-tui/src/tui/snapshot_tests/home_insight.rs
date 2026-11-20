//! Home Insight column, first-run logo and `n` (#1279), drawn through the
//! full `ui::draw` on the busy Mission Control fixture of `home_mission`.

use crossterm::event::KeyCode;
use insta::assert_snapshot;

use super::home_mission::{busy_app, draw, press, rows};
use crate::tui::app::TuiMode;
use crate::tui::screens::home::mission::WhatsNew;

/// The first row of the Nerd Font logo or the second of the ASCII one.
fn has_logo(r: &[String]) -> bool {
    r.iter()
        .any(|l| l.contains("███╗   ███╗") || l.contains("|  \\/  |"))
}

#[test]
fn home_wide_insight() {
    // The narrowest Wide terminal: the Insight column is still drawn.
    let mut app = busy_app();
    let t = draw(&mut app, 150, 40);
    let text = rows(&t).join("\n");
    for want in [
        "Budget & quota",
        "What's new v9.9.9",
        "+ Home shows the What's new card.",
        "n all notes",
    ] {
        assert!(text.contains(want), "missing {want:?}:\n{text}");
    }
    assert_snapshot!("home_wide_insight", t.backend());
}

#[test]
fn insight_column_is_wide_only() {
    let mut app = busy_app();
    let text = rows(&draw(&mut app, 120, 36)).join("\n");
    assert!(!text.contains("What's new"), "Regular drops it:\n{text}");
}

#[test]
fn whats_new_without_notes_says_so() {
    let mut app = busy_app();
    app.whats_new.items.clear();
    let text = rows(&draw(&mut app, 160, 48)).join("\n");
    assert!(text.contains("No notes for v9.9.9"), "{text}");
}

#[test]
fn whats_new_reads_the_installed_version() {
    let card = WhatsNew::installed();
    assert_eq!(card.version, env!("CARGO_PKG_VERSION"));
    assert!(card.items.len() <= 4, "one card, at most four highlights");
}

#[test]
fn logo_on_first_run() {
    for w in [160u16, 120] {
        let mut app = busy_app();
        app.first_run = true;
        let r = rows(&draw(&mut app, w, 60));
        assert!(has_logo(&r), "{w}x60:\n{}", r.join("\n"));
        assert!(r.join("\n").contains("Needs you (3)"), "{w}x60");
    }
}

#[test]
fn no_logo_after_first_run() {
    for w in [160u16, 120, 80] {
        let mut app = busy_app();
        app.first_run = false;
        let r = rows(&draw(&mut app, w, 60));
        assert!(!has_logo(&r), "{w}x60:\n{}", r.join("\n"));
    }
}

#[test]
fn first_run_logo_gives_way_on_a_short_terminal() {
    let mut app = busy_app();
    app.first_run = true;
    let r = rows(&draw(&mut app, 80, 24));
    assert!(!has_logo(&r), "80x24 keeps its rows:\n{}", r.join("\n"));
    assert!(r.join("\n").contains("Needs you (3)"));
}

#[tokio::test]
async fn home_n_opens_release_notes() {
    let mut app = busy_app();
    draw(&mut app, 80, 24);
    press(&mut app, KeyCode::Char('n')).await;
    assert_eq!(app.tui_mode, TuiMode::ReleaseNotes);
    let text = rows(&draw(&mut app, 80, 24)).join("\n");
    assert!(
        text.contains("Release notes") || text.contains("Release Notes"),
        "{text}"
    );
}
