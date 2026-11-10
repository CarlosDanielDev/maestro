//! Remote bearer-credential guard tests (#1030) — split out of
//! `tests.rs` to keep it under the 400-line guardrail.

use super::*;

// --- Remote bearer-credential guard (#1030) ---

#[test]
fn client_allows_bearer_only_for_default_loopback_or_confirmed_hosts() {
    for base_url in [
        crate::config::agents_remote_auth::MINIMAX_DEFAULT_BASE_URL,
        "http://127.0.0.1:9000",
        "http://localhost:11434",
    ] {
        let client = MinimaxClient::with_api_key_lookup(
            base_url,
            Duration::from_secs(1),
            "MINIMAX_API_KEY",
            |_| None,
        )
        .expect("client");
        assert!(
            client.remote_auth_allowed(),
            "default/loopback host {base_url} may receive bearer credentials"
        );
    }

    let client = MinimaxClient::with_api_key_lookup(
        "http://collector.example/v1",
        Duration::from_secs(1),
        "MINIMAX_API_KEY",
        |_| None,
    )
    .expect("client");
    assert!(
        !client.remote_auth_allowed(),
        "remote host must be refused without confirmation"
    );
    assert!(
        client.with_allow_remote_auth(true).remote_auth_allowed(),
        "allow_remote_auth must confirm the remote host"
    );
}

#[tokio::test]
async fn models_health_refuses_remote_host_without_confirmation() {
    let provider = MinimaxProvider::new_with_api_key_lookup(
        "minimax",
        "http://collector.example/v1",
        "MiniMax-M2.7",
        5,
        Some("MINIMAX_API_KEY".to_string()),
        |_| Some("test-key".to_string()),
    )
    .expect("provider");

    let err = provider
        .health_check()
        .await
        .expect_err("credentialed request to remote host must refuse");
    assert!(
        err.to_string().contains("refusing"),
        "expected a credential refusal, got: {err}"
    );
}

#[tokio::test]
async fn chat_stream_refuses_remote_host_without_confirmation() {
    let provider = MinimaxProvider::new_with_api_key_lookup(
        "minimax",
        "http://collector.example/v1",
        "MiniMax-M2.7",
        5,
        Some("MINIMAX_API_KEY".to_string()),
        |_| Some("test-key".to_string()),
    )
    .expect("provider");

    let (tx, _rx) = mpsc::unbounded_channel();
    let err = provider
        .run(
            AgentRequest::stream_json("hi".to_string(), "MiniMax-M2.7".to_string()),
            tx,
            CancellationToken::new(),
        )
        .await
        .expect_err("credentialed request to remote host must refuse");
    assert!(
        err.to_string().contains("refusing"),
        "expected a credential refusal, got: {err}"
    );
}
