//! Maps Claude Code session-transcript JSONL entries to [`StreamEvent`]s.
//!
//! The interactive (PTY) transport does not get machine-readable output on
//! stdout — the REPL renders for humans. Structure comes from the session
//! transcript at `~/.claude/projects/<munged-cwd>/<session-id>.jsonl`, which
//! Claude Code appends to as the turn progresses (spike #747,
//! `docs/spikes/2026-05-claude-interactive-transport.md`).
//!
//! Mapping rules:
//! - `assistant` entries embed the full API message; every content block is
//!   surfaced (`thinking` → `Thinking`, `text` → `AssistantMessage`,
//!   `tool_use` → `ToolUse`) and `message.usage` yields the same derived
//!   events as the headless path (`TokenUpdate`/`CostUpdate`/`ContextUpdate`).
//! - `system` entries with `subtype == "turn_duration"` mark the end of a
//!   turn → `Completed`. Cost is carried by the preceding `CostUpdate`s, so
//!   `cost_usd` here is `0.0` (same convention as the headless fallback
//!   `Completed` when no `result` line arrived).
//! - `user` entries surface `tool_result` blocks → `ToolResult`; plain text
//!   user entries are our own prompt echo and produce nothing.
//! - Benign metadata entries (`mode`, `permission-mode`, `ai-title`,
//!   `file-history-snapshot`, `last-prompt`, `attachment`, `pr-link`, …) and
//!   unrecognized-but-valid JSON produce nothing — the transcript schema
//!   grows with Claude Code releases and new metadata types must not spam
//!   the TUI as `Unknown`.
//! - Non-JSON lines → `Unknown` (genuine corruption worth surfacing).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use serde_json::Value;

use crate::session::parser::{content_block_event, usage_events};
use crate::session::types::StreamEvent;

/// Parse one transcript JSONL line into zero or more stream events.
pub(super) fn parse_transcript_line(line: &str) -> Vec<StreamEvent> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }

    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![StreamEvent::Unknown {
            raw: line.to_string(),
        }];
    };

    match v.get("type").and_then(|t| t.as_str()) {
        Some("assistant") => parse_assistant_entry(&v),
        Some("system") => parse_system_entry(&v),
        Some("user") => parse_user_entry(&v),
        _ => Vec::new(),
    }
}

fn parse_assistant_entry(v: &Value) -> Vec<StreamEvent> {
    let Some(msg) = v.get("message") else {
        return Vec::new();
    };

    let mut events: Vec<StreamEvent> = msg
        .get("content")
        .and_then(|c| c.as_array())
        .map(|blocks| blocks.iter().filter_map(content_block_event).collect())
        .unwrap_or_default();

    events.extend(usage_events(msg));
    events
}

fn parse_system_entry(v: &Value) -> Vec<StreamEvent> {
    if v.get("subtype").and_then(|s| s.as_str()) == Some("turn_duration") {
        vec![StreamEvent::Completed { cost_usd: 0.0 }]
    } else {
        Vec::new()
    }
}

fn parse_user_entry(v: &Value) -> Vec<StreamEvent> {
    let Some(blocks) = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    else {
        // Plain-string content: our own prompt echo.
        return Vec::new();
    };

    blocks
        .iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
        .map(|b| StreamEvent::ToolResult {
            // Transcript tool_result blocks carry only `tool_use_id`; the
            // tool name lives on the originating `tool_use` block, which the
            // TUI already rendered. "unknown" matches the headless parser's
            // fallback naming.
            tool: "unknown".to_string(),
            is_error: b.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::session::types::StreamEvent;

    const FIXTURE: &str = include_str!("../../../testdata/claude-transcript/3-turn.jsonl");

    fn fixture_line(n: usize) -> &'static str {
        FIXTURE.lines().nth(n).unwrap()
    }

    #[test]
    fn thinking_entry_maps_to_thinking_not_unknown() {
        // line 4 = assistant entry whose only content block is `thinking`.
        let events = parse_transcript_line(fixture_line(4));
        assert!(
            matches!(events.first(), Some(StreamEvent::Thinking { text }) if text == "simple arithmetic"),
            "expected Thinking first, got: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, StreamEvent::Unknown { .. })),
            "thinking entry must not degrade to Unknown: {events:?}"
        );
    }

    #[test]
    fn text_entry_maps_to_assistant_message_with_usage() {
        let events = parse_transcript_line(fixture_line(5));
        assert!(
            matches!(events.first(), Some(StreamEvent::AssistantMessage { text }) if text == "4"),
            "expected AssistantMessage, got: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::TokenUpdate { .. })),
            "usage must yield TokenUpdate: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ContextUpdate { .. })),
            "usage must yield ContextUpdate: {events:?}"
        );
    }

    #[test]
    fn tool_use_entry_maps_to_tool_use_with_file_path() {
        let events = parse_transcript_line(fixture_line(9));
        match events.first() {
            Some(StreamEvent::ToolUse {
                tool, file_path, ..
            }) => {
                assert_eq!(tool, "Read");
                assert_eq!(file_path.as_deref(), Some("/tmp/proj/notes.txt"));
            }
            other => panic!("expected ToolUse, got: {other:?}"),
        }
    }

    #[test]
    fn turn_duration_marks_completion() {
        let events = parse_transcript_line(fixture_line(6));
        assert!(
            matches!(events.as_slice(), [StreamEvent::Completed { cost_usd }] if *cost_usd == 0.0),
            "turn_duration must map to a single Completed: {events:?}"
        );
    }

    #[test]
    fn user_tool_result_block_maps_to_tool_result() {
        let events = parse_transcript_line(fixture_line(10));
        assert!(
            matches!(
                events.as_slice(),
                [StreamEvent::ToolResult {
                    is_error: false,
                    ..
                }]
            ),
            "expected one ToolResult, got: {events:?}"
        );
    }

    #[test]
    fn prompt_echo_and_metadata_entries_produce_nothing() {
        // user prompt echo (plain string content)
        assert!(parse_transcript_line(fixture_line(3)).is_empty());
        // mode / permission-mode / file-history-snapshot / ai-title / last-prompt
        for n in [0, 1, 2, 7, 16] {
            assert!(
                parse_transcript_line(fixture_line(n)).is_empty(),
                "metadata line {n} must produce no events"
            );
        }
    }

    #[test]
    fn unknown_future_metadata_type_is_silently_skipped() {
        let events = parse_transcript_line(r#"{"type":"shiny-new-thing","payload":42}"#);
        assert!(events.is_empty(), "got: {events:?}");
    }

    #[test]
    fn non_json_line_surfaces_as_unknown() {
        let events = parse_transcript_line("garbage not json");
        assert!(
            matches!(events.as_slice(), [StreamEvent::Unknown { raw }] if raw == "garbage not json"),
            "got: {events:?}"
        );
    }

    #[test]
    fn empty_line_produces_nothing() {
        assert!(parse_transcript_line("").is_empty());
        assert!(parse_transcript_line("   ").is_empty());
    }

    #[test]
    fn three_turn_fixture_event_sequence_snapshot() {
        let events = FIXTURE
            .lines()
            .flat_map(parse_transcript_line)
            .map(|e| format!("{e:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        insta::assert_snapshot!("transcript_three_turn_events", events);
    }
}
