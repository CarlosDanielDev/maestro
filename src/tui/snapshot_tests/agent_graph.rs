use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use crate::session::types::{Session, SessionStatus};
use crate::tui::agent_graph::model::build_graph;
use crate::tui::agent_graph::render::draw_agent_graph;
use crate::tui::snapshot_tests::{fixed_end, fixed_start};

fn make_session(
    prompt: &str,
    id: u128,
    issue: u64,
    status: SessionStatus,
    files: &[&str],
) -> Session {
    let mut s = Session::new(
        prompt.to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(issue),
        None,
    );
    s.id = Uuid::from_u128(id);
    s.status = status;
    s.started_at = Some(fixed_start());
    s.finished_at = Some(fixed_end());
    s.files_touched = files.iter().map(|f| (*f).to_string()).collect();
    s
}

fn three_agent_sessions() -> Vec<Session> {
    vec![
        make_session(
            "Implement login",
            0,
            101,
            SessionStatus::Running,
            &["src/auth/login.rs", "src/auth/token.rs"],
        ),
        make_session(
            "Add dashboard",
            1,
            102,
            SessionStatus::Running,
            &["src/tui/dashboard.rs", "src/auth/token.rs"],
        ),
        make_session(
            "Fix config",
            2,
            103,
            SessionStatus::Completed,
            &["src/config.rs"],
        ),
    ]
}

fn single_agent_session() -> Vec<Session> {
    vec![make_session(
        "Solo task",
        0,
        200,
        SessionStatus::Running,
        &["src/main.rs"],
    )]
}

fn render_with(
    sessions: &[Session],
    width: u16,
    height: u16,
    use_nerd_font: bool,
    tick: usize,
) -> Terminal<TestBackend> {
    let refs: Vec<&Session> = sessions.iter().collect();
    let (nodes, edges) = build_graph(&refs);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            draw_agent_graph(f, f.area(), &nodes, &edges, use_nerd_font, tick, &refs);
        })
        .unwrap();
    terminal
}

fn render_graph(sessions: &[Session], width: u16, height: u16) -> Terminal<TestBackend> {
    render_with(sessions, width, height, false, 0)
}

fn pulse_pair_sessions() -> Vec<Session> {
    let mut s1 = make_session(
        "Implement login",
        0,
        101,
        SessionStatus::Running,
        &["src/auth.rs"],
    );
    s1.current_activity = "Read: src/auth.rs".to_string();
    let s2 = make_session(
        "Add dashboard",
        1,
        102,
        SessionStatus::Running,
        &["src/dashboard.rs"],
    );
    vec![s1, s2]
}

#[test]
fn agent_graph_renders_at_80x24() {
    let t = render_graph(&three_agent_sessions(), 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_renders_at_100x30() {
    let t = render_graph(&three_agent_sessions(), 100, 30);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_renders_at_120x40() {
    let t = render_graph(&three_agent_sessions(), 120, 40);
    assert_snapshot!(t.backend());
}

/// A session with no `files_touched` — exercises the only remaining fallback
/// path (no edges to draw, just one node, so the card is still the right
/// affordance).
fn single_agent_no_files_session() -> Vec<Session> {
    vec![make_session(
        "Solo task with no files yet",
        0,
        201,
        SessionStatus::Running,
        &[],
    )]
}

fn buffer_text(t: &Terminal<TestBackend>) -> String {
    let buf = t.backend().buffer();
    let mut out = String::with_capacity((buf.area.width as usize + 1) * buf.area.height as usize);
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// Issue #543 follow-up: a single agent with files-touched should render the
/// full graph (agent + file ring + edges), not the fallback card. The graph
/// is meaningful as soon as there is at least one edge to draw.
#[test]
fn agent_graph_renders_single_agent_with_files_as_graph() {
    let t = render_graph(&single_agent_session(), 80, 24);
    let text = buffer_text(&t);
    assert!(
        !text.contains("no files touched yet"),
        "expected full graph render for 1 agent + 1 file, got fallback card:\n{text}"
    );
    assert_snapshot!(t.backend());
}

/// The fallback card is still the right affordance when there is nothing
/// edge-shaped to draw (1 agent, 0 files).
#[test]
fn agent_graph_falls_back_when_single_agent_has_no_files() {
    let t = render_graph(&single_agent_no_files_session(), 80, 24);
    let text = buffer_text(&t);
    assert!(
        text.contains("no files touched yet"),
        "expected fallback card for 1 agent + 0 files:\n{text}"
    );
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_running_node_braille_spinner_at_tick_0() {
    let t = render_with(&three_agent_sessions(), 120, 40, true, 0);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_running_node_braille_spinner_at_tick_5() {
    let t = render_with(&three_agent_sessions(), 120, 40, true, 5);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_completed_flash_tick4_bold_reversed() {
    let mut s_completed = make_session(
        "Fix config",
        0,
        103,
        SessionStatus::Completed,
        &["src/config.rs"],
    );
    s_completed.transition_flash_remaining = 4;
    let s_running = make_session(
        "Implement login",
        1,
        101,
        SessionStatus::Running,
        &["src/auth.rs"],
    );
    let t = render_with(&[s_completed, s_running], 120, 40, true, 0);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_edge_pulse_tooluse_tick0() {
    let t = render_with(&pulse_pair_sessions(), 120, 40, true, 0);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_edge_pulse_tooluse_tick5() {
    let t = render_with(&pulse_pair_sessions(), 120, 40, true, 5);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_ascii_fallback_spinner_running_node() {
    let t = render_with(&three_agent_sessions(), 120, 40, false, 0);
    assert_snapshot!(t.backend());
}
