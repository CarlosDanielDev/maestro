#![deny(clippy::unwrap_used)]

use serde_json::Value;

use crate::session::types::StreamEvent;

#[derive(Debug, Clone, Default)]
pub struct OpenAiCompatibleSseParser {
    buffer: String,
    completed: bool,
}

impl OpenAiCompatibleSseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_chunk(&mut self, chunk: &str) -> Result<Vec<StreamEvent>, String> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();

        while let Some(frame_end) = find_frame_end(&self.buffer) {
            let frame = self.buffer[..frame_end].to_string();
            let drain_to = if self.buffer[frame_end..].starts_with("\r\n\r\n") {
                frame_end + 4
            } else {
                frame_end + 2
            };
            self.buffer.drain(..drain_to);
            events.extend(self.parse_frame(&frame)?);
        }

        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Vec<StreamEvent>, String> {
        if self.buffer.trim().is_empty() {
            return Ok(Vec::new());
        }

        let frame = std::mem::take(&mut self.buffer);
        self.parse_frame(&frame)
    }

    fn parse_frame(&mut self, frame: &str) -> Result<Vec<StreamEvent>, String> {
        let data = frame
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
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

        let value: Value = serde_json::from_str(&data)
            .map_err(|err| format!("malformed OpenAI-compatible SSE JSON: {err}"))?;
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
                raw: format!("finish_reason:{other}"),
            }),
            None => {}
        }

        events
    }
}

fn find_frame_end(buffer: &str) -> Option<usize> {
    buffer.find("\n\n").or_else(|| buffer.find("\r\n\r\n"))
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

    #[test]
    fn parses_content_and_stop() {
        let mut parser = OpenAiCompatibleSseParser::new();
        let events = parser
            .push_chunk(
                "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n\
                 data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                 data: [DONE]\n\n",
            )
            .expect("valid sse");

        assert!(matches!(
            events.first(),
            Some(StreamEvent::AssistantMessage { text }) if text == "hello"
        ));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, StreamEvent::Completed { .. }))
                .count(),
            1
        );
    }
}
