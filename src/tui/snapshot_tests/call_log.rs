//! Snapshot tests for the per-agent call-log viewer (#868).

use super::*;
use crate::session::types::{CallLogEntry, CallLogKind, SessionStatus};
use crate::tui::call_log::draw::draw_call_log;
use crate::tui::call_log::state::CallLogState;
use crate::tui::theme::Theme;
use chrono::{TimeZone, Utc};
use insta::assert_snapshot;

fn entry(kind: CallLogKind, sec: u32, payload: &str) -> CallLogEntry {
    CallLogEntry {
        timestamp: Utc.with_ymd_and_hms(2026, 5, 23, 10, 42, sec).unwrap(),
        kind,
        payload_json: payload.to_string(),
    }
}

fn session_with_log(entries: Vec<CallLogEntry>) -> Session {
    let mut s = make_session(SessionStatus::Running, Some(868));
    s.call_log = entries;
    s
}

#[test]
fn call_log_empty_pane() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let session = session_with_log(vec![]);
    let state = CallLogState::default();
    terminal
        .draw(|f| draw_call_log(f, &session, &state, f.area(), &theme))
        .unwrap();
    let output = format!("{}", terminal.backend());
    assert!(
        output.contains("No events"),
        "empty pane must mention 'No events'; got:\n{output}"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn call_log_with_three_events() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let session = session_with_log(vec![
        entry(
            CallLogKind::AssistantMessage,
            0,
            r#"{"text":"Analyzing codebase..."}"#,
        ),
        entry(
            CallLogKind::ToolUse,
            1,
            r#"{"tool":"Read","file_path":"src/lib.rs"}"#,
        ),
        entry(
            CallLogKind::ToolResult,
            2,
            r#"{"tool":"Read","is_error":false}"#,
        ),
    ]);
    let state = CallLogState::default();
    terminal
        .draw(|f| draw_call_log(f, &session, &state, f.area(), &theme))
        .unwrap();
    assert_snapshot!(terminal.backend());
}

#[test]
fn call_log_with_hook_response() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let session = session_with_log(vec![
        entry(
            CallLogKind::ToolUse,
            0,
            r#"{"type":"ToolUse","tool":"Bash","command_preview":"cargo test"}"#,
        ),
        entry(
            CallLogKind::HookResponse,
            1,
            r#"{"type":"HookResponse","hook_name":"pre-commit","exit_code":1,"stdout":"","stderr":"hook failed: lint errors found"}"#,
        ),
    ]);
    let state = CallLogState {
        selected: 1,
        ..Default::default()
    };
    terminal
        .draw(|f| draw_call_log(f, &session, &state, f.area(), &theme))
        .unwrap();
    assert_snapshot!(terminal.backend());
}

#[test]
fn call_log_follow_tail_on() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let entries: Vec<CallLogEntry> = (0u32..10)
        .map(|i| entry(CallLogKind::AssistantMessage, i % 60, r#"{"text":"chunk"}"#))
        .collect();
    let session = session_with_log(entries);
    let mut state = CallLogState {
        follow_tail: true,
        ..Default::default()
    };
    // Live-tail reconcile snaps the cursor to the newest (last) entry, and the
    // footer reflects "Follow: ON".
    state.reconcile_follow_tail(10);
    assert_eq!(state.selected, 9);
    terminal
        .draw(|f| draw_call_log(f, &session, &state, f.area(), &theme))
        .unwrap();
    assert_snapshot!(terminal.backend());
}

#[test]
fn call_log_with_many_events_selection_mid() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let entries: Vec<CallLogEntry> = (0u32..50)
        .map(|i| {
            let kind = if i % 3 == 0 {
                CallLogKind::ToolUse
            } else {
                CallLogKind::AssistantMessage
            };
            entry(kind, i % 60, r#"{"text":"chunk"}"#)
        })
        .collect();
    let session = session_with_log(entries);
    let state = CallLogState {
        selected: 25,
        list_scroll: 20,
        ..Default::default()
    };
    terminal
        .draw(|f| draw_call_log(f, &session, &state, f.area(), &theme))
        .unwrap();
    assert_snapshot!(terminal.backend());
}

#[test]
fn call_log_expanded_thinking() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let thinking_payload =
        r#"{"text":"Step 1: read the file. Step 2: understand context. Step 3: write the fix."}"#;
    let session = session_with_log(vec![
        entry(CallLogKind::AssistantMessage, 0, r#"{"text":"start"}"#),
        entry(CallLogKind::Thinking, 1, thinking_payload),
        entry(CallLogKind::ToolUse, 2, r#"{"tool":"Write"}"#),
    ]);
    let state = CallLogState {
        selected: 1,
        expanded: true,
        ..Default::default()
    };
    terminal
        .draw(|f| draw_call_log(f, &session, &state, f.area(), &theme))
        .unwrap();
    assert_snapshot!(terminal.backend());
}

#[test]
fn call_log_with_error_event_uses_accent_color() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let session = session_with_log(vec![
        entry(CallLogKind::AssistantMessage, 0, r#"{"text":"working..."}"#),
        entry(
            CallLogKind::ToolUse,
            1,
            r#"{"tool":"Bash","command_preview":"cargo test"}"#,
        ),
        entry(
            CallLogKind::Error,
            2,
            r#"{"message":"context window exceeded"}"#,
        ),
    ]);
    // Select a non-error row so the selection style does NOT mask the
    // Error event's accent_error kind color.
    let state = CallLogState {
        selected: 0,
        ..Default::default()
    };
    terminal
        .draw(|f| draw_call_log(f, &session, &state, f.area(), &theme))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let error_color = theme.accent_error;
    let has_error_color = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .any(|(x, y)| buf[(x, y)].style().fg == Some(error_color));
    assert!(
        has_error_color,
        "error event must render with accent_error color somewhere"
    );
    assert_snapshot!(terminal.backend());
}
