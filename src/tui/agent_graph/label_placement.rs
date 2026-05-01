//! Pure helpers for placing the agent's `#NNN` label without overlapping the
//! outbound edges from the same agent.
//!
//! The algorithm: given the angles of all outbound edges from an agent, pick
//! the angle that sits at the midpoint of the largest angular gap between
//! edges. With no edges, default to north (π/2). See issue #567 for the
//! collision case this fixes.

use std::f64::consts::{FRAC_PI_2, TAU};

use ratatui::layout::Alignment;

use super::layout::CELL_ASPECT;

/// A point in canvas virtual coordinates ([-1.0, 1.0]).
#[derive(Clone, Copy, Debug)]
pub(super) struct CanvasPoint {
    pub(super) x: f64,
    pub(super) y: f64,
}

/// Approximate canvas-cell width in virtual units. Used to convert a label's
/// character count into an x-offset for centering. The terminal-cell width is
/// `2.0 / inner_cols` — about 0.026 at 80 columns, 0.017 at 120. This constant
/// is a midpoint that keeps centering visually balanced; exact pixel-perfect
/// centering is not required, only "the label does not overlap the edge".
const APPROX_CELL_WIDTH: f64 = 0.022;

/// Choose where to render an agent's label, given the agent's center, the
/// canvas points its outbound edges run to, the radial distance the label
/// should sit from the agent, and the label's character width.
///
/// Returns the `(anchor_x, anchor_y)` to pass to `Context::print` (which
/// anchors `x` at the leftmost cell of the rendered line). The anchor is
/// shifted leftward proportional to `(1 − cos θ) / 2`, so:
/// - east placements (θ ≈ 0) anchor flush-left (label extends right of the
///   agent, away from the western edge bundle),
/// - west placements (θ ≈ π) anchor full-width-left (label extends west),
/// - north/south placements (θ ≈ ±π/2) center around the angle.
///
/// This avoids visually pulling the label back into the edge it was meant
/// to avoid (issue #567 follow-up).
pub(super) fn place_label(
    agent: CanvasPoint,
    targets: &[CanvasPoint],
    radius: f64,
    label_width: usize,
) -> (f64, f64) {
    let theta = choose_label_angle(agent, targets);
    let center_x = agent.x + radius * theta.cos() * CELL_ASPECT;
    let center_y = agent.y + radius * theta.sin();
    let label_width_canvas = (label_width as f64) * APPROX_CELL_WIDTH;
    let anchor_shift = label_width_canvas * (1.0 - theta.cos()) * 0.5;
    (center_x - anchor_shift, center_y)
}

/// The label angle (radians) for the given agent and outbound edge targets.
///
/// With no targets, returns `π/2` (north). Otherwise, finds the largest
/// angular gap between consecutive sorted edge angles (treating the angles
/// as a circular sequence) and returns the midpoint of that gap.
pub(super) fn choose_label_angle(agent: CanvasPoint, targets: &[CanvasPoint]) -> f64 {
    if targets.is_empty() {
        return FRAC_PI_2;
    }

    let mut angles: Vec<f64> = targets
        .iter()
        .map(|t| (t.y - agent.y).atan2(t.x - agent.x))
        .map(normalize_to_two_pi)
        .collect();
    angles.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    safe_label_angle(&angles)
}

/// Pure-trig helper: given outbound edge angles already normalized to
/// `[0, 2π)` and sorted ascending, return the angle at the midpoint of the
/// largest angular gap. Empty input → north (π/2).
///
/// Exposed for unit testing the angle-selection logic in isolation from the
/// canvas-coordinate plumbing.
pub(super) fn safe_label_angle(sorted_angles: &[f64]) -> f64 {
    if sorted_angles.is_empty() {
        return FRAC_PI_2;
    }
    if sorted_angles.len() == 1 {
        // Single edge — the "gap" is the full 2π circle minus zero, midpoint
        // is the antipodal angle.
        return normalize_to_two_pi(sorted_angles[0] + std::f64::consts::PI);
    }

    let n = sorted_angles.len();
    let mut best_gap = -1.0_f64;
    let mut best_mid = FRAC_PI_2;

    for i in 0..n {
        let a = sorted_angles[i];
        let b = if i + 1 < n {
            sorted_angles[i + 1]
        } else {
            sorted_angles[0] + TAU
        };
        let gap = b - a;
        if gap > best_gap + 1e-12 {
            best_gap = gap;
            best_mid = normalize_to_two_pi((a + b) * 0.5);
        } else if (gap - best_gap).abs() <= 1e-12 {
            // Tie-break: prefer the gap whose midpoint is closer to north
            // (π/2). This keeps placement deterministic across runs and
            // matches the snapshot-friendly default.
            let candidate = normalize_to_two_pi((a + b) * 0.5);
            if angular_distance(candidate, FRAC_PI_2)
                < angular_distance(best_mid, FRAC_PI_2) - 1e-12
            {
                best_mid = candidate;
            }
        }
    }
    best_mid
}

