#![deny(clippy::unwrap_used)]

use std::str::FromStr;

use super::codex::parser::CodexStreamParser;
use super::openai_compat::sse::OpenAiCompatibleSseParser;
use super::opencode::OpenCodeJsonParser;
use super::qwen_parser::QwenStreamParser;
use crate::session::parser::parse_stream_line;
use crate::session::types::StreamEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentParserKind {
    Claude,
    Codex,
    Qwen,
    Opencode,
    Ollama,
    Minimax,
}

impl FromStr for AgentParserKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "qwen" => Ok(Self::Qwen),
            "opencode" => Ok(Self::Opencode),
            "ollama" => Ok(Self::Ollama),
            "minimax" => Ok(Self::Minimax),
            other => Err(format!("unsupported agent parser kind `{other}`")),
        }
    }
}

pub trait AgentStreamParser: Send {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent>;

    fn finish(&mut self) -> Vec<StreamEvent> {
        Vec::new()
    }
}

pub fn parser_for_kind(kind: AgentParserKind) -> Box<dyn AgentStreamParser> {
    match kind {
        AgentParserKind::Claude => Box::new(ClaudeStreamParser),
        AgentParserKind::Codex => Box::<CodexStreamParser>::default(),
        AgentParserKind::Qwen => Box::<QwenStreamParser>::default(),
        AgentParserKind::Opencode => Box::<OpenCodeJsonParser>::default(),
        AgentParserKind::Ollama | AgentParserKind::Minimax => Box::new(OpenAiSseLineParser {
            parser: OpenAiCompatibleSseParser::new(),
        }),
    }
}

pub fn parser_for_provider(provider: &str) -> Result<Box<dyn AgentStreamParser>, String> {
    provider.parse().map(parser_for_kind)
}

#[derive(Debug)]
struct ClaudeStreamParser;

impl AgentStreamParser for ClaudeStreamParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        parse_stream_line(line)
    }
}

#[derive(Debug)]
struct OpenAiSseLineParser {
    parser: OpenAiCompatibleSseParser,
}

impl AgentStreamParser for OpenAiSseLineParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        match self.parser.push_chunk(&format!("{line}\n")) {
            Ok(events) => events,
            Err(message) => vec![StreamEvent::Error { message }],
        }
    }

    fn finish(&mut self) -> Vec<StreamEvent> {
        match self.parser.finish() {
            Ok(events) => events,
            Err(message) => vec![StreamEvent::Error { message }],
        }
    }
}

impl AgentStreamParser for CodexStreamParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        CodexStreamParser::parse_line(self, line)
    }
}

impl AgentStreamParser for QwenStreamParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        QwenStreamParser::parse_line(self, line)
    }
}

impl AgentStreamParser for OpenCodeJsonParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        OpenCodeJsonParser::parse_line(self, line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_selects_all_agent_parsers() {
        for provider in ["claude", "codex", "qwen", "opencode", "ollama", "minimax"] {
            parser_for_provider(provider).expect("known provider");
        }
    }

    #[test]
    fn ollama_and_minimax_share_sse_parser_behavior() {
        for kind in [AgentParserKind::Ollama, AgentParserKind::Minimax] {
            let mut parser = parser_for_kind(kind);
            let mut events = parser.parse_line(
                r#"data: {"choices":[{"delta":{"content":"hi"},"finish_reason":null}]}"#,
            );
            events.extend(parser.parse_line(""));
            assert!(matches!(
                events.as_slice(),
                [StreamEvent::AssistantMessage { text }] if text == "hi"
            ));
        }
    }
}
