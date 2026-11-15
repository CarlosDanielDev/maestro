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
