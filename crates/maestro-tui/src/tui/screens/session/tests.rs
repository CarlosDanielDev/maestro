//! #1288: the session screen through the real key path (`handle_key`) and
//! `ui::draw`, at 80x24 or larger (below that the SizeGuard takes keys).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use crate::session::types::{Session, SessionStatus};
use crate::tui::app::{App, SessionView, TuiMode};
use crate::tui::input_handler::handle_key;

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

async fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c)).await;
    }
}

/// The session screen on `view`.
fn at(id: uuid::Uuid, view: SessionView) -> TuiMode {
    TuiMode::Session { id, view }
}

fn draw(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

fn rows(n: std::ops::RangeInclusive<u32>) -> String {
    n.map(|i| format!("- row {i:02}\n")).collect()
}

/// A running session with a fixed id, so snapshots are stable. No start
/// time: elapsed draws `—` instead of a clock reading.
fn session(n: u128, issue: u64, title: &str) -> Session {
    let mut s = Session::new(
        format!("task {issue}"),
        "claude-opus-4-5".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    );
    s.id = uuid::Uuid::from_u128(n);
    s.issue_title = Some(title.into());
    s
}

fn app_with(sessions: Vec<Session>) -> App {
    let mut app = crate::tui::make_test_app("session-1288");
    app.show_mascot = false;
    app.show_activity_log = false;
    app.no_splash = true;
    let ids: Vec<_> = sessions.iter().map(|s| s.id).collect();
    for s in sessions {
        app.pool.enqueue(s);
    }
    // #1515: live sessions, as the views show them: the status bar counts
    // only active sessions as running.
    crate::tui::snapshot_tests::promote(&mut app);
    for id in ids {
        app.pool.get_session_mut(id).unwrap().status = SessionStatus::Running;
    }
    app
}

fn one_session() -> (App, uuid::Uuid) {
    let mut s = session(0x412, 412, "auth-refactor");
    s.last_message = rows(1..=40);
    let id = s.id;
    (app_with(vec![s]), id)
}

/// Text of a drawn line, without its edge padding.
fn text(line: &ratatui::text::Line) -> String {
    let s: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    s.trim().to_string()
}

#[tokio::test]
async fn session_header_matches_dashboard_pane() {
    let (mut app, id) = one_session();
    app.tui_mode = TuiMode::Dashboard;
    let (w, h) = (120, 36);
    let pane = draw(&mut app, w, h);
    assert_eq!(
        app.screen_state.dashboard.list.selected_session(),
        Some(id),
        "the pane shows the session"
    );

    press(&mut app, KeyCode::Enter).await;
    assert_eq!(
        app.tui_mode,
        at(id, SessionView::Detail),
        "⏎ opens the session"
    );
    let screen = draw(&mut app, w, h);

    let theme = app.theme.clone();
    let s = app.pool.get_session(id).unwrap();
    let nerd = crate::icon_mode::use_nerd_font();
    let header = super::header(s, chrono::Utc::now(), w, w, &theme, nerd);
    assert_eq!(header.len(), 3, "status, title, pipeline");
    for line in &header {
        let words = text(line);
        assert!(!words.is_empty());
        assert!(pane.contains(&words), "pane lacks {words:?}:\n{pane}");
        assert!(screen.contains(&words), "screen lacks {words:?}:\n{screen}");
    }
}

#[tokio::test]
async fn output_follow_toggle() {
    let (mut app, id) = one_session();
    app.tui_mode = at(id, SessionView::Detail);
    press(&mut app, KeyCode::Char('f')).await;
    assert_eq!(app.tui_mode, at(id, SessionView::Output), "f opens Output");
    let out = draw(&mut app, 80, 24);
    assert!(out.contains("row 40"), "follows the tail:\n{out}");
    assert!(out.contains("following"), "says it follows:\n{out}");

    press(&mut app, KeyCode::Char('k')).await;
    let out = draw(&mut app, 80, 24);
    assert!(!out.contains("row 40"), "k scrolled up:\n{out}");
    assert!(out.contains("row 39"), "one row up:\n{out}");
    assert!(out.contains("End follows"), "says how to follow:\n{out}");

    // New output lands while scrolled up: the view stays put.
    app.pool.get_session_mut(id).unwrap().last_message = rows(1..=60);
    let out = draw(&mut app, 80, 24);
    assert!(out.contains("row 39"), "the view stayed:\n{out}");
    assert!(!out.contains("row 60"), "no jump to the tail:\n{out}");

    press(&mut app, KeyCode::End).await;
    let out = draw(&mut app, 80, 24);
    assert!(out.contains("row 60"), "End follows the tail:\n{out}");
    assert!(out.contains("following"), "{out}");
}

#[tokio::test]
async fn strip_keys_switch_views_and_act_on_the_mode_id() {
    // Two sessions: the Overview grid selects #411, the screen shows #412.
    let a = session(0x411, 411, "quota-router");
    let b = session(0x412, 412, "auth-refactor");
    let id = b.id;
    let mut app = app_with(vec![a, b]);
    app.nav_stack.push(TuiMode::Dashboard);
    app.tui_mode = at(id, SessionView::Detail);
    for (key, view) in [
        ('f', SessionView::Output),
        ('l', SessionView::Log),
        ('v', SessionView::Gates),
        ('e', SessionView::Events),
        ('d', SessionView::Detail),
    ] {
        press(&mut app, KeyCode::Char(key)).await;
        assert_eq!(app.tui_mode, TuiMode::Session { id, view }, "{key}");
        draw(&mut app, 80, 24);
    }
    assert_eq!(
        app.focused_session_for_copy().map(|s| s.id),
        Some(id),
        "c copies #412"
    );
    press(&mut app, KeyCode::Char('X')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmKill(id), "X kills #412");
}

#[tokio::test]
async fn switcher_filter_keeps_selection() {
    let a = session(0x411, 411, "auth-refactor");
    let b = session(0x412, 412, "auth-login");
    let c = session(0x500, 500, "docs-site");
    let want = b.id;
    let mut app = app_with(vec![a, b, c]);
    app.tui_mode = at(want, SessionView::Detail);
    press(&mut app, KeyCode::Char('w')).await;
    assert_eq!(app.tui_mode, TuiMode::SessionSwitcher);

    let selected = |app: &App| {
        let sessions = app.pool.all_sessions();
        let sw = app.screen_state.session_switcher.as_ref().unwrap();
        sw.selected_session(&sessions).map(|s| s.id)
    };
    for _ in 0..3 {
        if selected(&app) == Some(want) {
            break;
        }
        press(&mut app, KeyCode::Down).await;
    }
    assert_eq!(selected(&app), Some(want), "↓ reaches #412");

    type_text(&mut app, "auth").await;
    let out = draw(&mut app, 80, 24);
    assert!(out.contains("› auth"), "the typed filter shows:\n{out}");
    assert!(out.contains("#411") && out.contains("#412"), "{out}");
    assert!(!out.contains("#500"), "docs-site filtered out:\n{out}");
    assert_eq!(selected(&app), Some(want), "selection kept by id");

    // A filter that hides the row and back: the selection returns.
    type_text(&mut app, "zz").await;
    assert_eq!(selected(&app), None);
    press(&mut app, KeyCode::Backspace).await;
    press(&mut app, KeyCode::Backspace).await;
    assert_eq!(selected(&app), Some(want));

    // `q` is text here, not quit.
    press(&mut app, KeyCode::Char('q')).await;
    assert!(app.running);
    press(&mut app, KeyCode::Backspace).await;

    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, at(want, SessionView::Detail));
}

