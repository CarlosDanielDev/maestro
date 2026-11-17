use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use super::*;

/// Every segment on: a fleet, a capped budget, one quota badge,
/// TurboQuant, both safety chips and an update.
fn full_model() -> StatusModel {
    StatusModel {
        version: "0.0.0".to_string(),
        fleet: FleetCounts {
            running: 2,
            waiting: 1,
            failed: 1,
            total: 5,
        },
        spent: 12.4,
        cap: Some(50.0),
        quota: vec!["claude 42% ↻45m →".to_string()],
        turboquant: true,
        untrusted: true,
        bypass: true,
        update: Some("0.34.0".to_string()),
    }
}

fn render(model: &StatusModel, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
    let theme = Theme::dark();
    terminal
        .draw(|f| draw_for_mode(f, f.area(), model, &theme, true))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..width).map(|x| buffer[(x, 0)].symbol()).collect()
}

#[test]
fn status_bar_150() {
    let row = render(&full_model(), 150);
    for part in [
        "MAESTRO",
        "2 run",
        "1 wait",
        "1 fail",
        "$12.40/50",
        "claude 42%",
        "TQ:ON",
        "▲ BYPASS",
        "UNTRUSTED",
        "↑ 0.34.0 u",
    ] {
        assert!(row.contains(part), "{part:?} missing at 150: {row:?}");
    }
    assert_snapshot!(row);
}

#[test]
fn status_bar_100() {
    let row = render(&full_model(), 100);
    assert!(!row.contains("0.34.0"), "update drops first: {row:?}");
    assert!(row.contains("claude 42%"), "quota stays at 100: {row:?}");
    assert_snapshot!(row);
}

#[test]
fn status_bar_80() {
    let row = render(&full_model(), 80);
    assert!(!row.contains("0.34.0"), "update dropped: {row:?}");
    assert!(!row.contains("claude"), "quota dropped at 80: {row:?}");
    assert!(row.contains("$12.40/50"), "budget stays at 80: {row:?}");
    assert_snapshot!(row);
}

#[test]
fn status_bar_never_drops_safety_chips() {
    let mut model = full_model();
    model.quota = vec!["a-very-long-provider-name 99% ↻6d23h ↓ stale".to_string(); 3];
    model.spent = 123_456.78;
    model.cap = Some(999_999.0);
    for width in [80, 60, 40, 30] {
        let row = render(&model, width);
        assert!(
            row.contains("▲ BYPASS"),
            "BYPASS dropped at {width}: {row:?}"
        );
        assert!(
            row.contains("UNTRUSTED"),
            "UNTRUSTED dropped at {width}: {row:?}"
        );
    }
}

#[test]
fn status_bar_hides_chips_when_off() {
    let mut model = full_model();
    model.bypass = false;
    model.untrusted = false;
    model.update = None;
    let row = render(&model, 150);
    assert!(!row.contains("BYPASS"), "{row:?}");
    assert!(!row.contains("UNTRUSTED"), "{row:?}");
    assert!(!row.contains('↑'), "{row:?}");
}

#[test]
fn status_bar_renders_on_surface_bar() {
    let theme = Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(150, 1)).unwrap();
    let model = StatusModel {
        version: "0.0.0".to_string(),
        ..StatusModel::default()
    };
    terminal
        .draw(|f| draw_for_mode(f, f.area(), &model, &theme, true))
        .unwrap();
    let buffer = terminal.backend().buffer();
    // The last column is padding: no segment ever reaches it here.
    assert_eq!(buffer[(149, 0)].bg, theme.surface_bar);
    assert_eq!(buffer[(60, 0)].bg, theme.surface_bar);
}

#[test]
fn status_bar_no_color() {
    let theme = Theme::no_color();
    let mut terminal = Terminal::new(TestBackend::new(80, 1)).unwrap();
    terminal
        .draw(|f| draw_for_mode(f, f.area(), &full_model(), &theme, false))
        .unwrap();
    let out = format!("{:?}", terminal.backend().buffer());
    assert!(out.contains("UNTRUSTED"), "{out}");
    assert!(out.contains("BYPASS"), "{out}");
    for colour in ["Green", "Yellow", "Red", "White", "Black", "Rgb", "Indexed"] {
        assert!(!out.contains(colour), "NoColor drew {colour}: {out}");
    }
    assert_snapshot!(out);
}

#[test]
fn fleet_counts_group_statuses() {
    use SessionStatus::*;
    let statuses = [
        Running,
        GatesRunning,
        Spawning,
        NeedsReview,
        NeedsPr,
        Interactive,
        FailedGates,
        Errored,
        Completed,
        Queued,
    ];
    let counts = FleetCounts::from_statuses(statuses.iter());
    assert_eq!(
        counts,
        FleetCounts {
            running: 3,
            waiting: 3,
            failed: 2,
            total: 10,
        }
    );
}

#[test]
fn pinned_width_counts_only_safety_chips() {
    let mut model = full_model();
    assert!(pinned_width(&model, true) > 0);
    model.bypass = false;
    model.untrusted = false;
    assert_eq!(pinned_width(&model, true), 0);
}
