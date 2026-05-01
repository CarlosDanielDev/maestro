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
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    /// Minimum angular separation we expect between the chosen label angle
    /// and any input edge angle. ~14° — enough for visual breathing room
    /// at typical terminal sizes.
    const LABEL_EDGE_MIN_GAP: f64 = 0.25;

    fn assert_close(a: f64, b: f64, tol: f64, msg: &str) {
        let d = angular_distance(a, b);
        assert!(d <= tol, "{msg}: |{a:.6} − {b:.6}| = {d:.6} > {tol}");
    }

    #[test]
    fn empty_edges_returns_north() {
        let theta = safe_label_angle(&[]);
        assert_close(theta, FRAC_PI_2, 1e-9, "no edges should default to north");
    }

    #[test]
    fn single_south_edge_returns_north() {
        // South in [0, 2π) is 3π/2.
        let south = 3.0 * FRAC_PI_2;
        let theta = safe_label_angle(&[south]);
        assert_close(
            theta,
            FRAC_PI_2,
            1e-9,
            "single south edge should put label at north",
        );
        assert!(
            angular_distance(theta, south) > LABEL_EDGE_MIN_GAP,
            "label too close to south edge"
        );
    }

    #[test]
    fn single_north_edge_returns_south() {
        let theta = safe_label_angle(&[FRAC_PI_2]);
        assert_close(
            theta,
            3.0 * FRAC_PI_2,
            1e-9,
            "single north edge should put label at south",
        );
    }

    #[test]
    fn two_opposed_edges_east_west_picks_perpendicular() {
        // Edges at 0 (east) and π (west). Two equal half-circles — tie-break
        // picks the gap midpoint closest to north.
        let theta = safe_label_angle(&[0.0, PI]);
        assert_close(theta, FRAC_PI_2, 1e-9, "tie-break should prefer north");
        assert!(angular_distance(theta, 0.0) > LABEL_EDGE_MIN_GAP);
        assert!(angular_distance(theta, PI) > LABEL_EDGE_MIN_GAP);
    }

    #[test]
    fn three_clustered_east_edges_pick_west() {
        // Three edges near east. Largest gap wraps around the back —
        // midpoint is near west (π).
        let angles = [0.1_f64, 0.2, 0.3];
        let theta = safe_label_angle(&angles);
        assert_close(theta, PI, 0.4, "clustered east edges should put label west");
        for &a in &angles {
            assert!(
                angular_distance(theta, a) > LABEL_EDGE_MIN_GAP,
                "label {theta:.4} too close to clustered edge {a:.4}"
            );
        }
    }

    /// Issue #567 canonical test hint: file at exactly 270° (south).
    /// The label angle must be far from the edge angle.
    #[test]
    fn file_at_270_degrees_label_not_near_south() {
        let south = 270.0_f64.to_radians(); // 3π/2
        let theta = safe_label_angle(&[south]);
        assert!(
            angular_distance(theta, south) > LABEL_EDGE_MIN_GAP,
            "label angle {theta:.4} only {:.4} rad from south edge {south:.4}",
            angular_distance(theta, south)
        );
    }

    #[test]
    fn choose_label_angle_uses_canvas_geometry() {
        // Agent at origin, target at (0, -0.5) → atan2(-0.5, 0) = -π/2 = south.
        let agent = CanvasPoint { x: 0.0, y: 0.0 };
        let target = CanvasPoint { x: 0.0, y: -0.5 };
        let theta = choose_label_angle(agent, &[target]);
        assert_close(
            theta,
            FRAC_PI_2,
            1e-9,
            "south target should produce north label",
        );
    }

    #[test]
    fn place_label_centers_when_north() {
        // No targets → north (cos θ = 0). Label width 6 chars → anchor
        // shifts left by full label_width × (1 − 0)/2 = half_width.
        let agent = CanvasPoint { x: 0.0, y: 0.0 };
        let radius = 0.4;
        let label_width = 6;
        let (ax, ay) = place_label(agent, &[], radius, label_width);
        let expected_y = radius;
        let expected_x = -3.0 * APPROX_CELL_WIDTH;
        assert!(
            (ay - expected_y).abs() < 1e-9,
            "anchor_y should be at radius (north): got {ay} expected {expected_y}"
        );
        assert!(
            (ax - expected_x).abs() < 1e-9,
            "north anchor_x should be center − half_width: got {ax} expected {expected_x}"
        );
    }

    #[test]
    fn place_label_anchors_flush_left_when_east() {
        // Single edge due-west forces label angle to east (cos θ = 1).
        // anchor_shift = label_width × (1 − 1)/2 = 0 → anchor_x = center_x.
        let agent = CanvasPoint { x: 0.0, y: 0.0 };
        let west_target = CanvasPoint { x: -0.5, y: 0.0 };
        let radius = 0.4;
        let (ax, _ay) = place_label(agent, &[west_target], radius, 12);
        let expected = radius * CELL_ASPECT;
        assert!(
            (ax - expected).abs() < 1e-9,
            "east anchor should sit at agent + radius × CELL_ASPECT (label extends right): got {ax} expected {expected}"
        );
    }

    #[test]
    fn place_label_anchors_full_width_left_when_west() {
        // Single edge due-east forces label angle to west (cos θ = −1).
        // anchor_shift = label_width × (1 − (−1))/2 = label_width.
        let agent = CanvasPoint { x: 0.0, y: 0.0 };
        let east_target = CanvasPoint { x: 0.5, y: 0.0 };
        let radius = 0.4;
        let label_width = 10;
        let (ax, _ay) = place_label(agent, &[east_target], radius, label_width);
        let center_x = -radius * CELL_ASPECT; // west placement
        let expected = center_x - (label_width as f64) * APPROX_CELL_WIDTH;
        assert!(
            (ax - expected).abs() < 1e-9,
            "west anchor should sit at center − full label width (label extends left): got {ax} expected {expected}"
        );
    }

    #[test]
    fn output_is_deterministic_across_runs() {
        let angles = [0.0_f64, PI / 3.0, 2.0 * PI / 3.0, PI, 4.0 * PI / 3.0];
        let first = safe_label_angle(&angles);
        for _ in 0..50 {
            assert_close(
                safe_label_angle(&angles),
                first,
                1e-12,
                "safe_label_angle must be deterministic",
            );
        }
    }

    // --- place_file_label ----------------------------------------------------

    #[test]
    fn place_file_label_right_half_left_anchored_at_marker() {
        // Right-half marker, short label fits outward space. Anchor must equal
        // marker x — label grows rightward (outward) from the marker.
        let p = CanvasPoint { x: 0.7, y: 0.0 };
        let (anchor_x, display) = place_file_label(p, "main.rs", 98);
        assert_eq!(display, "main.rs");
        assert!(
            (anchor_x - 0.7).abs() < 1e-9,
            "right-half anchor must equal p.x; got {anchor_x}"
        );
    }

    #[test]
    fn place_file_label_left_half_right_anchored_at_marker() {
        // Left-half marker, short label fits. Right edge of label sits at marker x,
        // so anchor_x = p.x − label_width_canvas.
        let p = CanvasPoint { x: -0.7, y: 0.0 };
        let label = "auth.rs";
        let inner_cols: u16 = 98;
        let cell_w = 2.0 / inner_cols as f64;
        let (anchor_x, display) = place_file_label(p, label, inner_cols);
        assert_eq!(display, label);
        let expected = p.x - (label.chars().count() as f64) * cell_w;
        assert!(
            (anchor_x - expected).abs() < 1e-9,
            "left-half anchor must be p.x − label_width_canvas; got {anchor_x} expected {expected}"
        );
    }

    #[test]
    fn place_file_label_center_band_centered() {
        // |p.x| < 0.05 — center band (top/bottom of the file ring). Label
        // centered horizontally on the marker.
        let p = CanvasPoint { x: 0.0, y: 0.9 };
        let label = "types.rs";
        let inner_cols: u16 = 98;
        let cell_w = 2.0 / inner_cols as f64;
        let (anchor_x, display) = place_file_label(p, label, inner_cols);
        assert_eq!(display, label);
        let half = (label.chars().count() as f64) * cell_w / 2.0;
        assert!(
            (anchor_x - (p.x - half)).abs() < 1e-9,
            "center-band anchor must be p.x − half_label_canvas; got {anchor_x}"
        );
    }

    #[test]
    fn place_file_label_truncates_long_label_right_half() {
        let p = CanvasPoint { x: 0.6, y: 0.0 };
        let long_label = "a".repeat(80);
        let inner_cols: u16 = 98;
        let outward_cells = (inner_cols as f64 * (1.0 - p.x) / 2.0 - 1.0).floor() as usize;
        let (anchor_x, display) = place_file_label(p, &long_label, inner_cols);
        assert!(display.ends_with('…'), "truncated label must end with '…'");
        assert!(
            display.chars().count() <= outward_cells,
            "truncated label ({} chars) must fit in outward cells ({outward_cells})",
            display.chars().count()
        );
        assert!(
            (anchor_x - p.x).abs() < 1e-9,
            "right-half anchor must still equal p.x after truncation"
        );
    }

    #[test]
    fn place_file_label_truncates_long_label_left_half() {
        let p = CanvasPoint { x: -0.6, y: 0.0 };
        let long_label = "b".repeat(80);
        let inner_cols: u16 = 98;
        let outward_cells = (inner_cols as f64 * (1.0 + p.x) / 2.0 - 1.0).floor() as usize;
        let (_anchor_x, display) = place_file_label(p, &long_label, inner_cols);
        assert!(display.ends_with('…'), "truncated label must end with '…'");
        assert!(
            display.chars().count() <= outward_cells,
            "truncated label ({} chars) must fit in outward cells ({outward_cells})",
            display.chars().count()
        );
    }

    #[test]
    fn place_file_label_preserves_prefix() {
        // Outward cells at p.x = 0.5, inner_cols = 98 → floor(24.5 - 1) = 23.
        // Truncation keeps 22 leading chars + '…' (matches the recognizable
        // module-path prefix from the issue body example).
        let p = CanvasPoint { x: 0.5, y: 0.0 };
        let label = "maestro__tui__snapshot_tests__some_long_test_name";
        let inner_cols: u16 = 98;
        let (_anchor_x, display) = place_file_label(p, label, inner_cols);
        assert!(
            display.ends_with('…'),
            "long label must be truncated with '…'"
        );
        assert!(
            display.starts_with("maestro__tui__snapshot"),
            "truncated label must preserve leading prefix; got: {display}"
        );
    }

    #[test]
    fn place_file_label_no_truncation_when_fits() {
        // Short label at center; outward space is generous. No ellipsis.
        let p = CanvasPoint { x: 0.0, y: 0.9 };
        let label = "foo.rs";
        let (_anchor_x, display) = place_file_label(p, label, 98);
        assert_eq!(display, label, "fitting label must be returned unchanged");
        assert!(!display.contains('…'), "no ellipsis for a label that fits");
    }

    #[test]
    fn place_file_label_handles_minimum_inner_cols() {
        // 80-col viewport minus 2 borders = 78 inner cols; p.x = 0.9 →
        // outward space ≈ floor(78 * 0.1 / 2 - 1) = 2 cells. Output must fit.
        let p = CanvasPoint { x: 0.9, y: 0.0 };
        let inner_cols: u16 = 78;
        let outward_cells = (inner_cols as f64 * (1.0 - p.x) / 2.0 - 1.0).floor() as usize;
        let label = "config.rs";
        let (_anchor_x, display) = place_file_label(p, label, inner_cols);
        assert!(
            display.chars().count() <= outward_cells.max(1),
            "output ({} chars) must fit in tight outward space ({outward_cells} cells)",
            display.chars().count()
        );
    }
}
