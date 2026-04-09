use super::types::{StreamEvent, TokenUsage};
use serde_json::Value;

/// Find a valid char boundary at or after `max_bytes` for safe string slicing.
fn char_boundary(s: &str, max_bytes: usize) -> usize {
    if max_bytes >= s.len() {
        return s.len();
    }
    let mut i = max_bytes;
    while !s.is_char_boundary(i) && i > 0 {
        i -= 1;
    }
    i
}

/// Parse a single line of Claude CLI `--output-format stream-json` output.
///
/// The stream-json format emits one JSON object per line. Key event types:
/// - `{"type":"assistant","message":{"type":"text","text":"..."}}`
/// - `{"type":"assistant","message":{"type":"tool_use","name":"...","input":{...}}}`
/// - `{"type":"tool_result","tool_use_id":"...","content":"...","is_error":false}`
/// - `{"type":"result","cost_usd":1.23,"duration_ms":...,"session_id":"..."}`
/// - `{"type":"error","error":{"message":"..."}}`
pub fn parse_stream_line(line: &str) -> Vec<StreamEvent> {
    let line = line.trim();
    if line.is_empty() {
        return vec![StreamEvent::Unknown { raw: String::new() }];
    }

    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![StreamEvent::Unknown {
            raw: line.to_string(),
        }];
    };

    match v.get("type").and_then(|t| t.as_str()) {
        Some("assistant") => vec![parse_assistant_event(&v)],
        Some("tool_result") => vec![parse_tool_result(&v)],
        Some("result") => parse_result(&v),
        Some("system") => parse_system_event(&v),
        Some("error") => {
            let msg = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            vec![StreamEvent::Error { message: msg }]
        }
        _ => vec![StreamEvent::Unknown {
            raw: line.to_string(),
        }],
    }
}

fn parse_assistant_event(v: &Value) -> StreamEvent {
    let msg = v.get("message").or_else(|| v.get("content_block"));

    let msg_type = msg
        .and_then(|m| m.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    match msg_type {
        "thinking" => {
            let text = msg
                .and_then(|m| m.get("thinking"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            StreamEvent::Thinking { text }
        }
        "tool_use" => {
            let tool = msg
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let input = msg.and_then(|m| m.get("input"));
            let file_path = input.and_then(|inp| {
                inp.get("file_path")
                    .or_else(|| inp.get("path"))
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
            });
            let command_preview = input.and_then(|inp| {
                inp.get("command").and_then(|c| c.as_str()).map(|s| {
                    if s.len() > 60 {
                        let boundary = char_boundary(s, 60);
                        format!("{}…", &s[..boundary])
                    } else {
                        s.to_string()
                    }
                })
            });
            StreamEvent::ToolUse {
                tool,
                file_path,
                command_preview,
            }
        }
        "text" => {
            let text = msg
                .and_then(|m| m.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            StreamEvent::AssistantMessage { text }
        }
        _ => {
            // Try to extract text from content array
            if let Some(content) = msg
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                for block in content {
                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                        let text = block
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !text.is_empty() {
                            return StreamEvent::AssistantMessage { text };
                        }
                    }
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        let tool = block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let file_path = block.get("input").and_then(|inp| {
                            inp.get("file_path")
                                .or_else(|| inp.get("path"))
                                .and_then(|p| p.as_str())
                                .map(|s| s.to_string())
                        });
                        return StreamEvent::ToolUse {
                            tool,
                            file_path,
                            command_preview: None,
                        };
                    }
                }
            }
            StreamEvent::Unknown { raw: v.to_string() }
        }
    }
}

fn parse_tool_result(v: &Value) -> StreamEvent {
    let is_error = v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false);
    let tool = v
        .get("tool_name")
        .or_else(|| v.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string();
    StreamEvent::ToolResult { tool, is_error }
}

/// Extract TokenUsage from a `usage` JSON object if token fields are present.
fn extract_token_usage(usage: &Value) -> Option<TokenUsage> {
    let input = usage
        .get("input_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);

    if input > 0 || output > 0 || cache_read > 0 || cache_creation > 0 {
        Some(TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
        })
    } else {
        None
    }
}

fn parse_system_event(v: &Value) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    if let Some(usage) = v.get("usage") {
        if let Some(token_usage) = extract_token_usage(usage) {
            events.push(StreamEvent::TokenUpdate { usage: token_usage });
        }

        // Context percentage from input/max
        if let (Some(input_f), Some(max)) = (
            usage.get("input_tokens").and_then(|t| t.as_f64()),
            usage.get("max_input_tokens").and_then(|t| t.as_f64()),
        ) && max > 0.0
        {
            events.push(StreamEvent::ContextUpdate {
                context_pct: input_f / max,
            });
        }
    }

    // Context percentage from top-level context_pct
    if let Some(pct) = v.get("context_pct").and_then(|p| p.as_f64()) {
        events.push(StreamEvent::ContextUpdate {
            context_pct: pct / 100.0,
        });
    }

    if events.is_empty() {
        events.push(StreamEvent::Unknown { raw: v.to_string() });
    }

    events
}

