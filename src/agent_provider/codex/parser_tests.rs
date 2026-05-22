use super::*;

#[test]
fn turn_completed_emits_cost_update_when_model_known() {
    // gpt-5-codex pricing: input=2.50, output=10.00, cache=0.25 USD per MTok.
    // 10 input + 5 output + 2 cache → 2.5*10 + 10*5 + 0.25*2 = 25 + 50 + 0.5 = 75.5
    // 75.5 / 1_000_000 = 0.0000755 USD.
    let mut parser = CodexStreamParser::with_model("gpt-5-codex");
    let events = parser.parse_line(
        r#"{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5,"cached_input_tokens":2}}"#,
    );

    let cost = events
        .iter()
        .find_map(|event| match event {
            StreamEvent::CostUpdate { cost_usd } => Some(*cost_usd),
            _ => None,
        })
        .expect("turn.completed with known model should emit CostUpdate");
    assert!(
        (cost - 0.000_075_5).abs() < 1e-12,
        "expected 0.0000755, got {cost}"
    );

    let completed_cost = events
        .iter()
        .find_map(|event| match event {
            StreamEvent::Completed { cost_usd } => Some(*cost_usd),
            _ => None,
        })
        .expect("turn.completed should emit Completed");
    assert!((completed_cost - cost).abs() < 1e-12);
}

#[test]
fn turn_completed_with_unknown_model_keeps_zero_cost() {
    // Default parser → model is empty string → unknown → cost stays 0.
    let mut parser = CodexStreamParser::default();
    let events = parser
        .parse_line(r#"{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}"#);

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, StreamEvent::CostUpdate { .. })),
        "unknown model should not emit CostUpdate"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        StreamEvent::Completed { cost_usd } if *cost_usd == 0.0
    )));
}

#[test]
fn turn_completed_with_zero_usage_emits_no_cost_update() {
    let mut parser = CodexStreamParser::with_model("gpt-5-codex");
    let events = parser
        .parse_line(r#"{"type":"turn.completed","usage":{"input_tokens":0,"output_tokens":0}}"#);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, StreamEvent::CostUpdate { .. })),
        "zero usage should not emit CostUpdate"
    );
}

#[test]
fn parser_maps_codex_jsonl_to_stream_events() {
    let mut parser = CodexStreamParser::default();
    let events: Vec<StreamEvent> = [
        r#"{"type":"thread.started","thread_id":"t1"}"#,
        r#"{"type":"item.completed","item":{"type":"function_call","call_id":"call_1","name":"shell","arguments":"{\"command\":\"cargo test\",\"path\":\"src/lib.rs\"}"}}"#,
        r#"{"type":"item.completed","item":{"type":"function_call_output","call_id":"call_1","output":"ok"}}"#,
        r#"{"type":"item.completed","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done."}]}}"#,
        r#"{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5,"cached_input_tokens":2}}"#,
    ]
    .into_iter()
    .flat_map(|line| parser.parse_line(line))
    .collect();

    assert!(events.iter().any(|event| {
        matches!(
            event,
            StreamEvent::ToolUse {
                tool,
                file_path: Some(path),
                command_preview: Some(command),
                ..
            } if tool == "shell" && path == "src/lib.rs" && command == "cargo test"
        )
    }));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, StreamEvent::ToolResult { tool, is_error: false } if tool == "shell"))
    );
    assert!(
        events.iter().any(
            |event| matches!(event, StreamEvent::AssistantMessage { text } if text == "Done.")
        )
    );
    assert!(events.iter().any(|event| {
        matches!(event, StreamEvent::TokenUpdate { usage } if usage.input_tokens == 10 && usage.output_tokens == 5 && usage.cache_read_tokens == 2)
    }));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, StreamEvent::Completed { .. }))
    );
}

#[test]
fn parser_maps_agent_message_and_turn_failed() {
    let mut parser = CodexStreamParser::default();

    let message = parser
        .parse_line(r#"{"type":"item.completed","item":{"type":"agent_message","text":"Hello"}}"#);
    assert!(matches!(
        message.as_slice(),
        [StreamEvent::AssistantMessage { text }] if text == "Hello"
    ));

    let failed = parser.parse_line(r#"{"type":"turn.failed","error":{"message":"model failed"}}"#);
    assert!(matches!(
        failed.as_slice(),
        [StreamEvent::Error { message }] if message == "model failed"
    ));
}

#[test]
fn parser_maps_current_codex_command_execution_events() {
    let mut parser = CodexStreamParser::default();

    let events = parser.parse_line(
        r#"{"type":"item.completed","item":{"aggregated_output":"ok\n","command":"/bin/zsh -lc 'bats tests/scripts/release.bats'","exit_code":1,"id":"item_35","status":"failed","type":"command_execution"}}"#,
    );

    assert!(matches!(
        events.as_slice(),
        [
            StreamEvent::ToolUse {
                tool,
                command_preview: Some(command),
                ..
            },
            StreamEvent::ToolResult {
                tool: result_tool,
                is_error: true
            }
        ] if tool == "Bash"
            && result_tool == "Bash"
            && command == "/bin/zsh -lc 'bats tests/scripts/release.bats'"
    ));
}

#[test]
fn parser_maps_current_codex_file_change_events() {
    let mut parser = CodexStreamParser::default();

    let events = parser.parse_line(
        r#"{"type":"item.completed","item":{"changes":[{"kind":"update","path":"/tmp/work/scripts/release.sh"},{"kind":"add","path":"/tmp/work/tests/scripts/release.bats"}],"id":"item_18","status":"completed","type":"file_change"}}"#,
    );

    assert!(events.iter().any(|event| {
        matches!(
            event,
            StreamEvent::ToolUse {
                tool,
                file_path: Some(path),
                ..
            } if tool == "Edit" && path == "/tmp/work/scripts/release.sh"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            StreamEvent::ToolUse {
                tool,
                file_path: Some(path),
                ..
            } if tool == "Write" && path == "/tmp/work/tests/scripts/release.bats"
        )
    }));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                StreamEvent::ToolResult {
                    is_error: false,
                    ..
                }
            ))
            .count(),
        2
    );
}

#[test]
fn parser_maps_current_codex_todo_updates() {
    let mut parser = CodexStreamParser::default();

    let events = parser.parse_line(
        r#"{"type":"item.updated","item":{"id":"item_3","type":"todo_list","items":[{"text":"Inspect release script","completed":true},{"text":"Run tests","completed":false}]}}"#,
    );

    assert!(matches!(
        events.as_slice(),
        [
            StreamEvent::ToolUse { tool, .. },
            StreamEvent::ToolResult {
                tool: result_tool,
                is_error: false
            }
        ] if tool == "TodoWrite" && result_tool == "TodoWrite"
    ));
}