fn normalize_to_two_pi(theta: f64) -> f64 {
    let mut x = theta % TAU;
    if x < 0.0 {
        x += TAU;
    }
    x
}

/// Half-width of the center band (in canvas units) where a file label is
/// rendered centered on its marker rather than anchored to one side. Files
/// near the top / bottom of the ring sit here.
const FILE_LABEL_DEAD_BAND: f64 = 0.05;

/// Cells reserved between the rendered label and the nearest canvas border
/// so labels never touch the frame. Subtracted from the raw outward span.
const FILE_LABEL_BORDER_MARGIN_CELLS: f64 = 1.0;

/// Place a FILE label so it grows OUTWARD from the canvas center, away from
/// the marker, with ellipsis truncation when it would overshoot the canvas
/// border. Y stays at the caller's `p.y - 0.08` (or whatever offset they
/// chose); only the x-anchor and possibly-truncated label are returned.
///
/// Rule:
/// - `p.x >  DEAD_BAND` (right half) → anchor at `p.x` (label extends right;
///   `Alignment::Left` relative to the marker).
/// - `p.x < -DEAD_BAND` (left half) → anchor at `p.x − width` (right edge of
///   label sits at the marker; `Alignment::Right`).
/// - `|p.x| ≤ DEAD_BAND` (top/bottom of the ring) → centered on the marker.
///
/// Truncation: if the label cannot fit in the outward span
/// (`inner_cols * (1 - |p.x|) / 2 - margin` cells) it is truncated to
/// `available - 1` chars and an ellipsis is appended, preserving the leading
/// characters.
pub(super) fn place_file_label(p: CanvasPoint, label: &str, inner_cols: u16) -> (f64, String) {
    let inner_cols_f = (inner_cols as f64).max(1.0);
    let cell_w = 2.0 / inner_cols_f;

    let (align, available_cells) = if p.x > FILE_LABEL_DEAD_BAND {
        let raw = inner_cols_f * (1.0 - p.x) / 2.0 - FILE_LABEL_BORDER_MARGIN_CELLS;
        (Alignment::Left, raw.floor().max(1.0) as usize)
    } else if p.x < -FILE_LABEL_DEAD_BAND {
        let raw = inner_cols_f * (1.0 + p.x) / 2.0 - FILE_LABEL_BORDER_MARGIN_CELLS;
        (Alignment::Right, raw.floor().max(1.0) as usize)
    } else {
        let raw = inner_cols_f - 2.0 * FILE_LABEL_BORDER_MARGIN_CELLS;
        (Alignment::Center, raw.floor().max(1.0) as usize)
    };

    let (display, width) = truncate_label_to_cells(label, available_cells);
    let width_f = width as f64;

    let anchor_x = match align {
        Alignment::Left => p.x,
        Alignment::Right => p.x - width_f * cell_w,
        Alignment::Center => p.x - width_f * cell_w * 0.5,
    };

    (anchor_x, display)
}

/// Char-count truncation with `…` suffix. Returns the truncated label and its
/// char count, so callers don't re-walk the string for width.
fn truncate_label_to_cells(label: &str, available_cells: usize) -> (String, usize) {
    let len = label.chars().count();
    if len <= available_cells {
        return (label.to_string(), len);
    }
    if available_cells <= 1 {
        return ("…".to_string(), 1);
    }
    let keep = available_cells - 1;
    let mut out: String = label.chars().take(keep).collect();
    out.push('…');
    (out, available_cells)
}

fn angular_distance(a: f64, b: f64) -> f64 {
    let raw = (a - b).abs() % TAU;
    raw.min(TAU - raw)
}

#[cfg(test)]
#[path = "label_placement_tests.rs"]
mod tests;
