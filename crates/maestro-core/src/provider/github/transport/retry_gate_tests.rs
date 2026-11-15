use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

// -- idempotency gate: ambiguous network errors vs mutating commands --

#[tokio::test]
async fn mutating_command_with_connection_reset_is_not_retried() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = with_transient_retries(
        || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("gh command failed: read tcp 1.2.3.4:443: connection reset by peer")
            }
        },
        false,
        false,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn mutating_command_with_http_502_is_not_retried() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = with_transient_retries(
        || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("gh: HTTP 502: Bad Gateway (https://api.github.com/graphql)")
            }
        },
        false,
        false,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn mutating_command_with_http_504_is_not_retried() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = with_transient_retries(
        || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("gh: HTTP 504: Gateway Timeout (https://api.github.com/graphql)")
            }
        },
        false,
        false,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn idempotent_command_with_http_502_is_retried_to_cap() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = with_transient_retries(
        || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("gh: HTTP 502: Bad Gateway (https://api.github.com/graphql)")
            }
        },
        false,
        true,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn idempotent_command_with_http_504_is_retried_to_cap() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = with_transient_retries(
        || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("gh: HTTP 504: Gateway Timeout (https://api.github.com/graphql)")
            }
        },
        false,
        true,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn mutating_command_with_http_503_is_still_retried_to_cap() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = with_transient_retries(
        || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("gh: HTTP 503: Service Unavailable (https://api.github.com/graphql)")
            }
        },
        false,
        false,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}
