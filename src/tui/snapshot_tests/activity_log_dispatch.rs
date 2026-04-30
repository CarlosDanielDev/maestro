//! Snapshot tests for the activity-log subagent-dispatch rendering.
//!
//! Pins that a `LogEntry` whose `ToolMeta::subagent_name` is `Some(...)` is
//! rendered as `Dispatching <name>` rather than the bare `Using <tool>` label,
//! while plain tool calls (`Read`, etc.) keep their existing rendering.
//!
//! Issue #542. Follows the `snapshot_tests/agent_personalities.rs` precedent
//! of constructing display values directly and rendering via `TestBackend`.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::activity_log::{ActivityLog, LogEntry, LogLevel, ToolMeta};
use crate::tui::snapshot_tests::fixed_start;
use crate::tui::theme::Theme;

const TERM_WIDTH: u16 = 80;
const TERM_HEIGHT: u16 = 8;

fn render_log(log: &ActivityLog) -> Terminal<TestBackend> {
    let theme = Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(TERM_WIDTH, TERM_HEIGHT)).unwrap();
    terminal
        .draw(|f| {
            log.draw(f, f.area(), &theme);
        })
        .unwrap();
    terminal
}

#[test]
fn activity_log_dispatch_renders_dispatching_label() {
    let mut log = ActivityLog::new(10);
    log.push(LogEntry {
        timestamp: fixed_start(),
        session_label: "S-abc123".to_string(),
        message: "Dispatching subagent-architect".to_string(),
        level: LogLevel::Tool,
        tool_meta: Some(ToolMeta {
            tool_name: "Agent".to_string(),
            subagent_name: Some("subagent-architect".to_string()),
        }),
    });

    let terminal = render_log(&log);
    assert_snapshot!(terminal.backend());
}

#[test]
fn activity_log_plain_tool_unchanged() {
    let mut log = ActivityLog::new(10);
    log.push(LogEntry {
        timestamp: fixed_start(),
        session_label: "S-abc123".to_string(),
        message: "Read: /src/main.rs".to_string(),
        level: LogLevel::Tool,
        tool_meta: Some(ToolMeta {
            tool_name: "Read".to_string(),
            subagent_name: None,
        }),
    });

    let terminal = render_log(&log);
    assert_snapshot!(terminal.backend());
}
