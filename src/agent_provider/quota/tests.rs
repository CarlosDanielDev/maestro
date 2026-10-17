//! Quota module tests. Split by concern: `parsing`, `cache`, `security`.
//!
//! NOTE: `tests/fixtures/claude_oauth_usage.json` is SYNTHESIZED from the shape
//! described in issue #1064 and ai-usagebar. It is NOT a live capture. Endpoint
//! shape, percent scale, and headers are unverified against the live API.
//! (JSON has no comments, so the note lives here.)

mod cache;
mod cadence;
mod failures;
mod parsing;
mod retry;
mod security;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, TimeZone, Utc};
use tempfile::TempDir;

use super::QuotaFetcher;
use super::cache as cache_mod;
use super::credentials;
use super::fetch::{HttpResponse, QuotaHttp};
use super::types::{ProviderWindows, QuotaWindow, TransportError};
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

fn sample_windows() -> ProviderWindows {
    ProviderWindows {
        five_hour: Some(QuotaWindow {
            utilization_pct: 37.5,
            resets_at: at(16, 0, 0),
        }),
        seven_day: None,
        seven_day_sonnet: None,
        stale: false,
    }
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

    /// Moves the clock. A negative duration moves it backwards.
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

    /// Same harness, reporting failures to `sink`. Keeps the same cache dir.
    fn with_sink(self, sink: Box<dyn super::failures::FailureSink>) -> Self {
        let fetcher = QuotaFetcher::new(
            self.home.clone(),
            self.cache_dir.clone(),
            Box::new(self.http.clone()),
            Box::new(SharedClock(self.now.clone())),
        )
        .with_failure_sink(sink);
        Self { fetcher, ..self }
    }

    fn lock_path(&self) -> PathBuf {
        self.cache_dir.join("claude.quota.lock")
    }

    fn cache_path(&self) -> PathBuf {
        self.cache_dir.join("claude.quota.json")
    }

    /// Writes a cache entry directly, bypassing the fetcher.
    fn seed(&self, entry: &cache_mod::CacheEntry) {
        std::fs::create_dir_all(&self.cache_dir).expect("cache dir");
        cache_mod::store(&self.cache_path(), entry).expect("seed cache");
    }
}

fn write_credentials(home: &Path, token: &str) {
    let body = format!(r#"{{"claudeAiOauth":{{"accessToken":"{token}"}}}}"#);
    std::fs::write(home.join(".claude").join(".credentials.json"), body).expect("write creds");
}
