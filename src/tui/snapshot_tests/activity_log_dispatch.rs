//! Snapshot tests for the activity-log subagent-dispatch rendering.
//!
//! Pins that a `LogEntry` whose `ToolMeta::subagent_name` is `Some(...)` is
//! rendered as `Dispatching <name>` rather than the bare `Using <tool>` label,
//! while plain tool calls (`Read`, etc.) keep their existing rendering.
//!
//! Issue #542 introduced `subagent_name` on `ToolMeta` and the rendering
//! split. Issue #543 layers a role-colored chip span between the tool icon
//! and the message text whenever `role_for_subagent_name` resolves the name
//! to a known `Role`.
//!
//! Width is fixed at 80×8: large enough that no chip variant truncates the
//! timestamp or session label. If `TERM_WIDTH` ever shrinks, re-evaluate the
//! cell-budget assumption (chip body ≤ 3 cells in both modes).

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::session::role::Role;
use crate::tui::activity_log::{ActivityLog, LogEntry, LogLevel, ToolMeta};
use crate::tui::agent_graph::personalities::{ALL_ROLES, role_color};
use crate::tui::snapshot_tests::fixed_start;
use crate::tui::theme::Theme;

const TERM_WIDTH: u16 = 80;
const TERM_HEIGHT: u16 = 8;

fn dispatch_log(subagent: &str) -> ActivityLog {
    let mut log = ActivityLog::new(10);
    log.push(LogEntry {
        timestamp: fixed_start(),
        session_label: "S-abc123".to_string(),
        message: format!("Dispatching {}", subagent),
        level: LogLevel::Tool,
        tool_meta: Some(ToolMeta {
            tool_name: "Agent".to_string(),
            subagent_name: Some(subagent.to_string()),
        }),
    });
    log
}

fn plain_tool_log(tool_name: &str, message: &str) -> ActivityLog {
    let mut log = ActivityLog::new(10);
    log.push(LogEntry {
        timestamp: fixed_start(),
        session_label: "S-abc123".to_string(),
        message: message.to_string(),
        level: LogLevel::Tool,
        tool_meta: Some(ToolMeta {
            tool_name: tool_name.to_string(),
            subagent_name: None,
        }),
    });
    log
}

fn render_log_with_mode(log: &ActivityLog, use_nerd_font: bool) -> Terminal<TestBackend> {
    let theme = Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(TERM_WIDTH, TERM_HEIGHT)).unwrap();
    terminal
        .draw(|f| {
            log.draw(f, f.area(), &theme, use_nerd_font);
        })
        .unwrap();
    terminal
}

// --- existing #542 plain-tool snapshot, ported to the new signature ---------
// The #542 dispatch-label snapshot was folded into the chip Orchestrator
// nerd-font case below — same fixture, byte-identical render, more specific
// name in the chip-coverage matrix.

#[test]
fn activity_log_plain_tool_unchanged() {
    let log = plain_tool_log("Read", "Read: /src/main.rs");
    let terminal = render_log_with_mode(&log, true);
    assert_snapshot!(terminal.backend());
}

// --- Issue #543: role-colored chip ------------------------------------------
// 3 roles × 2 icon modes = 6 chip snapshots.

#[test]
fn activity_log_chip_orchestrator_nerd_font() {
    // Also covers the #542 "Dispatching <name>" label rendering.
    let log = dispatch_log("subagent-architect");
    let t = render_log_with_mode(&log, true);
    assert_snapshot!(t.backend());
}

#[test]
fn activity_log_chip_orchestrator_ascii() {
    let log = dispatch_log("subagent-architect");
    let t = render_log_with_mode(&log, false);
    assert_snapshot!(t.backend());
}

#[test]
fn activity_log_chip_reviewer_nerd_font() {
    let log = dispatch_log("subagent-qa");
    let t = render_log_with_mode(&log, true);
    assert_snapshot!(t.backend());
}

#[test]
fn activity_log_chip_reviewer_ascii() {
    let log = dispatch_log("subagent-qa");
    let t = render_log_with_mode(&log, false);
    assert_snapshot!(t.backend());
}

#[test]
fn activity_log_chip_docs_nerd_font() {
    let log = dispatch_log("subagent-docs-analyst");
    let t = render_log_with_mode(&log, true);
    assert_snapshot!(t.backend());
}

#[test]
fn activity_log_chip_docs_ascii() {
    let log = dispatch_log("subagent-docs-analyst");
    let t = render_log_with_mode(&log, false);
    assert_snapshot!(t.backend());
}