fn parse_result(v: &Value) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    let cost = v
        .get("cost_usd")
        .and_then(|c| c.as_f64())
        .or_else(|| {
            v.get("usage")
                .and_then(|u| u.get("cost_usd"))
                .and_then(|c| c.as_f64())
        })
        .unwrap_or(0.0);

    if let Some(usage) = v.get("usage")
        && let Some(token_usage) = extract_token_usage(usage)
    {
        events.push(StreamEvent::TokenUpdate { usage: token_usage });
    }

    events.push(StreamEvent::Completed { cost_usd: cost });
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: get the first event of a specific type from parsed events.
    fn first_event(line: &str) -> StreamEvent {
        parse_stream_line(line).into_iter().next().unwrap()
    }

    /// Helper: find a specific event type from parsed events.
    fn find_event<F: Fn(&StreamEvent) -> bool>(line: &str, pred: F) -> StreamEvent {
        parse_stream_line(line)
            .into_iter()
            .find(|e| pred(e))
            .unwrap_or_else(|| panic!("No matching event found in: {:?}", parse_stream_line(line)))
    }

    #[test]
    fn parse_text_message() {
        let line = r#"{"type":"assistant","message":{"type":"text","text":"Hello world"}}"#;
        match first_event(line) {
            StreamEvent::AssistantMessage { text } => assert_eq!(text, "Hello world"),
            other => panic!("Expected AssistantMessage, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Read","input":{"path":"/foo"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse { tool, .. } => assert_eq!(tool, "Read"),
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_result_event() {
        let line = r#"{"type":"result","cost_usd":1.5,"duration_ms":30000}"#;
        let event = find_event(line, |e| matches!(e, StreamEvent::Completed { .. }));
        match event {
            StreamEvent::Completed { cost_usd } => assert!((cost_usd - 1.5).abs() < f64::EPSILON),
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn parse_error() {
        let line = r#"{"type":"error","error":{"message":"rate limited"}}"#;
        match first_event(line) {
            StreamEvent::Error { message } => assert_eq!(message, "rate limited"),
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn parse_garbage() {
        let events = parse_stream_line("not json at all");
        assert!(matches!(events[0], StreamEvent::Unknown { .. }));
    }

    #[test]
    fn parse_tool_use_read_extracts_file_path() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Read","input":{"file_path":"/src/main.rs"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse {
                tool, file_path, ..
            } => {
                assert_eq!(tool, "Read");
                assert_eq!(file_path, Some("/src/main.rs".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_write_extracts_file_path() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Write","input":{"file_path":"/src/new.rs","content":"fn main() {}"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse {
                tool, file_path, ..
            } => {
                assert_eq!(tool, "Write");
                assert_eq!(file_path, Some("/src/new.rs".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_edit_extracts_file_path() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Edit","input":{"file_path":"/src/lib.rs","old_string":"foo","new_string":"bar"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse { file_path, .. } => {
                assert_eq!(file_path, Some("/src/lib.rs".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_bash_has_no_file_path() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse {
                tool, file_path, ..
            } => {
                assert_eq!(tool, "Bash");
                assert_eq!(file_path, None);
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_with_path_key_fallback() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Read","input":{"path":"/foo"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse {
                tool, file_path, ..
            } => {
                assert_eq!(tool, "Read");
                assert_eq!(file_path, Some("/foo".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_no_input_has_no_file_path() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Read"}}"#;
        match first_event(line) {
            StreamEvent::ToolUse { file_path, .. } => {
                assert_eq!(file_path, None);
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_system_event_context_update_from_context_pct() {
        let line = r#"{"type":"system","context_pct":68.4}"#;
        let event = find_event(line, |e| matches!(e, StreamEvent::ContextUpdate { .. }));
        match event {
            StreamEvent::ContextUpdate { context_pct } => {
                assert!((context_pct - 0.684).abs() < 0.001);
            }
            other => panic!("Expected ContextUpdate, got {:?}", other),
        }
    }

    #[test]
    fn parse_system_event_context_update_from_usage_tokens() {
        let line = r#"{"type":"system","usage":{"input_tokens":70000,"max_input_tokens":100000}}"#;
        let event = find_event(line, |e| matches!(e, StreamEvent::ContextUpdate { .. }));
        match event {
            StreamEvent::ContextUpdate { context_pct } => {
                assert!((context_pct - 0.7).abs() < f64::EPSILON);
            }
            other => panic!("Expected ContextUpdate, got {:?}", other),
        }
    }

    // --- Issue #102: Phase 1 — command_preview extraction ---

    #[test]
    fn parse_bash_tool_use_extracts_command_preview() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse {
                tool,
                command_preview,
                ..
            } => {
                assert_eq!(tool, "Bash");
                assert_eq!(command_preview, Some("cargo test".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_non_bash_tool_use_has_no_command_preview() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Read","input":{"file_path":"/src/lib.rs"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse {
                command_preview, ..
            } => {
                assert_eq!(command_preview, None);
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_bash_long_command_truncated_to_60_chars() {
        let long_cmd = "a".repeat(80);
        let line = format!(
            r#"{{"type":"assistant","message":{{"type":"tool_use","name":"Bash","input":{{"command":"{}"}}}}}}"#,
            long_cmd
        );
        match first_event(&line) {
            StreamEvent::ToolUse {
                command_preview: Some(p),
                ..
            } => {
                assert!(
                    p.ends_with('…'),
                    "truncated command must end with ellipsis, got: {:?}",
                    p
                );
                let without_ellipsis = p.trim_end_matches('…');
                assert!(
                    without_ellipsis.chars().count() <= 60,
                    "command prefix exceeds 60 chars: {:?}",
                    without_ellipsis
                );
            }
            other => panic!(
                "Expected ToolUse with Some(command_preview), got {:?}",
                other
            ),
        }
    }

    // --- Issue #102: Phase 2 — Thinking block extraction ---

    #[test]
    fn parse_thinking_message_produces_thinking_event() {
        let line =
            r#"{"type":"assistant","message":{"type":"thinking","thinking":"Let me reason..."}}"#;
        match first_event(line) {
            StreamEvent::Thinking { text } => assert_eq!(text, "Let me reason..."),
            other => panic!("Expected Thinking, got {:?}", other),
        }
    }

    #[test]
    fn parse_thinking_message_empty_text_produces_thinking_with_empty_string() {
        let line = r#"{"type":"assistant","message":{"type":"thinking","thinking":""}}"#;
        match first_event(line) {
            StreamEvent::Thinking { text } => assert!(text.is_empty()),
            other => panic!("Expected Thinking with empty text, got {:?}", other),
        }
    }

    #[test]
    fn parse_system_event_unknown_subtype_falls_through() {
        let line = r#"{"type":"system","event":"something_new"}"#;
        assert!(matches!(first_event(line), StreamEvent::Unknown { .. }));
    }

    // --- Issue #161: Token usage extraction ---

    #[test]
    fn parse_system_event_extracts_token_usage() {
        let line = r#"{"type":"system","usage":{"input_tokens":70000,"output_tokens":1200,"cache_read_input_tokens":45000,"cache_creation_input_tokens":5000,"max_input_tokens":200000}}"#;
        let event = find_event(line, |e| matches!(e, StreamEvent::TokenUpdate { .. }));
        match event {
            StreamEvent::TokenUpdate { usage } => {
                assert_eq!(usage.input_tokens, 70000);
                assert_eq!(usage.output_tokens, 1200);
                assert_eq!(usage.cache_read_tokens, 45000);
                assert_eq!(usage.cache_creation_tokens, 5000);
            }
            other => panic!("Expected TokenUpdate, got {:?}", other),
        }
    }

    #[test]
    fn parse_system_event_emits_both_token_and_context_updates() {
        let line = r#"{"type":"system","usage":{"input_tokens":70000,"output_tokens":1200,"max_input_tokens":200000}}"#;
        let events = parse_stream_line(line);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::TokenUpdate { .. }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ContextUpdate { .. }))
        );
    }

    #[test]
    fn parse_result_event_extracts_token_usage() {
        let line = r#"{"type":"result","cost_usd":1.5,"usage":{"input_tokens":50000,"output_tokens":2000,"cache_read_input_tokens":30000,"cache_creation_input_tokens":1000}}"#;
        let events = parse_stream_line(line);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::TokenUpdate { .. }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::Completed { .. }))
        );
    }

    #[test]
    fn parse_system_event_no_tokens_no_token_update() {
        let line = r#"{"type":"system","context_pct":50.0}"#;
        let events = parse_stream_line(line);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, StreamEvent::TokenUpdate { .. }))
        );
    }
}
