use super::types::{StreamEvent, TokenUsage};
use serde_json::Value;

/// Find a valid char boundary at or after `max_bytes` for safe string slicing.
const fn char_boundary(s: &str, max_bytes: usize) -> usize {
    if max_bytes >= s.len() {
        return s.len();
    }
    let mut i = max_bytes;
    while !s.is_char_boundary(i) && i > 0 {
        i -= 1;
    }
    i
}

/// Return the max input token limit for a given Claude model identifier.
fn model_max_input_tokens(model: &str) -> f64 {
    if model.contains("opus") {
        1_000_000.0
    } else {
        200_000.0
    }
}

/// Extract the subagent or skill name from a tool-use `input` payload.
///
/// `Agent` and `Task` carry the dispatched subagent in `input.subagent_type`;
/// `Skill` carries the invoked skill in `input.skill`. Any other tool — and any
/// missing or empty value — returns `None`. See issue #542.
///
/// The returned string is sanitized: control characters are stripped (so a
/// rogue `\n`/ANSI escape in the JSON cannot corrupt the activity-log render)
/// and the value is truncated to 80 chars at a UTF-8 boundary. Real subagent
/// names are well under that cap (`superpowers:brainstorming` is 25 chars).
fn extract_subagent_name(tool: &str, input: Option<&Value>) -> Option<String> {
    const MAX_NAME_LEN: usize = 80;
    let input = input?;
    let key = match tool {
        "Agent" | "Task" => "subagent_type",
        "Skill" => "skill",
        _ => return None,
    };
    let raw = input.get(key).and_then(|v| v.as_str())?;
    let sanitized: String = raw.chars().filter(|c| !c.is_control()).collect();
    if sanitized.is_empty() {
        return None;
    }
    if sanitized.len() <= MAX_NAME_LEN {
        Some(sanitized)
    } else {
        let boundary = char_boundary(&sanitized, MAX_NAME_LEN);
        Some(sanitized[..boundary].to_string())
    }
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

    let mut events = match v.get("type").and_then(|t| t.as_str()) {
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
    };

    // Extract context percentage from message.usage (present in assistant events).
    // The Claude CLI reports input_tokens (new) + cache_read_input_tokens (prior context)
    // but no max_input_tokens. We compute total and use model context limits.
    if let Some(msg) = v.get("message")
        && let Some(usage) = msg.get("usage")
    {
        let input = usage
            .get("input_tokens")
            .and_then(|t| t.as_f64())
            .unwrap_or(0.0);
        let cache_read = usage
            .get("cache_read_input_tokens")
            .and_then(|t| t.as_f64())
            .unwrap_or(0.0);
        let cache_create = usage
            .get("cache_creation_input_tokens")
            .and_then(|t| t.as_f64())
            .unwrap_or(0.0);
        let total_input = input + cache_read + cache_create;

        if total_input > 0.0 {
            let max = usage
                .get("max_input_tokens")
                .and_then(|t| t.as_f64())
                .unwrap_or_else(|| {
                    model_max_input_tokens(msg.get("model").and_then(|m| m.as_str()).unwrap_or(""))
                });
            if max > 0.0 {
                events.push(StreamEvent::ContextUpdate {
                    context_pct: total_input / max,
                });
            }
        }
    }

    events
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
            let subagent_name = extract_subagent_name(&tool, input);
            StreamEvent::ToolUse {
                tool,
                file_path,
                command_preview,
                subagent_name,
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
                        let input = block.get("input");
                        let file_path = input.and_then(|inp| {
                            inp.get("file_path")
                                .or_else(|| inp.get("path"))
                                .and_then(|p| p.as_str())
                                .map(|s| s.to_string())
                        });
                        let subagent_name = extract_subagent_name(&tool, input);
                        return StreamEvent::ToolUse {
                            tool,
                            file_path,
                            command_preview: None,
                            subagent_name,
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

    if let Some(usage) = v.get("usage") {
        if let Some(token_usage) = extract_token_usage(usage) {
            events.push(StreamEvent::TokenUpdate { usage: token_usage });
        }

        // Extract context percentage from input_tokens / max_input_tokens
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

    // --- Issue #197: Context percentage from result events ---

    #[test]
    fn parse_result_event_extracts_context_update() {
        let line = r#"{"type":"result","cost_usd":1.5,"usage":{"input_tokens":70000,"output_tokens":2000,"max_input_tokens":200000}}"#;
        let events = parse_stream_line(line);
        let ctx = events
            .iter()
            .find(|e| matches!(e, StreamEvent::ContextUpdate { .. }))
            .expect("Expected ContextUpdate from result event");
        match ctx {
            StreamEvent::ContextUpdate { context_pct } => {
                assert!((*context_pct - 0.35).abs() < 0.001);
            }
            other => panic!("Expected ContextUpdate, got {:?}", other),
        }
    }

    #[test]
    fn parse_result_event_no_max_tokens_no_context_update() {
        let line = r#"{"type":"result","cost_usd":1.0,"usage":{"input_tokens":50000,"output_tokens":1000}}"#;
        let events = parse_stream_line(line);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, StreamEvent::ContextUpdate { .. })),
            "Should NOT produce ContextUpdate when max_input_tokens is absent"
        );
    }

    #[test]
    fn parse_result_event_zero_max_tokens_no_context_update() {
        let line = r#"{"type":"result","cost_usd":1.0,"usage":{"input_tokens":50000,"output_tokens":1000,"max_input_tokens":0}}"#;
        let events = parse_stream_line(line);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, StreamEvent::ContextUpdate { .. })),
            "Should NOT produce ContextUpdate when max_input_tokens is 0"
        );
    }

    // --- Context percentage from assistant message.usage (cache tokens) ---

    #[test]
    fn parse_assistant_event_extracts_context_from_cache_tokens() {
        let line = r#"{"type":"assistant","message":{"type":"text","text":"hello","usage":{"input_tokens":3,"cache_read_input_tokens":50000,"cache_creation_input_tokens":10000,"output_tokens":25}}}"#;
        let events = parse_stream_line(line);
        let ctx = events
            .iter()
            .find(|e| matches!(e, StreamEvent::ContextUpdate { .. }))
            .expect("Expected ContextUpdate from assistant event with cache tokens");
        match ctx {
            StreamEvent::ContextUpdate { context_pct } => {
                // total = 3 + 50000 + 10000 = 60003, max = 200000 (default)
                assert!(
                    (*context_pct - 0.30).abs() < 0.01,
                    "expected ~30%, got {:.1}%",
                    context_pct * 100.0
                );
            }
            other => panic!("Expected ContextUpdate, got {:?}", other),
        }
    }

    #[test]
    fn parse_assistant_event_no_usage_no_context_update() {
        let line = r#"{"type":"assistant","message":{"type":"text","text":"hello"}}"#;
        let events = parse_stream_line(line);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, StreamEvent::ContextUpdate { .. })),
            "Should NOT produce ContextUpdate without usage"
        );
    }

    #[test]
    fn parse_assistant_event_opus_model_uses_1m_max() {
        let line = r#"{"type":"assistant","message":{"model":"claude-opus-4-6","type":"text","text":"hi","usage":{"input_tokens":3,"cache_read_input_tokens":50000,"cache_creation_input_tokens":10000,"output_tokens":25}}}"#;
        let events = parse_stream_line(line);
        let ctx = events
            .iter()
            .find(|e| matches!(e, StreamEvent::ContextUpdate { .. }))
            .expect("Expected ContextUpdate from opus assistant event");
        match ctx {
            StreamEvent::ContextUpdate { context_pct } => {
                // total = 60003, max = 1_000_000 (opus)
                assert!(
                    (*context_pct - 0.06).abs() < 0.01,
                    "expected ~6% for opus (1M ctx), got {:.1}%",
                    context_pct * 100.0
                );
            }
            other => panic!("Expected ContextUpdate, got {:?}", other),
        }
    }

    #[test]
    fn model_max_tokens_opus_is_1m() {
        assert_eq!(model_max_input_tokens("claude-opus-4-6"), 1_000_000.0);
        assert_eq!(model_max_input_tokens("claude-opus-4-6[1m]"), 1_000_000.0);
    }

    #[test]
    fn model_max_tokens_sonnet_is_200k() {
        assert_eq!(model_max_input_tokens("claude-sonnet-4-6"), 200_000.0);
    }

    #[test]
    fn model_max_tokens_unknown_defaults_200k() {
        assert_eq!(model_max_input_tokens("some-future-model"), 200_000.0);
        assert_eq!(model_max_input_tokens(""), 200_000.0);
    }

    // --- Issue #542: subagent_name extraction on dispatcher tools ---

    #[test]
    fn parse_tool_use_agent_with_subagent_type_extracts_name() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Agent","input":{"subagent_type":"subagent-architect"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse {
                tool,
                subagent_name,
                ..
            } => {
                assert_eq!(tool, "Agent");
                assert_eq!(subagent_name, Some("subagent-architect".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_task_with_subagent_type_extracts_name() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Task","input":{"subagent_type":"subagent-qa"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse {
                tool,
                subagent_name,
                ..
            } => {
                assert_eq!(tool, "Task");
                assert_eq!(subagent_name, Some("subagent-qa".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_skill_extracts_name() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Skill","input":{"skill":"superpowers:brainstorming"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse {
                tool,
                subagent_name,
                ..
            } => {
                assert_eq!(tool, "Skill");
                assert_eq!(subagent_name, Some("superpowers:brainstorming".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_agent_without_subagent_type_is_none() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Agent","input":{"prompt":"do something"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse { subagent_name, .. } => {
                assert_eq!(subagent_name, None);
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }

        // Empty-string guard: should also be treated as absent.
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Agent","input":{"subagent_type":""}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse { subagent_name, .. } => {
                assert_eq!(subagent_name, None);
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_read_has_no_subagent_name() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Read","input":{"file_path":"/src/main.rs"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse { subagent_name, .. } => {
                assert_eq!(subagent_name, None);
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_skill_without_skill_field_is_none() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Skill","input":{"args":"x"}}}"#;
        match first_event(line) {
            StreamEvent::ToolUse { subagent_name, .. } => {
                assert_eq!(subagent_name, None);
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_agent_subagent_type_strips_control_chars_and_caps_length() {
        // JSON escapes for newline (\n) and ANSI ESC (\u001b) — both control
        // chars that would otherwise corrupt the TUI activity-log render.
        let line = "{\"type\":\"assistant\",\"message\":{\"type\":\"tool_use\",\"name\":\"Agent\",\"input\":{\"subagent_type\":\"subagent-arch\\nitect\\u001b[31m\"}}}";
        match first_event(line) {
            StreamEvent::ToolUse { subagent_name, .. } => {
                let name = subagent_name.expect("control chars must not collapse to None");
                assert!(
                    !name.chars().any(char::is_control),
                    "control chars must be stripped, got {:?}",
                    name
                );
                assert!(
                    name.starts_with("subagent-architect"),
                    "stripping must preserve the visible name, got {:?}",
                    name
                );
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }

        // Length cap: input longer than 80 visible chars truncates at a
        // UTF-8 boundary.
        let oversized = "subagent-".to_string() + &"x".repeat(200);
        let line = format!(
            r#"{{"type":"assistant","message":{{"type":"tool_use","name":"Agent","input":{{"subagent_type":"{}"}}}}}}"#,
            oversized
        );
        match first_event(&line) {
            StreamEvent::ToolUse { subagent_name, .. } => {
                let name = subagent_name.expect("oversized name should still produce Some");
                assert!(
                    name.len() <= 80,
                    "name must be capped to 80, got {}",
                    name.len()
                );
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }
}
