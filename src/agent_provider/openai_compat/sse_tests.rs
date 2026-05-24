use super::*;

fn parse_all(input: &str) -> Vec<StreamEvent> {
    let mut parser = OpenAiCompatibleSseParser::new();
    let mut events = parser.push_chunk(input).expect("parse chunk");
    events.extend(parser.finish().expect("finish parser"));
    events
}

#[test]
fn valid_stream_maps_content_real_tool_name_stop_and_done() {
    // #891: tool_calls entries without `function.name` no longer emit
    // synthetic ToolUse events, and `finish_reason: tool_calls` no longer
    // double-fires. Entries WITH a real name are emitted verbatim.
    let events = parse_all(
        "event: completion.chunk\n\
         data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_1\",\"function\":{\"name\":\"read_file\"}}]},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
         data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":\"stop\"}]}\n\n\
         data: [DONE]\n\n",
    );

    assert!(matches!(&events[0], StreamEvent::AssistantMessage { text } if text == "hello"));
    assert!(matches!(&events[1], StreamEvent::ToolUse { tool, .. } if tool == "read_file"));
    assert!(matches!(&events[2], StreamEvent::AssistantMessage { text } if text == " world"));
    assert!(matches!(&events[3], StreamEvent::Completed { .. }));
    assert_eq!(events.len(), 4);
}

#[test]
fn tool_calls_without_function_name_is_silent() {
    // #891 / MiniMax compatibility: a `tool_calls` array whose entries lack
    // `function.name` (or whose name is empty) must NOT emit a phantom
    // "Using tool_calls" event.
    let events = parse_all(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_1\"}]},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
         data: [DONE]\n\n",
    );

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUse { .. })),
        "no ToolUse events expected for nameless tool_calls; got: {events:?}"
    );
}

#[test]
fn tool_calls_with_empty_function_name_is_silent() {
    // #891: `function.name == ""` must be treated the same as absent.
    let events = parse_all(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"\"}}]},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
         data: [DONE]\n\n",
    );

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUse { .. }))
    );
}

#[test]
fn finish_reason_tool_calls_alone_emits_no_synthetic_event() {
    // #891: a frame with only `finish_reason: tool_calls` must NOT push a
    // phantom ToolUse — the tool calls themselves are streamed via deltas.
    let events =
        parse_all("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n");

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUse { .. })),
        "got: {events:?}"
    );
}

