use super::*;

fn parse_all(input: &str) -> Vec<StreamEvent> {
    let mut parser = OpenAiCompatibleSseParser::new();
    let mut events = parser.push_chunk(input).expect("parse chunk");
    events.extend(parser.finish().expect("finish parser"));
    events
}

#[test]
fn valid_stream_maps_content_tool_calls_stop_and_done() {
    let events = parse_all(
        "event: completion.chunk\n\
         data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_1\"}]},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
         data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":\"stop\"}]}\n\n\
         data: [DONE]\n\n",
    );

    assert!(matches!(&events[0], StreamEvent::AssistantMessage { text } if text == "hello"));
    assert!(matches!(&events[1], StreamEvent::ToolUse { tool, .. } if tool == "tool_calls"));
    assert!(matches!(&events[2], StreamEvent::ToolUse { tool, .. } if tool == "tool_calls"));
    assert!(matches!(&events[3], StreamEvent::AssistantMessage { text } if text == " world"));
    assert!(matches!(&events[4], StreamEvent::Completed { .. }));
    assert_eq!(events.len(), 5);
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