/// `session_{detail,output,log,gates,events}`: every view at 80x24.
#[tokio::test]
async fn session_view_snapshots() {
    for (name, view) in [
        ("session_detail", SessionView::Detail),
        ("session_output", SessionView::Output),
        ("session_log", SessionView::Log),
        ("session_gates", SessionView::Gates),
        ("session_events", SessionView::Events),
    ] {
        let (mut app, id) = one_session();
        app.tui_mode = TuiMode::Session { id, view };
        let out = draw(&mut app, 80, 24);
        for tab in ["d Detail", "f Output", "l Log", "v Gates", "e Events"] {
            assert!(out.contains(tab), "{name}: strip lacks {tab}:\n{out}");
        }
        insta::assert_snapshot!(name, out);
    }
}

/// Judge round (#1440): F3 and F10 on the session screen act on the
/// session it shows, never the Dashboard's marked row. Entries: the
/// switcher, the completion summary's `l` / `v` (Log / Gates, then `d`),
/// and hollow retry's `Push(Detail(id))`.
#[tokio::test]
async fn fkeys_on_the_session_screen_act_on_the_shown_session() {
    for entry in ["switcher", "log", "gates", "hollow-retry"] {
        let a = session(0x411, 411, "quota-router");
        let b = session(0x412, 412, "auth-refactor");
        let (a_id, b_id) = (a.id, b.id);
        let mut app = app_with(vec![a, b]);
        app.tui_mode = TuiMode::Dashboard;
        draw(&mut app, 120, 36);
        assert_eq!(app.focused_session_id(), Some(a_id), "{entry}: A is marked");

        match entry {
            "switcher" => {
                press(&mut app, KeyCode::Char('w')).await;
                type_text(&mut app, "412").await;
                press(&mut app, KeyCode::Enter).await;
            }
            "log" => {
                app.navigate_to(at(b_id, SessionView::Log));
                press(&mut app, KeyCode::Char('d')).await;
            }
            "gates" => {
                app.tui_mode = at(b_id, SessionView::Gates);
                press(&mut app, KeyCode::Char('d')).await;
            }
            _ => app.navigate_to(at(b_id, SessionView::Detail)),
        }
        assert_eq!(app.tui_mode, at(b_id, SessionView::Detail), "{entry}");
        draw(&mut app, 120, 36);

        press(&mut app, KeyCode::F(10)).await;
        assert_eq!(app.tui_mode, TuiMode::ConfirmKill(b_id), "{entry}: F10");
        press(&mut app, KeyCode::Esc).await;
        app.tui_mode = at(b_id, SessionView::Detail);
        draw(&mut app, 120, 36);
        press(&mut app, KeyCode::F(3)).await;
        assert_eq!(app.tui_mode, at(b_id, SessionView::Output), "{entry}: F3");
    }
}

