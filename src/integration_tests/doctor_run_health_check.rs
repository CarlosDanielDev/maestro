//! Smoke tests for `crate::commands::doctor::run_health_check`.
//!
//! `available` is environment-dependent, so the tests assert only structural
//! properties (count, ids, ordering) and the unknown-id contract.

use crate::commands::doctor::run_health_check;

#[tokio::test]
async fn run_health_check_smoke_returns_one_record_for_claude() {
    let results = run_health_check(&["claude".to_string()]).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].provider_id.as_str(), "claude");
}

#[tokio::test]
async fn run_health_check_unknown_agent_returns_unhealthy() {
    let results = run_health_check(&["definitely-not-real".to_string()]).await;
    assert_eq!(results.len(), 1);
    assert!(!results[0].available);
    let msg = results[0].message.to_lowercase();
    assert!(
        msg.contains("unknown") || msg.contains("not found") || msg.contains("no provider"),
        "expected unknown-id hint in: {}",
        results[0].message
    );
}

#[tokio::test]
async fn run_health_check_returns_one_record_per_id() {
    let ids = vec![
        "claude".to_string(),
        "ollama".to_string(),
        "codex".to_string(),
    ];
    let results = run_health_check(&ids).await;
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].provider_id.as_str(), "claude");
    assert_eq!(results[1].provider_id.as_str(), "ollama");
    assert_eq!(results[2].provider_id.as_str(), "codex");
}
