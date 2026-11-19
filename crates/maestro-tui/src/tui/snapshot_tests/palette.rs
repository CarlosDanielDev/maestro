//! ⌃K command palette (#1255): opens on every top-level screen, lists the
//! registry plus the palette-only commands, underlines fuzzy hits over a
//! dimmed screen.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatui::style::Modifier;
use ratatui::{Terminal, backend::TestBackend};

use super::log_drawer::{draw, log_app};
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::palette::{self, Run};
use crate::tui::ds::tab_bar::TABS;
use crate::tui::input_handler::handle_key;
use crate::tui::navigation::registry::{
    Action, Binding, Chord, Danger, PALETTE_COMMANDS, PaletteCommand, Scope, registry,
};

const TITLE: &str = "Command palette";
/// What the network command puts on screen.
const NETWORK_RAN: &str = "Network diagnostics";

fn ctrl_k() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

async fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        handle_key(app, key(KeyCode::Char(c))).await;
    }
}

/// The cell where `text` starts on screen.
fn find(t: &Terminal<TestBackend>, text: &str) -> Option<(u16, u16)> {
    let buf = t.backend().buffer();
    (0..buf.area.height).find_map(|y| {
        let row: Vec<&str> = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        let row = row.concat();
        let at = row.find(text)?;
        Some((row[..at].chars().count() as u16, y))
    })
}

fn screen(t: &Terminal<TestBackend>) -> String {
    format!("{}", t.backend())
}

#[tokio::test]
async fn ctrl_k_opens_palette() {
    for (_, name, mode) in TABS {
        for (w, h) in [(80, 24), (80, 27)] {
            let mut app = log_app(mode);
            assert!(!screen(&draw(&mut app, w, h)).contains(TITLE), "{name}");
            handle_key(&mut app, ctrl_k()).await;
            let out = screen(&draw(&mut app, w, h));
            assert!(
                out.contains(TITLE),
                "{name} {w}x{h}: ⌃K must open it:\n{out}"
            );
            assert!(out.contains("INSERT"), "{name}: the query has focus");
            assert_eq!(app.tui_mode, mode, "{name}: the screen stays under it");

            // A letter types into the query, not into the screen under it.
            type_text(&mut app, "q").await;
            assert!(app.running, "{name}: `q` is query text, not quit");
            handle_key(&mut app, key(KeyCode::Esc)).await;
            let out = screen(&draw(&mut app, w, h));
            assert!(!out.contains(TITLE), "{name}: Esc closes it");
            assert_eq!(app.tui_mode, mode, "{name}: Esc only closes the palette");
        }
    }
}

/// A binding added to the registry shows up with its key, with no palette
/// edit; every reachable action of the real registry is listed.
#[test]
fn palette_entries_follow_registry() {
    let mut bindings = registry().to_vec();
    let new = Binding::new(
        Chord::ctrl('o'),
        Scope::Global,
        Action::Undo,
        Danger::Safe,
        "brand new action",
    );
    let before = palette::entries(&bindings, TuiMode::Landing, |_| None);
    assert!(!before.iter().any(|e| e.label == "brand new action"));
    bindings.insert(0, new);
    let after = palette::entries(&bindings, TuiMode::Landing, |_| None);
    let entry = after
        .iter()
        .find(|e| e.label == "brand new action")
        .unwrap();
    assert_eq!(entry.key, "⌃O");
    assert_eq!(entry.run, Run::Binding(Action::Undo, Chord::ctrl('o')));

    let real = palette::entries(registry(), TuiMode::Landing, |_| None);
    for b in registry()
        .iter()
        .filter(|b| b.scope == Scope::Global && b.footer)
    {
        if b.action == Action::Palette {
            assert!(
                !real
                    .iter()
                    .any(|e| e.run == Run::Binding(b.action, b.chord))
            );
            continue;
        }
        assert!(
            real.iter()
                .any(|e| matches!(e.run, Run::Binding(a, _) if a == b.action)),
            "{:?} ({}) is missing from the palette",
            b.action,
            b.label
        );
    }
    for (_, name, _) in TABS {
        assert!(real.iter().any(|e| e.label == name), "tab {name}");
    }
    // A screen block shows only on its screen.
    let log = TuiMode::LogViewer(uuid::Uuid::nil());
    let scroll = |es: &[palette::Entry]| {
        es.iter()
            .any(|e| matches!(e.run, Run::Binding(Action::Top, _)))
    };
    assert!(scroll(&palette::entries(registry(), log, |_| None)));
    assert!(!scroll(&real));
    // Destructive bindings are marked; a disabled one carries its reason.
    let quit = real.iter().find(|e| e.label == "quit").unwrap();
    assert!(quit.destructive);
    assert_eq!(
        quit.key, "⌃Q",
        "the first chord of an action is the one shown"
    );
    let off = palette::entries(registry(), TuiMode::Landing, |a| {
        (a == Action::ReviewConcerns).then_some("no review pending")
    });
    let concerns = off.iter().find(|e| e.label == "concerns").unwrap();
    assert_eq!(concerns.disabled, Some("no review pending"));
}

