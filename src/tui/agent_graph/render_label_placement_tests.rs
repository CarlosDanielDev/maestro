//! Regression tests for issue #567 — agent label placement must not overlap
//! the outbound edges from the same agent. Kept in a separate file to keep
//! `render_tests.rs` under the 400-line cap (`scripts/check-file-size.sh`).

use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use super::draw_agent_graph;
use crate::session::types::{Session, SessionStatus};
use crate::tui::agent_graph::model::build_graph;

fn make_session_with(id: u128, issue: u64, status: SessionStatus, files: &[&str]) -> Session {
    let mut s = Session::new(
        "task".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(issue),
        None,
    );
    s.id = Uuid::from_u128(id);
    s.status = status;
    s.files_touched = files.iter().map(|f| (*f).to_string()).collect();
    s
}

fn render_buffer(
    sessions: &[&Session],
    tick: usize,
    use_nerd_font: bool,
) -> ratatui::buffer::Buffer {
    let (nodes, edges) = build_graph(sessions);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|f| {
            draw_agent_graph(f, f.area(), &nodes, &edges, use_nerd_font, tick, sessions);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn label_row_in_buffer(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<u16> {
    let mut text_per_row: Vec<String> = Vec::with_capacity(buf.area.height as usize);
    for y in 0..buf.area.height {
        let mut row = String::with_capacity(buf.area.width as usize);
        for x in 0..buf.area.width {
            row.push_str(buf[(x, y)].symbol());
        }
        text_per_row.push(row);
    }
    text_per_row
        .iter()
        .enumerate()
        .find(|(_, line)| line.contains(needle))
        .map(|(i, _)| i as u16)
}

// ── Issue #567 ──────────────────────────────────────────────────────────────
//
// The layout for 1 agent + 2 files places the second file at exactly 270°
// (south of the agent at angle 0). Pre-fix the label was painted at
// `(p.x, p.y - 0.35)` — directly on the south edge. After the fix the label
// is at the midpoint of the largest angular gap between outbound edges,
// which for this geometry sits in the upper half of the canvas.

#[test]
fn agent_label_renders_above_mid_when_file_is_south_nerd_font() {
    let s = make_session_with(0, 567, SessionStatus::Running, &["src/a.rs", "src/b.rs"]);
    let buf = render_buffer(&[&s], 0, true);

    let label_row = label_row_in_buffer(&buf, "#567")
        .expect("#567 label must appear somewhere in the rendered buffer");

    let mid_row = buf.area.height / 2;
    assert!(
        label_row < mid_row,
        "label row {label_row} should be above mid_row {mid_row} (north of agent); \
         pre-fix the label rendered at p.y - 0.35 (south), overlapping the edge \
         to the file at 270°"
    );
}

#[test]
fn agent_label_renders_above_mid_when_file_is_south_ascii() {
    let s = make_session_with(0, 567, SessionStatus::Running, &["src/a.rs", "src/b.rs"]);
    let buf = render_buffer(&[&s], 0, false);

    let label_row =
        label_row_in_buffer(&buf, "#567").expect("#567 label must appear in ASCII buffer");

    let mid_row = buf.area.height / 2;
    assert!(
        label_row < mid_row,
        "ASCII mode: label row {label_row} should be above mid_row {mid_row}"
    );
}
