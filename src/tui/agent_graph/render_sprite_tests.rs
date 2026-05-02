//! Geometry tests for `draw_sprite_on_canvas` across viewport sizes.
//!
//! Issue #576 split: behavioral assertions for sprite contiguity, height
//! budget, and horizontal centering live here so `render_tests.rs` stays
//! within the 400-line file-size guard.

use ratatui::{Terminal, backend::TestBackend, style::Color};
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

fn render_buffer_sized(
    sessions: &[&Session],
    width: u16,
    height: u16,
    tick: usize,
    use_nerd_font: bool,
) -> ratatui::buffer::Buffer {
    let (nodes, edges) = build_graph(sessions);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            draw_agent_graph(f, f.area(), &nodes, &edges, use_nerd_font, tick, sessions);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

/// Map a canvas-space x to its column index in the rendered buffer (including
/// the 1-cell border on the left). Mirrors ratatui 0.29's Canvas label-print
/// mapping (`(x - left) * (inner_cols - 1) / width`) for `x_bounds = [-1, 1]`.
fn agent_col(cx: f64, inner_cols: u16) -> u16 {
    let inner = inner_cols.saturating_sub(1) as f64;
    1 + ((cx + 1.0) / 2.0 * inner) as u16
}

/// Coordinates in `buf` of cells whose foreground equals `color` and whose
/// symbol is non-blank, restricted to columns in `[col_lo, col_hi]`. Used to
/// isolate the agent sprite (Implementer = Green) from neighboring labels and
/// edge glyphs (Cyan).
fn sprite_cells(
    buf: &ratatui::buffer::Buffer,
    color: Color,
    col_lo: u16,
    col_hi: u16,
) -> Vec<(u16, u16)> {
    let mut cells = Vec::new();
    let max_x = col_hi.min(buf.area.width.saturating_sub(1));
    for y in 0..buf.area.height {
        for x in col_lo..=max_x {
            let cell = &buf[(x, y)];
            if cell.style().fg == Some(color) && cell.symbol() != " " {
                cells.push((x, y));
            }
        }
    }
    cells
}

fn issue_576_session() -> Session {
    make_session_with(0, 576, SessionStatus::Running, &["src/main.rs"])
}

/// Single-agent + 1-file graph at canvas (0.225, 0). Agent sprite must render
/// as 6 contiguous cell rows on every viewport. Pre-fix `ROW_STEP = 0.1` is
/// constant in canvas units; on viewports with cell height < 0.1 (e.g.
/// 120×40 at 0.053 and 200×60 at 0.034), `ctx.print` lands consecutive sprite
/// rows on non-adjacent buffer rows, producing 1- to 3-row gaps that visually
/// fragment the sprite.
fn assert_sprite_contiguous(width: u16, height: u16) {
    let s = issue_576_session();
    let buf = render_buffer_sized(&[&s], width, height, 0, true);

    let inner_cols = width.saturating_sub(2);
    let center = agent_col(0.225, inner_cols);
    let col_lo = center.saturating_sub(5);
    let col_hi = center + 5;

    let cells = sprite_cells(&buf, Color::Green, col_lo, col_hi);
    assert!(
        !cells.is_empty(),
        "no Green sprite cells found at {width}x{height} in cols [{col_lo}, {col_hi}]"
    );

    let mut sprite_rows: Vec<u16> = cells.iter().map(|&(_, y)| y).collect();
    sprite_rows.sort_unstable();
    sprite_rows.dedup();

    for window in sprite_rows.windows(2) {
        assert_eq!(
            window[1],
            window[0] + 1,
            "sprite gap at {width}x{height}: row {} not followed by {} (gap {}); rows: {sprite_rows:?}",
            window[0],
            window[0] + 1,
            window[1] - window[0]
        );
    }
}

#[test]
fn sprite_no_gap_between_rows_at_80x24() {
    assert_sprite_contiguous(80, 24);
}

#[test]
fn sprite_no_gap_between_rows_at_120x40() {
    assert_sprite_contiguous(120, 40);
}

#[test]
fn sprite_no_gap_between_rows_at_200x60() {
    assert_sprite_contiguous(200, 60);
}

#[test]
fn sprite_height_within_30_percent_of_inner_rows_at_120x40() {
    let s = issue_576_session();
    let buf = render_buffer_sized(&[&s], 120, 40, 0, true);

    let inner_cols: u16 = 118;
    let inner_rows: u16 = 38;
    let center = agent_col(0.225, inner_cols);
    let col_lo = center.saturating_sub(5);
    let col_hi = center + 5;

    let cells = sprite_cells(&buf, Color::Green, col_lo, col_hi);
    assert!(!cells.is_empty(), "no sprite cells at 120x40");

    let min_row = cells.iter().map(|&(_, y)| y).min().unwrap();
    let max_row = cells.iter().map(|&(_, y)| y).max().unwrap();
    let bbox_height = max_row - min_row + 1;

    let budget = (inner_rows as f64 * 0.30).floor() as u16;
    assert!(
        bbox_height <= budget,
        "sprite bbox height {bbox_height} exceeds 30% budget ({budget}) of inner_rows={inner_rows}"
    );
}

fn assert_sprite_centered(width: u16, height: u16) {
    let s = issue_576_session();
    let buf = render_buffer_sized(&[&s], width, height, 0, true);

    let inner_cols = width.saturating_sub(2);
    let expected_center = agent_col(0.225, inner_cols);
    let col_lo = expected_center.saturating_sub(5);
    let col_hi = expected_center + 5;

    let cells = sprite_cells(&buf, Color::Green, col_lo, col_hi);
    assert!(
        !cells.is_empty(),
        "no sprite cells in search window at {width}x{height}"
    );

    let min_col = cells.iter().map(|&(x, _)| x).min().unwrap();
    let max_col = cells.iter().map(|&(x, _)| x).max().unwrap();
    let actual_center = (min_col + max_col) / 2;

    let delta = actual_center.abs_diff(expected_center);
    assert!(
        delta <= 1,
        "sprite column-center {actual_center} is {delta} cells from expected {expected_center} at {width}x{height} (tolerance ±1)"
    );
}

#[test]
fn sprite_horizontally_centered_within_1_cell_at_80x24() {
    assert_sprite_centered(80, 24);
}

#[test]
fn sprite_horizontally_centered_within_1_cell_at_120x40() {
    assert_sprite_centered(120, 40);
}

#[test]
fn sprite_horizontally_centered_within_1_cell_at_200x60() {
    assert_sprite_centered(200, 60);
}