#[test]
fn multiple_tool_calls_in_one_delta_emit_one_event_per_named_call() {
    let events = parse_all(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[\
            {\"function\":{\"name\":\"read_file\"}},\
            {\"function\":{\"name\":\"write_file\"}}\
         ]},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
         data: [DONE]\n\n",
    );

    let tools: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::ToolUse { tool, .. } => Some(tool.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(tools, vec!["read_file", "write_file"]);
}

#[test]
fn malformed_json_inside_data_becomes_unknown() {
    let events = parse_all("data: {\"choices\": [}\n\n");

    assert!(matches!(&events[..], [StreamEvent::Unknown { raw }] if raw == "{\"choices\": [}"));
}

#[test]
fn unexpected_finish_reason_becomes_unknown() {
    let events = parse_all("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n");

    assert!(
        matches!(&events[..], [StreamEvent::Unknown { raw }] if raw == "unexpected finish_reason: length")
    );
}

#[test]
fn missing_choices_array_becomes_unknown() {
    let events =
        parse_all("data: {\"id\":\"chatcmpl_1\",\"object\":\"chat.completion.chunk\"}\n\n");

    assert!(
        matches!(&events[..], [StreamEvent::Unknown { raw }] if raw.contains("\"chatcmpl_1\""))
    );
}

#[test]
fn premature_stream_end_parses_remaining_frame() {
    let events = parse_all(
        "data: {\"choices\":[{\"delta\":{\"content\":\"tail\"},\"finish_reason\":null}]}",
    );

    assert!(matches!(&events[..], [StreamEvent::AssistantMessage { text }] if text == "tail"));
}

#[test]
fn preserves_utf8_split_across_byte_chunks() {
    let frame =
        "data: {\"choices\":[{\"delta\":{\"content\":\"olá\"},\"finish_reason\":null}]}\n\n";
    let split = frame
        .as_bytes()
        .windows("á".len())
        .position(|window| window == "á".as_bytes())
        .expect("accented byte");
    let mut parser = OpenAiCompatibleSseParser::new();

    let first = parser
        .push_bytes(&frame.as_bytes()[..split + 1])
        .expect("first chunk");
    let second = parser
        .push_bytes(&frame.as_bytes()[split + 1..])
        .expect("second chunk");

    assert!(first.is_empty());
    assert!(matches!(&second[..], [StreamEvent::AssistantMessage { text }] if text == "olá"));
}

#[test]
fn multiline_data_fields_are_joined_with_newlines() {
    let events = parse_all(
        "data: {\"choices\":[\n\
         data: {\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}\n\
         data: ]}\n\n",
    );

    assert!(matches!(&events[..], [StreamEvent::AssistantMessage { text }] if text == "hello"));
}

#[test]
fn top_level_error_maps_to_error_event() {
    let events = parse_all("data: {\"error\":{\"message\":\"bad request\"}}\n\n");

    assert!(matches!(&events[..], [StreamEvent::Error { message }] if message == "bad request"));
}

#[test]
fn done_without_prior_frames_completes_stream() {
    let events = parse_all("data: [DONE]\n\n");

    assert!(matches!(&events[..], [StreamEvent::Completed { .. }]));
}

#[test]
fn usage_block_on_stop_frame_emits_token_update() {
    let events = parse_all(
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":120,\"completion_tokens\":45,\"total_tokens\":165}}\n\n\
         data: [DONE]\n\n",
    );

    let usage = events
        .iter()
        .find_map(|event| match event {
            StreamEvent::TokenUpdate { usage } => Some(usage),
            _ => None,
        })
        .expect("expected TokenUpdate from final SSE frame with usage block");
    assert_eq!(usage.input_tokens, 120);
    assert_eq!(usage.output_tokens, 45);
    assert_eq!(usage.cache_read_tokens, 0);

    assert!(
        events
            .iter()
            .any(|event| matches!(event, StreamEvent::Completed { .. })),
        "Completed event must still be emitted alongside TokenUpdate"
    );
}

#[test]
fn usage_block_with_cached_tokens_populates_cache_read() {
    let events = parse_all(
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":200,\"completion_tokens\":50,\"prompt_tokens_details\":{\"cached_tokens\":100}}}\n\n",
    );

    let usage = events
        .iter()
        .find_map(|event| match event {
            StreamEvent::TokenUpdate { usage } => Some(usage),
            _ => None,
        })
        .expect("expected TokenUpdate");
    assert_eq!(usage.input_tokens, 200);
    assert_eq!(usage.output_tokens, 50);
    assert_eq!(usage.cache_read_tokens, 100);
}

#[test]
fn usage_block_all_zero_emits_no_token_update() {
    let events = parse_all(
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":0,\"completion_tokens\":0,\"total_tokens\":0}}\n\n",
    );

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, StreamEvent::TokenUpdate { .. })),
        "all-zero usage must not emit TokenUpdate"
    );
}

#[test]
fn frames_without_usage_block_emit_no_token_update() {
    let events = parse_all(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n",
    );

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, StreamEvent::TokenUpdate { .. })),
        "frames without usage block emit no TokenUpdate"
    );
}

#[test]
fn representative_success_snapshot() {
    let events = parse_all(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\r\n\r\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\r\n\r\n",
    );

    insta::assert_debug_snapshot!("openai_compatible_sse_success", events);
}

#[test]
fn representative_failure_snapshot() {
    let events = parse_all(
        "data: {\"error\":{\"message\":\"quota exceeded\"}}\n\n\
         data: {\"choices\": [}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
    );

    insta::assert_debug_snapshot!("openai_compatible_sse_failure", events);
}

// ---- #846 token-count sanitization ----

#[test]
fn sse_parser_caps_giant_prompt_tokens_and_emits_warning() {
    use crate::session::types::TOKEN_COUNT_CAP;
    let chunk = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\
        \"usage\":{\"prompt_tokens\":999999999999,\"completion_tokens\":1}}\n\ndata: [DONE]\n\n";
    let events = parse_all(chunk);

    let token_update = events
        .iter()
        .find_map(|e| match e {
            StreamEvent::TokenUpdate { usage } => Some(usage),
            _ => None,
        })
        .expect("TokenUpdate must be emitted");
    assert_eq!(token_update.input_tokens, TOKEN_COUNT_CAP);

    assert!(
        events.iter().any(|e| matches!(
            e,
            StreamEvent::Warning { code, .. } if code == "token_count_clamped"
        )),
        "Warning with token_count_clamped must be emitted"
    );
}

#[test]
fn sse_parser_below_cap_emits_no_warning() {
    let chunk = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\
        \"usage\":{\"prompt_tokens\":1000,\"completion_tokens\":200}}\n\ndata: [DONE]\n\n";
    let events = parse_all(chunk);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::Warning { .. })),
    );
}
