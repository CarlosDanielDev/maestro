use std::collections::BTreeMap;
use std::path::PathBuf;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::*;

fn request() -> AgentRequest {
    let mut request = AgentRequest::stream_json("test prompt".into(), "gpt-5.4-codex".into());
    request.cwd = Some(PathBuf::from("/tmp/worktree"));
    request.images = vec![PathBuf::from("a.png"), PathBuf::from("b.jpg")];
    request.system_prompt_appendix = Some("appendix".to_string());
    request
}

#[test]
fn stream_args_match_codex_exec_contract() {
    let provider = CodexProvider::with_config(
        "codex",
        Some("workspace-write".to_string()),
        Some(true),
        Some("work".to_string()),
        BTreeMap::from([(
            "approval_policy".to_string(),
            toml::Value::String("never".to_string()),
        )]),
        vec!["--reasoning-effort".to_string(), "high".to_string()],
        BTreeMap::new(),
        Some(true),
    );

    let args = provider.build_stream_args(&request());
    assert_eq!(args[0], "exec");
    assert!(args.iter().any(|arg| arg == "--json"));
    assert!(args.windows(2).any(|w| w == ["--model", "gpt-5.4-codex"]));
    assert!(
        args.windows(2)
            .any(|w| w == ["--sandbox", "workspace-write"])
    );
    assert!(args.windows(2).any(|w| w == ["--cd", "/tmp/worktree"]));
    assert!(args.iter().any(|arg| arg == "--ephemeral"));
    assert!(args.windows(2).any(|w| w == ["--profile", "work"]));
    assert!(
        args.windows(2)
            .any(|w| w == ["--config", "approval_policy=\"never\""])
    );
    assert!(args.windows(2).any(|w| w == ["--image", "a.png"]));
    assert!(args.windows(2).any(|w| w == ["--image", "b.jpg"]));
    assert!(args.windows(2).any(|w| w == ["--reasoning-effort", "high"]));
    assert!(!args.iter().any(|arg| arg == "--full-auto"));
    assert_eq!(
        args.last().map(String::as_str),
        Some("Maestro session context:\nappendix\n\nUser task:\ntest prompt")
    );
}

#[tokio::test]
async fn run_streams_events_from_mock_codex_cli_and_records_process_context() {
    let temp = tempfile::tempdir().expect("tempdir");
    let worktree = tempfile::tempdir().expect("worktree");
    let codex = temp.path().join("codex");
    std::fs::write(
        &codex,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\npwd > \"{}\"\nprintf '%s\\n' 'codex stderr line' >&2\ncat <<'EOF'\n{}\nEOF\n",
            temp.path().join("argv.txt").display(),
            temp.path().join("cwd.txt").display(),
            codex_fixture_jsonl()
        ),
    )
    .expect("write codex mock");
    make_executable(&codex);

    let provider = CodexProvider::new(codex.to_string_lossy().to_string());
    let mut request = request();
    request.cwd = Some(worktree.path().to_path_buf());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = provider
        .run(request, tx, CancellationToken::new())
        .await
        .expect("mock codex run");

    assert_eq!(result.exit_code, Some(0));
    assert!(matches!(
        rx.recv().await,
        Some(AgentProviderEvent::Started(AgentRunStarted {
            process_id: Some(_)
        }))
    ));

    let mut saw_message = false;
    let mut saw_stderr = false;
    let mut saw_completed = false;
    while let Some(event) = rx.recv().await {
        match event {
            AgentProviderEvent::Stream(StreamEvent::AssistantMessage { text })
                if text == "Done." =>
            {
                saw_message = true;
            }
            AgentProviderEvent::Stream(StreamEvent::Error { message })
                if message == "codex stderr line" =>
            {
                saw_stderr = true;
            }
            AgentProviderEvent::Stream(StreamEvent::Completed { .. }) => {
                saw_completed = true;
            }
            _ => {}
        }
        if saw_message && saw_stderr && saw_completed {
            break;
        }
    }

    assert!(saw_message);
    assert!(saw_stderr);
    assert!(saw_completed);

    let argv = std::fs::read_to_string(temp.path().join("argv.txt")).expect("argv");
    assert!(argv.contains("exec\n"));
    assert!(argv.contains("--json\n"));
    assert!(argv.contains("--model\ngpt-5.4-codex"));
    assert!(argv.contains("--sandbox\nworkspace-write"));
    assert!(argv.contains("--cd\n"));
    assert!(argv.contains("--image\na.png"));
    assert!(argv.contains("--image\nb.jpg"));
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
    let codex = temp.path().join("codex");
    std::fs::write(
        &codex,
        "#!/bin/sh\nprintf '%s\\n' 'auth failed' >&2\nexit 42\n",
    )
    .expect("write codex mock");
    make_executable(&codex);

    let provider = CodexProvider::new(codex.to_string_lossy().to_string());
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
    assert!(err.to_string().contains("codex exited with status"));
}

fn codex_fixture_jsonl() -> &'static str {
    r#"{"type":"thread.started","thread_id":"t1"}
{"type":"item.completed","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done."}]}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":2}}"#
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
