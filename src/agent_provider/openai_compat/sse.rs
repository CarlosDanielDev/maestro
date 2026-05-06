#![deny(clippy::unwrap_used)]

use serde_json::Value;

use crate::session::types::StreamEvent;

#[derive(Debug, Clone, Default)]
pub struct OpenAiCompatibleSseParser {
    buffer: Vec<u8>,
    completed: bool,
}

impl OpenAiCompatibleSseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_chunk(&mut self, chunk: &str) -> Result<Vec<StreamEvent>, String> {
        self.push_bytes(chunk.as_bytes())
    }

    pub fn push_bytes(&mut self, chunk: &[u8]) -> Result<Vec<StreamEvent>, String> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some((frame_end, delimiter_len)) = find_frame_end(&self.buffer) {
            let frame = self.buffer[..frame_end].to_vec();
            let drain_to = frame_end + delimiter_len;
            self.buffer.drain(..drain_to);
            events.extend(self.parse_frame(&frame)?);
        }

        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Vec<StreamEvent>, String> {
        if self.buffer.iter().all(u8::is_ascii_whitespace) {
            return Ok(Vec::new());
        }

        let frame = std::mem::take(&mut self.buffer);
        self.parse_frame(&frame)
    }

    fn parse_frame(&mut self, frame: &[u8]) -> Result<Vec<StreamEvent>, String> {
        let Ok(frame) = std::str::from_utf8(frame) else {
            return Ok(vec![StreamEvent::Unknown {
                raw: String::from_utf8_lossy(frame).to_string(),
            }]);
        };

        let data = frame
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .filter_map(data_field_value)
            .collect::<Vec<_>>()
            .join("\n");

        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        if data.trim() == "[DONE]" {
            if self.completed {
                return Ok(Vec::new());
            }
            self.completed = true;
            return Ok(vec![StreamEvent::Completed { cost_usd: 0.0 }]);
        }

        let value: Value = match serde_json::from_str(&data) {
            Ok(value) => value,
            Err(_) => {
                return Ok(vec![StreamEvent::Unknown { raw: data }]);
            }
        };
        Ok(self.parse_json_event(&value))
    }

    fn parse_json_event(&mut self, value: &Value) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        if let Some(error) = value.get("error") {
            events.push(StreamEvent::Error {
                message: error_message(error),
            });
            return events;
        }

        let Some(choice) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return vec![StreamEvent::Unknown {
                raw: value.to_string(),
            }];
        };

        if let Some(content) = choice
            .get("delta")
            .and_then(|delta| delta.get("content"))
            .and_then(Value::as_str)
            && !content.is_empty()
        {
            events.push(StreamEvent::AssistantMessage {
                text: content.to_string(),
            });
        }

        if choice
            .get("delta")
            .and_then(|delta| delta.get("tool_calls"))
            .and_then(Value::as_array)
            .is_some_and(|calls| !calls.is_empty())
        {
            events.push(StreamEvent::ToolUse {
                tool: "tool_calls".to_string(),
                file_path: None,
                command_preview: None,
                subagent_name: None,
            });
        }

        match choice.get("finish_reason").and_then(Value::as_str) {
            Some("stop") if !self.completed => {
                self.completed = true;
                events.push(StreamEvent::Completed { cost_usd: 0.0 });
            }
            Some("stop") => {}
            Some("tool_calls") => {
                events.push(StreamEvent::ToolUse {
                    tool: "tool_calls".to_string(),
                    file_path: None,
                    command_preview: None,
                    subagent_name: None,
                });
            }
            Some(other) => events.push(StreamEvent::Unknown {
                raw: format!("unexpected finish_reason: {other}"),
            }),
            None => {}
        }

        events
    }
}

fn find_frame_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = find_bytes(buffer, b"\n\n").map(|index| (index, 2));
    let crlf = find_bytes(buffer, b"\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(lf), Some(crlf)) => Some(if lf.0 < crlf.0 { lf } else { crlf }),
        (Some(lf), None) => Some(lf),
        (None, Some(crlf)) => Some(crlf),
        (None, None) => None,
    }
}

fn find_bytes(buffer: &[u8], needle: &[u8]) -> Option<usize> {
    buffer
        .windows(needle.len())
        .position(|window| window == needle)
}

fn data_field_value(line: &str) -> Option<&str> {
    let value = line.strip_prefix("data:")?;
    Some(value.strip_prefix(' ').unwrap_or(value))
}

fn error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| error.to_string())
}

#[cfg(test)]
mod tests {
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
        let events =
            parse_all("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n");

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

        assert!(
            matches!(&events[..], [StreamEvent::Error { message }] if message == "bad request")
        );
    }

    #[test]
    fn done_without_prior_frames_completes_stream() {
        let events = parse_all("data: [DONE]\n\n");

        assert!(matches!(&events[..], [StreamEvent::Completed { .. }]));
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
}
