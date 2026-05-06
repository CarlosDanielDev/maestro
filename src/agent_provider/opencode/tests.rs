use std::collections::BTreeMap;
use std::path::PathBuf;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::session::types::StreamEvent;

fn request() -> AgentRequest {
    let mut request =
        AgentRequest::stream_json("test prompt".into(), "anthropic/claude-sonnet-4-5".into());
    request.cwd = Some(PathBuf::from("/tmp/worktree"));
    request.system_prompt_appendix = Some("appendix".to_string());
    request
}

#[test]
fn stream_args_match_opencode_run_contract() {
    let provider = OpenCodeProvider::with_config(
        "opencode",
        vec!["--log-level".to_string(), "debug".to_string()],
        BTreeMap::new(),
    );

    let args = provider.build_stream_args(&request());

    assert_eq!(args[0], "run");
    assert!(args.windows(2).any(|w| w == ["--format", "json"]));
    assert!(
        args.windows(2)
            .any(|w| w == ["--model", "anthropic/claude-sonnet-4-5"])
    );
    assert!(args.windows(2).any(|w| w == ["--dir", "/tmp/worktree"]));
    assert!(args.windows(2).any(|w| w == ["--log-level", "debug"]));
    assert_eq!(
        args.last().map(String::as_str),
        Some("Maestro session context:\nappendix\n\nUser task:\ntest prompt")
    );
}

#[test]
fn json_parser_captures_stdout_and_emits_unknown() {
    let mut parser = OpenCodeJsonParser::default();

    let events = parser.parse_line(r#"{"type":"session.started","id":"s1"}"#);

    assert!(matches!(events.as_slice(), [StreamEvent::Unknown { .. }]));
    assert_eq!(
        parser.stdout_bytes(),
        b"{\"type\":\"session.started\",\"id\":\"s1\"}\n"
    );
}

#[tokio::test]
async fn run_streams_events_from_mock_opencode_cli_and_records_process_context() {
    let temp = tempfile::tempdir().expect("tempdir");
    let worktree = tempfile::tempdir().expect("worktree");
    let opencode = temp.path().join("opencode");
    std::fs::write(
        &opencode,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\npwd > \"{}\"\nprintf '%s\\n' 'opencode stderr line' >&2\ncat <<'EOF'\n{}\nEOF\n",
            temp.path().join("argv.txt").display(),
            temp.path().join("cwd.txt").display(),
            opencode_fixture_jsonl()
        ),
    )
    .expect("write opencode mock");
    make_executable(&opencode);

    let provider = OpenCodeProvider::with_config(
        opencode.to_string_lossy().to_string(),
        vec!["--log-level".to_string(), "debug".to_string()],
        BTreeMap::new(),
    );
    let mut request = request();
    request.cwd = Some(worktree.path().to_path_buf());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = provider
        .run(request, tx, CancellationToken::new())
        .await
        .expect("mock opencode run");

    assert_eq!(result.exit_code, Some(0));
    assert!(matches!(
        rx.recv().await,
        Some(AgentProviderEvent::Started(AgentRunStarted {
            process_id: Some(_)
        }))
    ));

    let mut saw_unknown = false;
    let mut saw_stderr = false;
    let mut saw_completed = false;
    while let Some(event) = rx.recv().await {
        match event {
            AgentProviderEvent::Stream(StreamEvent::Unknown { raw })
                if raw.contains("session.started") =>
            {
                saw_unknown = true;
            }
            AgentProviderEvent::Stream(StreamEvent::Error { message })
                if message == "opencode stderr line" =>
            {
                saw_stderr = true;
            }
            AgentProviderEvent::Stream(StreamEvent::Completed { .. }) => {
                saw_completed = true;
            }
            _ => {}
        }
        if saw_unknown && saw_stderr && saw_completed {
            break;
        }
    }

    assert!(saw_unknown);
    assert!(saw_stderr);
    assert!(saw_completed);

    let argv = std::fs::read_to_string(temp.path().join("argv.txt")).expect("argv");
    assert!(argv.contains("run\n"));
    assert!(argv.contains("--format\njson"));
    assert!(argv.contains("--model\nanthropic/claude-sonnet-4-5"));
    assert!(argv.contains("--dir\n"));
    assert!(argv.contains("--log-level\ndebug"));
    let recorded_cwd = PathBuf::from(
        std::fs::read_to_string(temp.path().join("cwd.txt"))
            .expect("cwd")
            .trim(),
    )
    .canonicalize()
    .expect("recorded cwd canonicalizes");
    let expected_cwd = worktree
        .path()
        .canonicalize()
        .expect("worktree canonicalizes");
    assert_eq!(recorded_cwd, expected_cwd);
}

#[tokio::test]
async fn run_returns_session_error_on_nonzero_exit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let opencode = temp.path().join("opencode");
    std::fs::write(
        &opencode,
        "#!/bin/sh\nprintf '%s\\n' 'auth failed' >&2\nexit 42\n",
    )
    .expect("write opencode mock");
    make_executable(&opencode);

    let provider = OpenCodeProvider::new(opencode.to_string_lossy().to_string());
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut request = request();
    request.cwd = None;
    let err = provider
        .run(request, tx, CancellationToken::new())
        .await
        .expect_err("nonzero exit should fail");

    assert!(
        err.to_string().contains("auth failed"),
        "unexpected error: {err}"
    );
    assert!(err.to_string().contains("opencode exited with status"));
}

#[tokio::test]
async fn missing_binary_surfaces_install_instructions() {
    let provider = OpenCodeProvider::new("/tmp/maestro-missing-opencode-binary");
    let (tx, _rx) = mpsc::unbounded_channel();

    let err = provider
        .run(request(), tx, CancellationToken::new())
        .await
        .expect_err("missing binary should fail");

    assert!(err.to_string().contains("opencode CLI not found"));
    assert!(
        err.to_string()
            .contains("brew install anomalyco/tap/opencode")
    );
}

fn opencode_fixture_jsonl() -> &'static str {
    r#"{"type":"session.started","id":"s1"}
{"type":"message.delta","text":"Done."}"#
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod");
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) {}
