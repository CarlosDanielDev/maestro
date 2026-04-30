//! Snapshot tests for the agent-personalities visual identity.
//!
//! Pins the rendered Buffer at fixed 80×24 across all 5 roles × 2 icon modes,
//! plus a transition-stability test asserting that switching status from
//! `Running` to `Completed` does not move sprite glyph bytes — only the style
//! modifier changes.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend, style::Modifier};
use uuid::Uuid;

use crate::session::role::Role;
use crate::session::types::{Session, SessionStatus};
use crate::tui::agent_graph::model::build_graph;
use crate::tui::agent_graph::render::draw_agent_graph;
use crate::tui::snapshot_tests::{fixed_end, fixed_start};

/// Build a session with an explicit role override (bypasses `derive_role`).
fn make_session_with_role(role: Role, status: SessionStatus, id: u128) -> Session {
    let mut s = Session::new(
        "Implement feature X".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(539),
        Some(role),
    );
    s.id = Uuid::from_u128(id);
    s.status = status;
    s.started_at = Some(fixed_start());
    s.finished_at = Some(fixed_end());
    s.cost_usd = 0.0;
    s.context_pct = 0.0;
    s.current_activity = String::new();
    s.last_message = String::new();
    s
}

/// Render the agent-graph view with one role-bearing primary session and a
/// secondary queued Implementer (so we exercise the multi-agent path, not the
/// single-agent card fallback).
fn render_for_role(
    role: Role,
    status: SessionStatus,
    use_nerd_font: bool,
) -> Terminal<TestBackend> {
    let primary = make_session_with_role(role, status, 0);
    let secondary = make_session_with_role(Role::Implementer, SessionStatus::Queued, 1);
    let refs: Vec<&Session> = vec![&primary, &secondary];
    let (nodes, edges) = build_graph(&refs);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| {
            draw_agent_graph(f, f.area(), &nodes, &edges, use_nerd_font, 0, &refs);
        })
        .unwrap();
    terminal
}

// ── 5 roles × 2 icon modes = 10 snapshots ────────────────────────────────────

#[test]
fn snapshot_implementer_nerd_font() {
    let t = render_for_role(Role::Implementer, SessionStatus::Running, true);
    assert_snapshot!(t.backend());
}

#[test]
fn snapshot_implementer_ascii() {
    let t = render_for_role(Role::Implementer, SessionStatus::Running, false);
    assert_snapshot!(t.backend());
}

#[test]
fn snapshot_orchestrator_nerd_font() {
    let t = render_for_role(Role::Orchestrator, SessionStatus::Running, true);
    assert_snapshot!(t.backend());
}

#[test]
fn snapshot_orchestrator_ascii() {
    let t = render_for_role(Role::Orchestrator, SessionStatus::Running, false);
    assert_snapshot!(t.backend());
}

#[test]
fn snapshot_reviewer_nerd_font() {
    let t = render_for_role(Role::Reviewer, SessionStatus::Running, true);
    assert_snapshot!(t.backend());
}

#[test]
fn snapshot_reviewer_ascii() {
    let t = render_for_role(Role::Reviewer, SessionStatus::Running, false);
    assert_snapshot!(t.backend());
}

#[test]
fn snapshot_docs_nerd_font() {
    let t = render_for_role(Role::Docs, SessionStatus::Running, true);
    assert_snapshot!(t.backend());
}

#[test]
fn snapshot_docs_ascii() {
    let t = render_for_role(Role::Docs, SessionStatus::Running, false);
    assert_snapshot!(t.backend());
}

#[test]
fn snapshot_devops_nerd_font() {
    let t = render_for_role(Role::DevOps, SessionStatus::Running, true);
    assert_snapshot!(t.backend());
}

#[test]
fn snapshot_devops_ascii() {
    let t = render_for_role(Role::DevOps, SessionStatus::Running, false);
    assert_snapshot!(t.backend());
}

// ── transition stability: glyph stable across Running → Completed ────────────

#[test]
fn snapshot_running_to_completed_sprite_stable() {
    // Use Reviewer (Magenta) for the primary so its sprite cells are
    // distinguishable by color from the secondary Implementer (Green).
    use ratatui::style::Color;

    let running = render_for_role(Role::Reviewer, SessionStatus::Running, true);
    let completed = render_for_role(Role::Reviewer, SessionStatus::Completed, true);

    let buf_running = running.backend().buffer().clone();
    let buf_completed = completed.backend().buffer().clone();

    // Find primary's sprite cells by Magenta foreground; assert the footprint
    // (set of (x, y) positions) is identical between Running and Completed.
    // Only the style modifier should differ.
    let primary_cells = |buf: &ratatui::buffer::Buffer| -> Vec<(u16, u16)> {
        let mut out = Vec::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                if cell.symbol() == "█" && cell.style().fg == Some(Color::Magenta) {
                    out.push((x, y));
                }
            }
        }
        out
    };

    let cells_running = primary_cells(&buf_running);
    let cells_completed = primary_cells(&buf_completed);

    assert!(
        !cells_running.is_empty(),
        "Running Reviewer render must contain Magenta sprite cells"
    );
    assert_eq!(
        cells_running, cells_completed,
        "primary sprite footprint (set of Magenta '█' cells) must be identical \
         between Running and Completed — only the modifier should differ"
    );

    // Spot-check the modifier difference on the first sprite cell.
    let (sx, sy) = cells_running[0];
    let mod_running = buf_running[(sx, sy)].style().add_modifier;
    let mod_completed = buf_completed[(sx, sy)].style().add_modifier;
    assert!(
        mod_running.contains(Modifier::BOLD),
        "Running primary sprite cell at ({sx},{sy}) must carry BOLD; got {:?}",
        mod_running
    );
    assert!(
        mod_completed.contains(Modifier::DIM),
        "Completed primary sprite cell at ({sx},{sy}) must carry DIM; got {:?}",
        mod_completed
    );
}
