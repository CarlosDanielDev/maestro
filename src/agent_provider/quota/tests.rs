use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, TimeZone, Utc};
use tempfile::TempDir;

use super::QuotaFetcher;
use super::cache::{self, LockOutcome};
use super::credentials;
use super::fetch::{HttpResponse, QuotaHttp};
use super::types::TransportError;
use crate::agent_provider::minimax::quota::Clock;

const FIXTURE: &str = include_str!("../../../tests/fixtures/claude_oauth_usage.json");
const SECRET: &str = "sk-ant-oat01-TEST-SECRET-DO-NOT-LEAK";

#[derive(Debug)]
struct SharedClock(Arc<Mutex<DateTime<Utc>>>);

impl Clock for SharedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock lock")
    }
}

/// Scripted HTTP. Panics on a call with no scripted reply, so an unexpected
/// network call fails the test. Records the token it was given.
#[derive(Clone, Default)]
struct FakeHttp {
    replies: Arc<Mutex<VecDeque<Result<HttpResponse, TransportError>>>>,
    tokens: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl QuotaHttp for FakeHttp {
    async fn get_usage(&self, access_token: &str) -> Result<HttpResponse, TransportError> {
        self.tokens
            .lock()
            .expect("tokens lock")
            .push(access_token.to_string());
        self.replies
            .lock()
            .expect("replies lock")
            .pop_front()
            .expect("unexpected HTTP call")
    }
}

fn ok(body: &str) -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status: 200,
        body: body.to_string(),
    })
}

fn status(code: u16) -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status: code,
        body: String::new(),
    })
}

fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 17, hour, minute, second)
        .single()
        .expect("valid timestamp")
}

struct Harness {
    _dir: TempDir,
    home: PathBuf,
    cache_dir: PathBuf,
    now: Arc<Mutex<DateTime<Utc>>>,
    http: FakeHttp,
    fetcher: QuotaFetcher,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().join("home");
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(home.join(".claude")).expect("create .claude");
        write_credentials(&home, SECRET);
        let now = Arc::new(Mutex::new(at(12, 0, 0)));
        let http = FakeHttp::default();
        let fetcher = QuotaFetcher::new(
            home.clone(),
            cache_dir.clone(),
            Box::new(http.clone()),
            Box::new(SharedClock(now.clone())),
        );
        Self {
            _dir: dir,
            home,
            cache_dir,
            now,
            http,
            fetcher,
        }
    }

    fn advance(&self, by: Duration) {
        *self.now.lock().expect("clock lock") += by;
    }

    fn queue(&self, reply: Result<HttpResponse, TransportError>) {
        self.http
            .replies
            .lock()
            .expect("replies lock")
            .push_back(reply);
    }

    fn calls(&self) -> usize {
        self.http.tokens.lock().expect("tokens lock").len()
    }

    fn lock_path(&self) -> PathBuf {
        self.cache_dir.join("claude.quota.lock")
    }
}

fn write_credentials(home: &Path, token: &str) {
    let body = format!(r#"{{"claudeAiOauth":{{"accessToken":"{token}"}}}}"#);
    std::fs::write(home.join(".claude").join(".credentials.json"), body).expect("write creds");
}

// ---- parsing -------------------------------------------------------------

#[tokio::test]
async fn parses_claude_windows_from_fixture() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));

    let windows = h.fetcher.windows("claude").await.expect("windows");

    let five = windows.five_hour.expect("five_hour");
    assert_eq!(five.utilization_pct, 37.5);
    assert_eq!(five.resets_at, at(16, 0, 0));
    let week = windows.seven_day.expect("seven_day");
    assert_eq!(week.utilization_pct, 12.0);
    let sonnet = windows.seven_day_sonnet.expect("seven_day_sonnet");
    assert_eq!(sonnet.utilization_pct, 4.0);
    assert!(!windows.stale);
}

#[tokio::test]
async fn missing_window_is_none_not_zero() {
    let h = Harness::new();
    h.queue(ok(
        r#"{"five_hour":{"utilization":10,"resets_at":"2026-10-17T16:00:00Z"}}"#,
    ));

    let windows = h.fetcher.windows("claude").await.expect("windows");

    assert!(windows.five_hour.is_some());
    assert!(windows.seven_day.is_none());
    assert!(windows.seven_day_sonnet.is_none());
}

#[tokio::test]
async fn window_without_reset_time_is_dropped_not_zero() {
    let h = Harness::new();
    h.queue(ok(
        r#"{"five_hour":{"utilization":10},"seven_day":{"utilization":5,"resets_at":"2026-10-23T09:00:00Z"}}"#,
    ));

    let windows = h.fetcher.windows("claude").await.expect("windows");

    assert!(windows.five_hour.is_none());
    assert_eq!(windows.seven_day.expect("seven_day").utilization_pct, 5.0);
}

