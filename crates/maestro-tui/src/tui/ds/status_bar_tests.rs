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
        update: Some(UpdateChip::Available("0.34.0".to_string())),
        continuous: None,
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
    let counts = FleetCounts::from_sessions(statuses.iter().map(|s| (s, Place::Active)));
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

#[test]
fn status_bar_shows_continuous_progress() {
    let mut model = full_model();
    model.continuous = Some((3, 7));
    let row = render(&model, 150);
    assert!(row.contains("CONTINUOUS: 3/7 done"), "{row:?}");
    assert_snapshot!(row);

    model.continuous = None;
    let row = render(&model, 150);
    assert!(
        !row.contains("CONTINUOUS"),
        "only while a run is active: {row:?}"
    );
}

#[test]
fn continuous_drops_after_quota_before_budget() {
    let mut model = full_model();
    model.continuous = Some((3, 7));
    // Room for brand, fleet, budget and the run chip, but not for quota.
    let row = render(&model, 110);
    assert!(!row.contains("claude"), "quota drops first: {row:?}");
    assert!(row.contains("CONTINUOUS: 3/7 done"), "{row:?}");
    assert!(row.contains("$12.40/50"), "{row:?}");
}

#[test]
fn continuous_chip_reads_without_colour() {
    let theme = Theme::no_color();
    let mut model = full_model();
    model.continuous = Some((3, 7));
    let (left, _) = segments(&model, 150, &theme, false);
    let chip = left
        .iter()
        .find(|s| s.content.contains("CONTINUOUS"))
        .expect("continuous chip drawn");
    assert!(
        chip.style
            .add_modifier
            .contains(ratatui::style::Modifier::REVERSED | ratatui::style::Modifier::BOLD),
        "{chip:?}"
    );
}

/// #1267: the queued chip counts the agents it waits for, and has no `u`.
#[test]
fn update_chip_queued_counts_agents() {
    let mut model = full_model();
    for (after, text) in [
        (3, "↑ queued · after 3 agents"),
        (1, "↑ queued · after 1 agent"),
        (0, "↑ queued · until idle"),
    ] {
        model.update = Some(UpdateChip::Queued { after });
        let row = render(&model, 150);
        assert!(row.contains(text), "{row:?}");
        assert!(!row.contains("0.34.0"), "{row:?}");
    }
}

/// #1507: one live running session and one saved record kept with a
/// Running status. Only the live one is counted as running.
#[test]
fn status_bar_does_not_count_kept_records_as_running() {
    use crate::session::types::Session;
    let mut app = crate::tui::make_test_app("maestro-kept-count-1507");
    app.tui_mode = crate::tui::app::TuiMode::Dashboard;
    let session = |issue| Session::new("p".into(), "opus".into(), "o".into(), Some(issue), None);
    let live = session(1);
    let live_id = live.id;
    app.pool.enqueue(live);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(live_id).expect("promoted");
    managed.session.status = SessionStatus::Running;
    let mut kept = session(2);
    kept.status = SessionStatus::Running;
    app.pool.keep_saved([&kept]);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| crate::tui::ui::draw(f, &mut app))
        .unwrap();
    let out = format!("{}", terminal.backend());
    let bar = out.lines().next().unwrap_or_default();
    assert!(bar.contains("1 run"), "the live session counts:\n{out}");
    assert!(
        !bar.contains("2 run"),
        "the kept record is not running:\n{out}"
    );
}

/// #1515: a CI fix waiting in the queue has no process yet. It counts as
/// waiting, not running.
#[test]
fn status_bar_counts_a_queued_fix_as_waiting_not_running() {
    use crate::session::types::Session;
    let mut app = crate::tui::make_test_app("maestro-queued-fix-1515");
    let mut queued = Session::new("p".into(), "opus".into(), "o".into(), Some(3), None);
    queued.status = SessionStatus::CiFix;
    app.pool.enqueue(queued);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| crate::tui::ui::draw(f, &mut app))
        .unwrap();
    let out = format!("{}", terminal.backend());
    let bar = out.lines().next().unwrap_or_default();
    assert!(bar.contains("0 run"), "the queued fix is not running:\n{out}");
    assert!(bar.contains("1 wait"), "the queued fix waits:\n{out}");
}

/// #1515: a saved record kept with a Running status has no process, so the
/// mascot does not conduct for it.
#[test]
fn mascot_ignores_kept_running_record() {
    use crate::mascot::MascotState;
    use crate::session::types::Session;
    let mut app = crate::tui::make_test_app("maestro-kept-mascot-1515");
    let mut kept = Session::new("p".into(), "opus".into(), "o".into(), Some(2), None);
    kept.status = SessionStatus::Running;
    app.pool.keep_saved([&kept]);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| crate::tui::ui::draw(f, &mut app))
        .unwrap();
    assert_eq!(app.mascot_animator.state(), MascotState::Idle);
}
