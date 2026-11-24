//! #1292: the Files lens of Dashboard Deps (§02.20 of
//! `docs/design-system/index.html`) through `navigate_to(AgentGraph)`
//! and `ui::draw`: agents left, edges, files right, the contested file
//! first, at 160 (Wide) and 100 (Regular) columns.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use super::usage::{empty_app, session, tokens};
use crate::session::types::SessionStatus;
use crate::tui::app::{App, TuiMode};
use crate::tui::ui;

const GRAPH_ON: &str = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n\
    per_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
    [github]\n[notifications]\n[views]\nagent_graph_enabled = true\n";

/// Three agents: #407 and #409 both edit `src/auth/middleware.rs` (a
/// recorded conflict, #407 holds the claim), #412 edits alone, and
/// `README.md` is only read.
fn lens_app() -> App {
    let mut app = empty_app();
    app.config = Some(toml::from_str(GRAPH_ON).expect("test config"));
    let mut ids = Vec::new();
    for (issue, status, title) in [
        (407, SessionStatus::Running, "auth-refactor"),
        (409, SessionStatus::Running, "session-expiry"),
        (412, SessionStatus::FailedGates, "fleet-sort"),
    ] {
        let mut s = session(issue, status, "claude-opus-4-5", 0.5, tokens(1, 1, 1, 1));
        s.issue_title = Some(title.into());
        s.files_touched = vec!["README.md".into()];
        ids.push(s.id);
        app.pool.enqueue(s);
    }
    let claims = &mut app.pool.file_claims;
    for (path, who) in [
        ("src/auth/middleware.rs", ids[0]),
        ("src/auth/token.rs", ids[0]),
        ("src/session/expiry.rs", ids[1]),
        ("src/fleet/sort.rs", ids[2]),
        (
            "crates/maestro-tui/src/tui/screens/fleet_board/draw.rs",
            ids[2],
        ),
    ] {
        let _ = claims.claim(path, who);
    }
    claims.record_conflict("src/auth/middleware.rs", ids[0], ids[1]);
    app.navigate_to(TuiMode::AgentGraph);
    app
}

fn draw(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

#[test]
fn files_lens_wide() {
    let mut app = lens_app();
    let out = draw(&mut app, 160, 40);
    let contested = out.find("CONTESTED").expect("contested section");
    assert!(contested < out.find("EDITED").expect("edited section"));
    assert!(out.contains("+ 1 read only"));
    assert_snapshot!(out);
}

#[test]
fn files_lens_regular() {
    let mut app = lens_app();
    let out = draw(&mut app, 100, 30);
    assert!(out.contains("src/auth/middleware.rs"));
    assert_snapshot!(out);
}