#[tokio::test]
async fn response_without_any_window_is_no_data() {
    let h = Harness::new();
    h.queue(ok(r#"{"unrelated":true}"#));

    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(h.calls(), 1);
}

// ---- cache: fresh, stale, locked -----------------------------------------

#[tokio::test]
async fn fresh_cache_is_served_without_second_http_call() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    h.advance(Duration::seconds(30));
    let windows = h.fetcher.windows("claude").await.expect("cached");

    assert!(!windows.stale);
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn expired_ttl_refetches() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    h.advance(Duration::seconds(61));
    h.queue(ok(
        r#"{"five_hour":{"utilization":80,"resets_at":"2026-10-17T16:00:00Z"}}"#,
    ));
    let windows = h.fetcher.windows("claude").await.expect("refetch");

    assert_eq!(h.calls(), 2);
    assert_eq!(windows.five_hour.expect("five_hour").utilization_pct, 80.0);
    assert!(!windows.stale);
}

#[tokio::test]
async fn rate_limit_429_keeps_last_good_snapshot_flagged_stale() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    h.advance(Duration::seconds(61));
    h.queue(status(429));
    let windows = h.fetcher.windows("claude").await.expect("stale snapshot");

    assert!(windows.stale);
    assert_eq!(windows.five_hour.expect("five_hour").utilization_pct, 37.5);
    assert_eq!(h.calls(), 2);
}

#[tokio::test]
async fn rate_limited_provider_is_not_polled_for_five_minutes() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");
    h.advance(Duration::seconds(61));
    h.queue(status(429));
    h.fetcher.windows("claude").await.expect("stale after 429");

    h.advance(Duration::minutes(4));
    let windows = h.fetcher.windows("claude").await.expect("still backed off");
    assert!(windows.stale);
    assert_eq!(h.calls(), 2, "no poll during the 5 minute backoff");

    h.advance(Duration::minutes(2));
    h.queue(ok(FIXTURE));
    let windows = h.fetcher.windows("claude").await.expect("polled again");
    assert!(!windows.stale);
    assert_eq!(h.calls(), 3);
}

#[tokio::test]
async fn rate_limit_without_cache_is_no_data() {
    let h = Harness::new();
    h.queue(status(429));

    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn transport_failure_serves_last_good_snapshot_as_stale() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");

    h.advance(Duration::seconds(61));
    h.queue(Err(TransportError("connection reset".to_string())));
    let windows = h.fetcher.windows("claude").await.expect("stale snapshot");

    assert!(windows.stale);
}

#[tokio::test]
async fn locked_cache_is_read_without_polling() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("first fetch");
    h.advance(Duration::seconds(61));

    // Another process holds the lock and is fetching.
    let _held = match cache::try_lock(&h.lock_path()).expect("lock file") {
        LockOutcome::Acquired(guard) => guard,
        LockOutcome::Busy => panic!("lock should be free in test setup"),
    };
    let windows = h
        .fetcher
        .windows("claude")
        .await
        .expect("cached while locked");

    assert!(windows.stale);
    assert_eq!(h.calls(), 1, "no poll while another process holds the lock");
}

#[tokio::test]
async fn locked_without_cache_is_no_data() {
    let h = Harness::new();
    std::fs::create_dir_all(&h.cache_dir).expect("cache dir");
    let _held = match cache::try_lock(&h.lock_path()).expect("lock file") {
        LockOutcome::Acquired(guard) => guard,
        LockOutcome::Busy => panic!("lock should be free in test setup"),
    };

    assert!(h.fetcher.windows("claude").await.is_none());
    assert_eq!(h.calls(), 0);
}

#[tokio::test]
async fn corrupt_cache_file_is_treated_as_empty() {
    let h = Harness::new();
    std::fs::create_dir_all(&h.cache_dir).expect("cache dir");
    std::fs::write(h.cache_dir.join("claude.quota.json"), "{not json").expect("write junk");
    h.queue(ok(FIXTURE));

    let windows = h.fetcher.windows("claude").await.expect("fetched");

    assert!(!windows.stale);
    assert_eq!(h.calls(), 1);
}

// ---- no source, no credentials, read-only --------------------------------

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

#[test]
fn token_debug_is_redacted() {
    let h = Harness::new();
    let token = credentials::load_claude_access_token(&h.home).expect("token");

    let shown = format!("{token:?}");

    assert!(!shown.contains(SECRET));
    assert_eq!(shown, "AccessToken(<redacted>)");
}

#[tokio::test]
async fn fetcher_debug_never_shows_token() {
    let h = Harness::new();
    h.queue(ok(FIXTURE));
    h.fetcher.windows("claude").await.expect("windows");

    assert!(!format!("{:?}", h.fetcher).contains(SECRET));
}
