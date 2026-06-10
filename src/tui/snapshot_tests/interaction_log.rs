//! Snapshot test for the INTERACTION lifecycle activity-log sequence (#742).
//!
//! Drives `ActivityLog::emit_interaction` with the full transition sequence
//! of an `Interactive + produce_pr=true` session — launched → turn →
//! resumed → closing → teardown ok — and pins the rendered dashboard pane.
//! A second case pins the failure tail (turn failed → teardown failed).
//!
//! Entry timestamps come from `Utc::now()` inside `push_simple`, so the
//! rendered `HH:MM:SS` column is masked the same way the terminator
//! snapshots mask card-header times.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::activity_log::ActivityLog;
use crate::tui::theme::Theme;
use crate::work::activity::{CloseReasonSummary, InteractionActivity};
use std::path::PathBuf;

const TERM_WIDTH: u16 = 120;
const TERM_HEIGHT: u16 = 10;

fn render_log(log: &ActivityLog) -> Terminal<TestBackend> {
    let theme = Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(TERM_WIDTH, TERM_HEIGHT)).unwrap();
    terminal
        .draw(|f| {
            log.draw(f, f.area(), &theme, true);
        })
        .unwrap();
    terminal
}

/// Mask the wall-clock `HH:MM:SS` timestamp column.
fn with_time_mask(body: impl FnOnce()) {
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d{2}:\d{2}:\d{2}", "HH:MM:SS");
    settings.bind(body);
}

#[test]
fn interaction_lifecycle_log_sequence_full_session() {
    let mut log = ActivityLog::new(20);
    for activity in [
        InteractionActivity::Launched {
            issue: 42,
            produce_pr: true,
            transport: "interactive".into(),
        },
        InteractionActivity::TurnComplete {
            issue: 42,
            turn_index: 1,
            chunk_count: 12,
            duration_ms: 1430,
        },
        InteractionActivity::Resumed { issue: 42 },
        InteractionActivity::Closing {
            issue: 42,
            reason: CloseReasonSummary::PrCreated { pr_number: 7 },
        },
        InteractionActivity::TeardownOk {
            issue: 42,
            path: PathBuf::from("/tmp/maestro/issue-42"),
        },
    ] {
        log.emit_interaction(&activity);
    }

    let terminal = render_log(&log);
    let rendered = format!("{:?}", terminal.backend());
    for needle in [
        "#42 launched (mode: produce_pr=true, interaction=true, transport=interactive)",
        "#42 turn 1: 12 chunks streamed (1430 ms)",
        "#42 resumed",
        "#42 closing (reason: PrCreated #7); wiping worktree",
        "#42 worktree removed at /tmp/maestro/issue-42; branch deleted",
    ] {
        assert!(
            rendered.contains(needle),
            "missing {needle:?} in:\n{rendered}"
        );
    }
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}

#[test]
fn interaction_lifecycle_log_sequence_failure_tail() {
    let mut log = ActivityLog::new(20);
    for activity in [
        InteractionActivity::TurnFailed {
            issue: 42,
            detail: "agent exit 1".into(),
        },
        InteractionActivity::TeardownFail {
            issue: 42,
            path: PathBuf::from("/tmp/maestro/issue-42"),
            error: "path still exists".into(),
        },
    ] {
        log.emit_interaction(&activity);
    }

    let terminal = render_log(&log);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("#42 turn failed: agent exit 1"),
        "missing turn-failed line:\n{rendered}"
    );
    assert!(
        rendered.contains("teardown FAILED"),
        "missing teardown-failed line:\n{rendered}"
    );
    with_time_mask(|| assert_snapshot!(terminal.backend()));
}
