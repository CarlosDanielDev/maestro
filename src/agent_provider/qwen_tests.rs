use std::collections::BTreeMap;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::qwen::QwenProvider;
use super::qwen_parser::QwenStreamParser;
use super::types::{AgentProvider, AgentProviderEvent, AgentRequest, AgentRunStarted};
use crate::session::types::StreamEvent;

fn request() -> AgentRequest {
    let mut request = AgentRequest::stream_json("test prompt".into(), "qwen-test".into());
    request.permission_mode = Some("bypassPermissions".to_string());
    request.system_prompt_appendix = Some("appendix".to_string());
    request
}

#[test]
fn stream_args_match_qwen_research_contract() {
    let provider = QwenProvider::with_config(
        "qwen",
        vec!["--auth-type".to_string(), "openai".to_string()],
        BTreeMap::new(),
    );
    let args = provider.build_stream_args(&request());
    assert!(
        args.windows(2)
            .any(|w| w == ["--output-format", "stream-json"])
    );
    assert!(args.iter().any(|arg| arg == "--bare"));
    assert!(args.iter().any(|arg| arg == "--include-partial-messages"));
    assert!(args.windows(2).any(|w| w == ["--model", "qwen-test"]));
    assert!(args.windows(2).any(|w| w == ["--approval-mode", "yolo"]));
    assert!(args.windows(2).any(|w| w == ["--auth-type", "openai"]));
    assert!(args.windows(2).any(|w| w
        == [
            "--prompt",
            "Maestro session context:\nappendix\n\nUser task:\ntest prompt"
        ]));
}

#[test]
fn text_args_use_qwen_text_output() {
    let provider = QwenProvider::default();
    let args = provider.build_text_args(&request());
    assert!(args.windows(2).any(|w| w == ["--output-format", "text"]));
    assert!(!args.iter().any(|arg| arg == "--include-partial-messages"));
}

#[test]
fn fixture_maps_to_stream_events() {
    let fixture = include_str!("../../tests/fixtures/qwen_output_sample.jsonl");
    let mut parser = QwenStreamParser::default();
    let events: Vec<StreamEvent> = fixture
        .lines()
        .flat_map(|line| parser.parse_line(line))
        .collect();

    assert!(events.iter().any(|event| {
        matches!(
            event,
            StreamEvent::ToolUse {
                tool,
                file_path: Some(path),
                ..
            } if tool == "read_file" && path.ends_with("README.md")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            StreamEvent::ToolResult {
                tool,
                is_error: false,
            } if tool == "read_file"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            StreamEvent::AssistantMessage { text }
                if text == "The README title identifies the project as Maestro."
        )
    }));
    assert!(
        events.iter().any(|event| {
            matches!(event, StreamEvent::Completed { cost_usd } if *cost_usd == 0.0)
        })
    );
}

#[test]
fn api_error_text_maps_to_error_even_with_success_result() {
    let mut parser = QwenStreamParser::default();
    let events = parser.parse_line(
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"[API Error: bad gateway]"}]}}"#,
    );

    assert!(matches!(
        events.first(),
        Some(StreamEvent::Error { message }) if message == "[API Error: bad gateway]"
    ));
}

#[tokio::test]
async fn run_streams_events_from_mock_qwen_cli() {
    let temp = tempfile::tempdir().expect("tempdir");
    let qwen = temp.path().join("qwen");
    std::fs::write(
        &qwen,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\ncat \"{}\"\n",
            temp.path().join("argv.txt").display(),
            std::path::Path::new("tests/fixtures/qwen_output_sample.jsonl").display()
        ),
    )
    .expect("write qwen mock");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&qwen).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&qwen, perms).expect("chmod");
    }

    let provider = QwenProvider::new(qwen.to_string_lossy().to_string());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = provider
        .run(request(), tx, CancellationToken::new())
        .await
        .expect("mock qwen run");

    assert_eq!(result.exit_code, Some(0));
    assert!(matches!(
        rx.recv().await,
        Some(AgentProviderEvent::Started(AgentRunStarted {
            process_id: Some(_)
        }))
    ));

    let mut saw_completed = false;
    while let Some(event) = rx.recv().await {
        if matches!(
            event,
            AgentProviderEvent::Stream(StreamEvent::Completed { .. })
        ) {
            saw_completed = true;
            break;
        }
    }
    assert!(saw_completed);

    let argv = std::fs::read_to_string(temp.path().join("argv.txt")).expect("argv");
    assert!(argv.contains("--bare"));
    assert!(argv.contains("--output-format\nstream-json"));
    assert!(argv.contains("--include-partial-messages"));
}
