use super::*;

#[tokio::test]
async fn provider_without_quota_source_is_no_data_and_touches_nothing() {
    let h = Harness::new();

    assert!(h.fetcher.windows("ollama").await.is_none());
    assert_eq!(h.calls(), 0);
    assert!(
        !h.cache_dir.exists(),
        "no cache file for a provider with no source"
    );
}

#[tokio::test]
async fn missing_credentials_is_no_data() {
    let h = Harness::new();
    std::fs::remove_file(h.home.join(".claude").join(".credentials.json")).expect("remove");

    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(h.calls(), 0);
}

#[tokio::test]
async fn token_from_credentials_file_is_sent_to_fake_only() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));

    h.fetcher.windows("claude").await.expect("windows");

    let tokens = h.http.tokens.lock().expect("tokens lock").clone();
    assert_eq!(tokens, vec![SECRET.to_string()]);
}

#[tokio::test]
async fn credentials_file_is_never_written() {
    let h = Harness::new();
    let path = h.home.join(".claude").join(".credentials.json");
    let before = std::fs::read(&path).expect("read before");
    h.queue(ok(FIXTURE));

    h.fetcher.windows("claude").await.expect("windows");

    assert_eq!(std::fs::read(&path).expect("read after"), before);
}

#[tokio::test]
async fn cache_file_on_disk_never_contains_token() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("windows");

    let on_disk = std::fs::read_to_string(h.cache_path()).expect("cache file");

    assert!(!on_disk.contains(SECRET));
    assert!(on_disk.contains("five_hour"), "the windows were persisted");
}

#[cfg(unix)]
#[tokio::test]
async fn cache_dir_and_files_are_private_on_unix() {
    use std::os::unix::fs::PermissionsExt;

    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("windows");

    let mode = |path: &Path| {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode(&h.cache_dir), 0o700, "cache dir");
    assert_eq!(mode(&h.cache_path()), 0o600, "cache file");
    assert_eq!(mode(&h.lock_path()), 0o600, "lock file");
}

#[test]
fn token_debug_is_redacted() {
    let h = Harness::new();
    let token = credentials::load_claude_access_token(&h.home).expect("token");

    let shown = format!("{token:?}");

    assert!(!shown.contains(SECRET));
    assert_eq!(shown, "AccessToken(<redacted>)");
}

/// Smoke check only: `Debug` on the fetcher never prints the token. The real
/// leak check is `cache_file_on_disk_never_contains_token` above.
#[tokio::test]
async fn fetcher_debug_smoke_omits_token() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("windows");

    assert!(!format!("{:?}", h.fetcher).contains(SECRET));
}