/// A quit chord while the palette is open shows Confirm Exit alone.
#[tokio::test]
async fn quit_chord_closes_palette() {
    let mut app = log_app(TuiMode::Dashboard);
    handle_key(&mut app, ctrl_k()).await;
    let ctrl_x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);
    handle_key(&mut app, ctrl_x).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmExit);
    assert!(
        app.palette.is_none(),
        "Confirm Exit is not drawn under the palette"
    );
}

#[tokio::test]
async fn palette_has_network() {
    assert!(
        PALETTE_COMMANDS
            .iter()
            .any(|(label, cmd)| *label == "network" && *cmd == PaletteCommand::Network)
    );
    let mut app = log_app(TuiMode::Dashboard);
    handle_key(&mut app, ctrl_k()).await;
    type_text(&mut app, "network").await;
    let out = screen(&draw(&mut app, 80, 24));
    assert!(out.contains("network"), "the entry is listed:\n{out}");
    handle_key(&mut app, key(KeyCode::Enter)).await;
    assert!(app.palette.is_none(), "⏎ closes the palette");
    let out = screen(&draw(&mut app, 80, 24));
    assert!(
        out.contains(NETWORK_RAN),
        "⏎ runs the network command:\n{out}"
    );
}

#[tokio::test]
async fn palette_query_dash() {
    let mut app = log_app(TuiMode::Landing);
    handle_key(&mut app, ctrl_k()).await;
    type_text(&mut app, "dash").await;
    let t = draw(&mut app, 80, 24);
    let buf = t.backend().buffer();

    // Underlined cells, row by row, so the snapshot shows the hits.
    let mut hits = String::new();
    for y in 0..buf.area.height {
        let row: String = (0..buf.area.width)
            .map(|x| {
                let cell = &buf[(x, y)];
                if cell.modifier.contains(Modifier::UNDERLINED) {
                    cell.symbol()
                } else {
                    " "
                }
            })
            .collect();
        if !row.trim().is_empty() {
            hits.push_str(&format!("{y:>2}|{}\n", row.trim_end()));
        }
    }
    assert!(hits.contains("Dash"), "`dash` underlines D-a-s-h:\n{hits}");
    // The screen under the palette is dimmed; the palette is not.
    assert!(
        buf[(0, 0)].modifier.contains(Modifier::DIM),
        "backdrop dims"
    );
    let (x, y) = find(&t, TITLE).expect("palette drawn");
    assert!(
        !buf[(x, y)].modifier.contains(Modifier::DIM),
        "palette not dim"
    );
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d{2}:\d{2}:\d{2}", "HH:MM:SS");
    settings.bind(|| {
        assert_snapshot!(
            "palette_query_dash",
            format!("{}\n--- underlined ---\n{hits}", t.backend())
        )
    });
}

/// ⏎ on a tab opens it, also from a screen where its digit is content.
#[tokio::test]
async fn palette_tab_entry_opens_the_tab() {
    let mut app = log_app(TuiMode::Overview);
    handle_key(&mut app, ctrl_k()).await;
    type_text(&mut app, "Issues").await;
    handle_key(&mut app, key(KeyCode::Enter)).await;
    assert!(app.palette.is_none());
    assert_eq!(app.tui_mode, TuiMode::IssueBrowser);
}
