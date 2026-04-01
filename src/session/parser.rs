use super::types::StreamEvent;
use serde_json::Value;

/// Parse a single line of Claude CLI `--output-format stream-json` output.
///
/// The stream-json format emits one JSON object per line. Key event types:
/// - `{"type":"assistant","message":{"type":"text","text":"..."}}`
/// - `{"type":"assistant","message":{"type":"tool_use","name":"...","input":{...}}}`
/// - `{"type":"tool_result","tool_use_id":"...","content":"...","is_error":false}`
/// - `{"type":"result","cost_usd":1.23,"duration_ms":...,"session_id":"..."}`
/// - `{"type":"error","error":{"message":"..."}}`
pub fn parse_stream_line(line: &str) -> StreamEvent {
    let line = line.trim();
    if line.is_empty() {
        return StreamEvent::Unknown { raw: String::new() };
    }

    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return StreamEvent::Unknown {
            raw: line.to_string(),
        };
    };

    match v.get("type").and_then(|t| t.as_str()) {
        Some("assistant") => parse_assistant_event(&v),
        Some("tool_result") => parse_tool_result(&v),
        Some("result") => parse_result(&v),
        Some("system") => parse_system_event(&v),
        Some("error") => {
            let msg = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            StreamEvent::Error { message: msg }
        }
        _ => StreamEvent::Unknown {
            raw: line.to_string(),
        },
    }
}

fn parse_assistant_event(v: &Value) -> StreamEvent {
    let msg = v.get("message").or_else(|| v.get("content_block"));

    let msg_type = msg
        .and_then(|m| m.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    match msg_type {
        "tool_use" => {
            let tool = msg
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let input = msg.and_then(|m| m.get("input"));
            let args_preview = match input {
                Some(inp) => {
                    let s = inp.to_string();
                    if s.len() > 80 {
                        format!("{}…", &s[..80])
                    } else {
                        s
                    }
                }
                None => String::new(),
            };
            let file_path = input.and_then(|inp| {
                inp.get("file_path")
                    .or_else(|| inp.get("path"))
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
            });
            StreamEvent::ToolUse {
                tool,
                args_preview,
                file_path,
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
                            args_preview: String::new(),
                            file_path,
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

fn parse_system_event(v: &Value) -> StreamEvent {
    // Check for context usage percentage in system events
    if let Some(pct) = v.get("context_pct").and_then(|p| p.as_f64()) {
        return StreamEvent::ContextUpdate {
            context_pct: pct / 100.0,
        };
    }
    // Check usage sub-object for input_tokens and max context
    if let Some(usage) = v.get("usage")
        && let (Some(input), Some(max)) = (
            usage.get("input_tokens").and_then(|t| t.as_f64()),
            usage.get("max_input_tokens").and_then(|t| t.as_f64()),
        )
        && max > 0.0
    {
        return StreamEvent::ContextUpdate {
            context_pct: input / max,
        };
    }
    StreamEvent::Unknown { raw: v.to_string() }
}

fn parse_result(v: &Value) -> StreamEvent {
    let cost = v
        .get("cost_usd")
        .and_then(|c| c.as_f64())
        .or_else(|| {
            v.get("usage")
                .and_then(|u| u.get("cost_usd"))
                .and_then(|c| c.as_f64())
        })
        .unwrap_or(0.0);
    StreamEvent::Completed { cost_usd: cost }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_message() {
        let line = r#"{"type":"assistant","message":{"type":"text","text":"Hello world"}}"#;
        match parse_stream_line(line) {
            StreamEvent::AssistantMessage { text } => assert_eq!(text, "Hello world"),
            other => panic!("Expected AssistantMessage, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Read","input":{"path":"/foo"}}}"#;
        match parse_stream_line(line) {
            StreamEvent::ToolUse { tool, .. } => assert_eq!(tool, "Read"),
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_result_event() {
        let line = r#"{"type":"result","cost_usd":1.5,"duration_ms":30000}"#;
        match parse_stream_line(line) {
            StreamEvent::Completed { cost_usd } => assert!((cost_usd - 1.5).abs() < f64::EPSILON),
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn parse_error() {
        let line = r#"{"type":"error","error":{"message":"rate limited"}}"#;
        match parse_stream_line(line) {
            StreamEvent::Error { message } => assert_eq!(message, "rate limited"),
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn parse_garbage() {
        let event = parse_stream_line("not json at all");
        assert!(matches!(event, StreamEvent::Unknown { .. }));
    }

    #[test]
    fn parse_tool_use_read_extracts_file_path() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Read","input":{"file_path":"/src/main.rs"}}}"#;
        match parse_stream_line(line) {
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
        match parse_stream_line(line) {
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
        match parse_stream_line(line) {
            StreamEvent::ToolUse { file_path, .. } => {
                assert_eq!(file_path, Some("/src/lib.rs".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_use_bash_has_no_file_path() {
        let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}}"#;
        match parse_stream_line(line) {
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
        match parse_stream_line(line) {
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
        match parse_stream_line(line) {
            StreamEvent::ToolUse { file_path, .. } => {
                assert_eq!(file_path, None);
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn parse_system_event_context_update_from_context_pct() {
        let line = r#"{"type":"system","context_pct":68.4}"#;
        match parse_stream_line(line) {
            StreamEvent::ContextUpdate { context_pct } => {
                assert!((context_pct - 0.684).abs() < 0.001);
            }
            other => panic!("Expected ContextUpdate, got {:?}", other),
        }
    }

    #[test]
    fn parse_system_event_context_update_from_usage_tokens() {
        let line = r#"{"type":"system","usage":{"input_tokens":70000,"max_input_tokens":100000}}"#;
        match parse_stream_line(line) {
            StreamEvent::ContextUpdate { context_pct } => {
                assert!((context_pct - 0.7).abs() < f64::EPSILON);
            }
            other => panic!("Expected ContextUpdate, got {:?}", other),
        }
    }

    #[test]
    fn parse_system_event_unknown_subtype_falls_through() {
        let line = r#"{"type":"system","event":"something_new"}"#;
        assert!(matches!(
            parse_stream_line(line),
            StreamEvent::Unknown { .. }
        ));
    }
}
