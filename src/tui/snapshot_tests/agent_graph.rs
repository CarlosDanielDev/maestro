use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use crate::session::types::{Session, SessionStatus};
use crate::tui::agent_graph::model::build_graph;
use crate::tui::agent_graph::render::draw_agent_graph;
use crate::tui::snapshot_tests::{fixed_end, fixed_start};

fn three_agent_sessions() -> Vec<Session> {
    let mut s1 = Session::new(
        "Implement login".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(101),
    );
    s1.id = Uuid::nil();
    s1.status = SessionStatus::Running;
    s1.started_at = Some(fixed_start());
    s1.finished_at = Some(fixed_end());
    s1.files_touched = vec![
        "src/auth/login.rs".to_string(),
        "src/auth/token.rs".to_string(),
    ];

    let mut s2 = Session::new(
        "Add dashboard".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(102),
    );
    s2.id = Uuid::from_u128(1);
    s2.status = SessionStatus::Running;
    s2.started_at = Some(fixed_start());
    s2.finished_at = Some(fixed_end());
    s2.files_touched = vec![
        "src/tui/dashboard.rs".to_string(),
        "src/auth/token.rs".to_string(),
    ];

    let mut s3 = Session::new(
        "Fix config".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(103),
    );
    s3.id = Uuid::from_u128(2);
    s3.status = SessionStatus::Completed;
    s3.started_at = Some(fixed_start());
    s3.finished_at = Some(fixed_end());
    s3.files_touched = vec!["src/config.rs".to_string()];

    vec![s1, s2, s3]
}

fn single_agent_session() -> Vec<Session> {
    let mut s = Session::new(
        "Solo task".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(200),
    );
    s.id = Uuid::nil();
    s.status = SessionStatus::Running;
    s.started_at = Some(fixed_start());
    s.finished_at = Some(fixed_end());
    s.files_touched = vec!["src/main.rs".to_string()];
    vec![s]
}

fn render_graph(sessions: &[Session], width: u16, height: u16) -> Terminal<TestBackend> {
    let (nodes, edges) = build_graph(sessions);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            draw_agent_graph(f, f.area(), &nodes, &edges, /* use_braille = */ false);
        })
        .unwrap();
    terminal
}

#[test]
fn agent_graph_renders_at_80x24() {
    let sessions = three_agent_sessions();
    let t = render_graph(&sessions, 80, 24);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_renders_at_100x30() {
    let sessions = three_agent_sessions();
    let t = render_graph(&sessions, 100, 30);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_renders_at_120x40() {
    let sessions = three_agent_sessions();
    let t = render_graph(&sessions, 120, 40);
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_renders_single_agent_card() {
    let sessions = single_agent_session();
    let t = render_graph(&sessions, 80, 24);
    assert_snapshot!(t.backend());
}