/// #1291: the Gates view lists every gate (pass and fail) with command and
/// duration, and a failing gate shows its digest with the first failure
/// open, then the output tail.
#[tokio::test]
async fn session_gates_digest() {
    use crate::session::types::GateResultEntry;
    let tests_out = "\
running 3 tests
test parser::tests::parses_header ... FAILED
test store::tests::round_trip ... FAILED
test store::tests::loads ... ok

failures:

---- parser::tests::parses_header stdout ----

thread 'parser::tests::parses_header' panicked at src/parser.rs:42:9:
assertion `left == right` failed
  left: 3
 right: 4
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- store::tests::round_trip stdout ----

thread 'store::tests::round_trip' panicked at src/store.rs:88:5:
called `Result::unwrap()` on an `Err` value: NotFound

failures:
    parser::tests::parses_header
    store::tests::round_trip

test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `--lib`
";
    let gate = |name: &str, passed: bool, command: &str, ms: u64, output: &str| GateResultEntry {
        command: command.into(),
        duration_ms: ms,
        output: output.into(),
        log_path: (!passed).then(|| format!(".maestro/logs/gates/issue-412/{name}.log").into()),
        ..if passed {
            GateResultEntry::pass(name, format!("{name} passed"))
        } else {
            GateResultEntry::fail(name, format!("{name} failed: error: test failed"))
        }
    };
    let (mut app, id) = one_session();
    {
        let s = app.pool.get_session_mut(id).unwrap();
        s.status = SessionStatus::FailedGates;
        s.gate_results = vec![
            gate("fmt", true, "cargo fmt -- --check", 400, ""),
            gate("clippy", true, "cargo clippy -- -D warnings", 12_300, ""),
            gate("test", false, "cargo test", 41_000, tests_out),
        ];
    }
    app.tui_mode = at(id, SessionView::Gates);
    let out = draw(&mut app, 100, 40);
    for want in [
        "fmt",
        "cargo clippy -- -D warnings",
        "41.0s",
        "✗ 2 failing · 1 passed",
        "FAILURES BY FILE",
        "src/parser.rs",
        "parser::tests::parses_header  :42:9",
        "assertion `left == right` failed",
        "store::tests::round_trip  :88:5",
        "OUTPUT TAIL",
    ] {
        assert!(out.contains(want), "Gates view lacks {want:?}:\n{out}");
    }
    assert_eq!(
        out.matches("called `Result::unwrap()`").count(),
        1,
        "only the first failure is open (the second one's body is in the tail only):\n{out}"
    );
    insta::assert_snapshot!("session_gates_digest", out);
}
