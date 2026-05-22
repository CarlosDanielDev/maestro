//! Test suite for `MinimaxProvider`, extracted from `mod.rs` to keep the
//! production file under the 400-line cap. Re-included via
//! `#[cfg(test)] #[path = "tests.rs"] mod tests;` in `mod.rs`.

use super::*;
use crate::session::types::StreamEvent;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn pre_spawn_gate_refuses_at_95_percent_without_force() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("minimax-quota.json");
    let quota = std::sync::Arc::new(
        crate::agent_provider::minimax::MinimaxQuota::open_with(
            path,
            Box::new(crate::agent_provider::minimax::quota::SystemClock),
            100,
        )
        .expect("quota"),
    );
    // Push 95 samples through the public record() API.
    for _ in 0..95 {
        quota.record().expect("record");
    }

    // Provider need not make an HTTP request when the gate refuses,
    // but we still need a working base URL for construction.
    let base_url = spawn_test_server("HTTP/1.1 200 OK\r\n\r\n").await;
    let provider = MinimaxProvider::new_with_api_key_lookup(
        "minimax",
        base_url,
        "MiniMax-M2.7",
        5,
        Some("MINIMAX_API_KEY".to_string()),
        |_| Some("test-key".to_string()),
    )
    .expect("provider")
    .with_quota(quota);

    let (tx, _rx) = mpsc::unbounded_channel();
    let err = provider
        .run(
            AgentRequest::stream_json("hi".to_string(), "MiniMax-M2.7".to_string()),
            tx,
            CancellationToken::new(),
        )
        .await
        .expect_err("95% quota should refuse spawn");

    assert!(err.to_string().contains("refusing spawn"));
    assert!(err.to_string().contains("--force-quota"));
}

#[tokio::test]
async fn pre_spawn_gate_allows_when_per_request_force_is_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("minimax-quota.json");
    let quota = std::sync::Arc::new(
        crate::agent_provider::minimax::MinimaxQuota::open_with(
            path,
            Box::new(crate::agent_provider::minimax::quota::SystemClock),
            100,
        )
        .expect("quota"),
    );
    for _ in 0..95 {
        quota.record().expect("record");
    }

    let base_url = spawn_test_server(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
         data: [DONE]\n\n",
    )
    .await;
    let provider = MinimaxProvider::new_with_api_key_lookup(
        "minimax",
        base_url,
        "MiniMax-M2.7",
        5,
        Some("MINIMAX_API_KEY".to_string()),
        |_| Some("test-key".to_string()),
    )
    .expect("provider")
    .with_quota(quota);

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut request = AgentRequest::stream_json("hi".to_string(), "MiniMax-M2.7".to_string());
    request.force = true;
    provider
        .run(request, tx, CancellationToken::new())
        .await
        .expect("forced spawn should succeed past gate");
}

#[tokio::test]
async fn streams_openai_compatible_sse() {
    let base_url = spawn_test_server(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n\
         data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
         data: [DONE]\n\n",
    )
    .await;
    let provider = MinimaxProvider::new_with_api_key_lookup(
        "minimax",
        base_url,
        "MiniMax-M2.7",
        5,
        Some("MINIMAX_API_KEY".to_string()),
        |_| Some("test-key".to_string()),
    )
    .expect("provider");
    let (tx, mut rx) = mpsc::unbounded_channel();

    provider
        .run(
            AgentRequest::stream_json("say hi".to_string(), "MiniMax-M2.7".to_string()),
            tx,
            CancellationToken::new(),
        )
        .await
        .expect("run");

    assert!(matches!(
        rx.recv().await,
        Some(AgentProviderEvent::Started(_))
    ));
    assert!(matches!(
        rx.recv().await,
        Some(AgentProviderEvent::Stream(StreamEvent::AssistantMessage { text })) if text == "hello"
    ));
    assert!(matches!(
        rx.recv().await,
        Some(AgentProviderEvent::Stream(StreamEvent::Completed { .. }))
    ));
}

#[tokio::test]
async fn maps_unauthorized_status() {
    let base_url = spawn_test_server(
        "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\n\r\n\
         {\"error\":\"invalid key\"}",
    )
    .await;
    let provider = MinimaxProvider::new_with_api_key_lookup(
        "minimax",
        base_url,
        "MiniMax-M2.7",
        5,
        Some("MINIMAX_API_KEY".to_string()),
        |_| Some("test-key".to_string()),
    )
    .expect("provider");
    let (tx, _rx) = mpsc::unbounded_channel();
    let err = provider
        .run(
            AgentRequest::stream_json("say hi".to_string(), "MiniMax-M2.7".to_string()),
            tx,
            CancellationToken::new(),
        )
        .await
        .expect_err("401 should fail");

    assert!(
        err.to_string()
            .contains("invalid MINIMAX_API_KEY — check your key at platform.minimax.io")
    );
}

#[tokio::test]
async fn missing_api_key_uses_env_var_name_only() {
    let base_url =
        spawn_test_server("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"data\":[]}")
            .await;
    let provider = MinimaxProvider::new_with_api_key_lookup(
        "minimax",
        base_url,
        "MiniMax-M2.7",
        5,
        Some("MINIMAX_API_KEY".to_string()),
        |_| None,
    )
    .expect("provider");
    let err = provider.health_check().await.expect_err("missing key");
    let rendered = err.to_string();

    assert!(rendered.contains("set MINIMAX_API_KEY to your MiniMax API key"));
    assert!(!rendered.contains("secret-value"));
}

#[tokio::test]
async fn health_check_passes_when_models_endpoint_is_reachable() {
    let base_url =
        spawn_test_server("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"data\":[]}")
            .await;
    let provider = MinimaxProvider::new_with_api_key_lookup(
        "minimax",
        base_url,
        "MiniMax-M2.7",
        5,
        Some("MINIMAX_API_KEY".to_string()),
        |_| Some("test-key".to_string()),
    )
    .expect("provider");

    let health = provider.health_check().await.expect("health check");

    assert!(health.available);
    assert!(health.message.contains("models endpoint reachable"));
}

#[tokio::test]
async fn health_check_maps_unauthorized_without_exposing_key_value() {
    let base_url = spawn_test_server(
        "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\n\r\n\
         {\"error\":\"invalid key\"}",
    )
    .await;
    let provider = MinimaxProvider::new_with_api_key_lookup(
        "minimax",
        base_url,
        "MiniMax-M2.7",
        5,
        Some("MINIMAX_API_KEY".to_string()),
        |_| Some("secret-value".to_string()),
    )
    .expect("provider");

    let err = provider.health_check().await.expect_err("401");
    let rendered = err.to_string();

    assert!(rendered.contains("invalid MINIMAX_API_KEY"));
    assert!(!rendered.contains("secret-value"));
}

async fn spawn_test_server(response: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut request = vec![0_u8; 2048];
            let _ = socket.read(&mut request).await;
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });
    format!("http://{addr}")
}