// --- unknown subagent: no chip (snapshot pins absence) ----------------------

#[test]
fn activity_log_unknown_subagent_renders_no_chip() {
    let log = dispatch_log("subagent-mystery");
    let t = render_log_with_mode(&log, true);
    assert_snapshot!(t.backend());
}

// --- non-subagent tool: no chip (buffer-color invariant) --------------------

#[test]
fn activity_log_plain_tool_has_no_chip() {
    let log = plain_tool_log("Read", "Read: /src/main.rs");
    let t = render_log_with_mode(&log, true);
    let buf = t.backend().buffer().clone();

    // Iterate `ALL_ROLES` so future Role additions are scanned automatically.
    // Safe because `LogLevel::Tool` uses the theme's `accent_info` (Cyan in
    // dark theme), which does not collide with any role color. Skip rows 0
    // and height-1 (the block's top/bottom borders); the dark theme's
    // `title_accent` is Yellow, identical to `role_color(Orchestrator)`, so
    // scanning the title row would self-fire.
    for y in 1..buf.area.height.saturating_sub(1) {
        for x in 0..buf.area.width {
            let cell_fg = buf[(x, y)].style().fg;
            for role in ALL_ROLES {
                assert_ne!(
                    cell_fg,
                    Some(role_color(role)),
                    "plain Read tool rendered a {role:?}-color chip at ({x},{y}): \
                     expected no chip for non-subagent tool calls"
                );
            }
        }
    }
}

// --- coverage gaps surfaced by QA -------------------------------------------

#[test]
fn activity_log_multiple_dispatches_all_get_chips() {
    // Composition: three dispatches in one log render three chips, each in
    // its role color. Buffer scan asserts all three role colors appear.
    let mut log = ActivityLog::new(10);
    for subagent in ["subagent-architect", "subagent-qa", "subagent-docs-analyst"] {
        log.push(LogEntry {
            timestamp: fixed_start(),
            session_label: "S-abc123".to_string(),
            message: format!("Dispatching {}", subagent),
            level: LogLevel::Tool,
            tool_meta: Some(ToolMeta {
                tool_name: "Agent".to_string(),
                subagent_name: Some(subagent.to_string()),
            }),
        });
    }
    let t = render_log_with_mode(&log, true);
    assert_snapshot!(t.backend());

    let buf = t.backend().buffer().clone();
    // Skip top/bottom borders: the dark theme's `title_accent` is Yellow,
    // identical to `role_color(Orchestrator)`, so the title row would
    // satisfy the Orchestrator scan vacuously.
    for expected in [
        role_color(Role::Orchestrator),
        role_color(Role::Reviewer),
        role_color(Role::Docs),
    ] {
        let found = (1..buf.area.height.saturating_sub(1))
            .any(|y| (0..buf.area.width).any(|x| buf[(x, y)].style().fg == Some(expected)));
        assert!(
            found,
            "missing role color {:?} in multi-dispatch render",
            expected
        );
    }
}

#[test]
fn activity_log_empty_subagent_name_renders_no_chip() {
    // Defensive: `role_for_subagent_name("")` returns None. The renderer must
    // treat `Some("")` the same as `None` (no chip, no panic).
    let mut log = ActivityLog::new(10);
    log.push(LogEntry {
        timestamp: fixed_start(),
        session_label: "S-abc123".to_string(),
        message: "Dispatching ".to_string(),
        level: LogLevel::Tool,
        tool_meta: Some(ToolMeta {
            tool_name: "Agent".to_string(),
            subagent_name: Some(String::new()),
        }),
    });
    let t = render_log_with_mode(&log, true);
    assert_snapshot!(t.backend());
}

#[test]
fn activity_log_agent_tool_without_subagent_name_renders_no_chip() {
    // Defensive: `tool_name = "Agent"` but `subagent_name = None` (parser
    // failed to extract the name). Inner `if let Some(name)` guard must
    // fall through to no-chip rendering.
    let mut log = ActivityLog::new(10);
    log.push(LogEntry {
        timestamp: fixed_start(),
        session_label: "S-abc123".to_string(),
        message: "Dispatching unknown-agent".to_string(),
        level: LogLevel::Tool,
        tool_meta: Some(ToolMeta {
            tool_name: "Agent".to_string(),
            subagent_name: None,
        }),
    });
    let t = render_log_with_mode(&log, true);
    assert_snapshot!(t.backend());
}
