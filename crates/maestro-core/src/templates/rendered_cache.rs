//! Disk lookup for templates rendered by `maestro sync-templates` (#706).
//!
//! HTTP-generic providers (Qwen/Ollama/MiniMax) have no `target_dir` and
//! cache their rendered command bodies under XDG cache. This module exposes
//! a trait so `SessionPool` can read those bodies without coupling to the
//! `directories` crate or to filesystem layout, and so tests can inject a
//! fake without touching disk.
//!
//! ## Trust boundary
//!
//! The XDG cache root is **trusted by construction** — it is owned by the
//! user running `maestro sync-templates`. The rendered template body is
//! appended verbatim to `system_prompt_appendix`, so anything that can
//! influence the rendered output influences every HTTP-provider session
//! prompt. Per `provider_rules/http_generic.rs`, `.claude/skills/<name>/SKILL.md`
//! content is inlined into rendered templates — treat that directory as
//! part of the prompt surface.
//!
//! ## Defense in depth
//!
//! Even though `provider_id` and `command` are constrained at *write* time
//! (`sync-templates` only iterates the static `PROVIDERS` + `COMMANDS`
//! arrays), the runtime lookup path re-validates both inputs via
//! [`crate::util::validation::validate_slug`] and caps the read body size.
//! Ollama/MiniMax `provider.id()` returns the user-configured `[agents.*]`
//! id, which is *not* otherwise slug-validated at config-load time — this
//! is the only place that catches a hostile config (issue #707 security review).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::PathBuf;

/// Hard upper bound on a rendered template body. Templates are bounded
/// in practice (the largest canonical command renders to a few KiB);
/// anything beyond this is either a misconfigured cache or a malicious
/// payload trying to bloat the prompt. Soft-fail with a `tracing::warn!`
/// and no injection rather than reading the whole file into memory.
const MAX_TEMPLATE_BYTES: u64 = 256 * 1024;

/// Lookup interface for cached rendered command templates.
///
/// Implementations may read from disk, from memory, or fail closed. The
/// trait returns `Option<String>` (not `Result<String, Error>`) because
/// every failure mode collapses into "no injection" at the call site —
/// the orchestrator running the agent prompt does not care *why* a cache
/// entry is missing. Missing-file diagnostics are logged via `tracing`
/// inside the implementation.
pub trait RenderedTemplateStore: Send + Sync {
    /// Look up the rendered template body for a (provider_id, command)
    /// pair. Returns `None` when the cache entry is missing, unreadable,
    /// or empty.
    fn lookup(&self, provider_id: &str, command: &str) -> Option<String>;
}

/// Default on-disk implementation rooted at the XDG cache resolved by
/// `directories::ProjectDirs::from("io", "maestro", "maestro")
/// .cache_dir().join("rendered-templates")` — same path written by
/// `maestro sync-templates` (`src/commands/sync_templates/mod.rs`).
pub struct DiskRenderedTemplateStore {
    cache_root: PathBuf,
}

impl DiskRenderedTemplateStore {
    /// Resolve the default XDG cache root. Returns `None` when the
    /// platform's project-dirs lookup fails (no HOME on Unix, etc.) —
    /// the pool falls back to no-injection in that case.
    pub fn from_xdg() -> Option<Self> {
        directories::ProjectDirs::from("io", "maestro", "maestro").map(|p| Self {
            cache_root: p.cache_dir().join("rendered-templates"),
        })
    }

    /// Construct a store rooted at an explicit cache directory. Used by
    /// integration tests with a `tempfile::TempDir`.
    pub fn new(cache_root: PathBuf) -> Self {
        Self { cache_root }
    }
}

impl RenderedTemplateStore for DiskRenderedTemplateStore {
    fn lookup(&self, provider_id: &str, command: &str) -> Option<String> {
        // Defense in depth: reject `..`, `/`, `\`, NUL, etc., even though
        // both inputs come from constrained sources today. Issue #707
        // security review noted Ollama/MiniMax `provider.id()` is
        // user-configurable via `[agents.*]` and is not slug-validated at
        // config load time.
        if crate::util::validation::validate_slug(provider_id).is_err() {
            tracing::warn!(
                provider = provider_id,
                "rejected provider id (not a valid slug)"
            );
            return None;
        }
        if crate::util::validation::validate_slug(command).is_err() {
            tracing::warn!(
                command = command,
                "rejected command name (not a valid slug)"
            );
            return None;
        }

        let path = self
            .cache_root
            .join(provider_id)
            .join(format!("{command}.md"));

        // Size cap: stat before reading. Soft-fail on oversize so the
        // session still spawns without injection.
        match std::fs::metadata(&path) {
            Ok(m) if m.len() > MAX_TEMPLATE_BYTES => {
                tracing::warn!(
                    provider = provider_id,
                    command = command,
                    bytes = m.len(),
                    "rendered-template cache entry exceeds size cap"
                );
                return None;
            }
            Ok(_) => {}
            Err(_) => { /* fall through to read_to_string for unified error handling */ }
        }

        match std::fs::read_to_string(&path) {
            Ok(body) if !body.trim().is_empty() => Some(body),
            Ok(_) => {
                tracing::warn!(
                    provider = provider_id,
                    command = command,
                    path = %path.display(),
                    "rendered-template cache entry is empty"
                );
                None
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!(
                    provider = provider_id,
                    command = command,
                    path = %path.display(),
                    "rendered-template cache miss — run `maestro sync-templates` to populate"
                );
                None
            }
            Err(e) => {
                tracing::warn!(
                    provider = provider_id,
                    command = command,
                    path = %path.display(),
                    error = %e,
                    "rendered-template cache read failed"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// In-memory fake for tests that want to drive the pool's lookup
    /// gate without touching disk. Public(crate) so unit tests in
    /// `pool.rs` and integration tests in `integration_tests/` can
    /// share it.
    pub(crate) struct FakeRenderedStore {
        entries: HashMap<(String, String), String>,
    }

    impl FakeRenderedStore {
        pub fn new() -> Self {
            Self {
                entries: HashMap::new(),
            }
        }

        pub fn with(mut self, provider: &str, command: &str, body: &str) -> Self {
            self.entries.insert(
                (provider.to_string(), command.to_string()),
                body.to_string(),
            );
            self
        }
    }

    impl RenderedTemplateStore for FakeRenderedStore {
        fn lookup(&self, provider: &str, command: &str) -> Option<String> {
            self.entries
                .get(&(provider.to_string(), command.to_string()))
                .cloned()
        }
    }

    #[test]
    fn fake_rendered_store_lookup_returns_body_for_known_key() {
        let store = FakeRenderedStore::new().with("qwen", "implement", "# Implement\n\ndo stuff");
        let result = store.lookup("qwen", "implement");
        assert_eq!(result.as_deref(), Some("# Implement\n\ndo stuff"));
    }

    #[test]
    fn fake_rendered_store_lookup_returns_none_for_unknown_provider() {
        let store = FakeRenderedStore::new().with("qwen", "implement", "body");
        assert!(store.lookup("ollama", "implement").is_none());
    }

    #[test]
    fn fake_rendered_store_lookup_returns_none_for_unknown_command() {
        let store = FakeRenderedStore::new().with("qwen", "implement", "body");
        assert!(store.lookup("qwen", "plan-feature").is_none());
    }

    #[test]
    fn disk_store_lookup_returns_body_for_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("qwen")).unwrap();
        std::fs::write(dir.path().join("qwen").join("implement.md"), "# body").unwrap();
        let store = DiskRenderedTemplateStore::new(dir.path().to_path_buf());
        let result = store.lookup("qwen", "implement");
        assert_eq!(result.as_deref(), Some("# body"));
    }

    #[test]
    fn disk_store_lookup_returns_none_for_missing_provider_dir() {
        let dir = tempfile::tempdir().unwrap();
        let store = DiskRenderedTemplateStore::new(dir.path().to_path_buf());
        assert!(store.lookup("qwen", "implement").is_none());
    }

    #[test]
    fn disk_store_lookup_returns_none_for_missing_command_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("qwen")).unwrap();
        let store = DiskRenderedTemplateStore::new(dir.path().to_path_buf());
        assert!(store.lookup("qwen", "implement").is_none());
    }

    #[test]
    fn disk_store_lookup_returns_none_for_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("qwen")).unwrap();
        std::fs::write(dir.path().join("qwen").join("implement.md"), "").unwrap();
        let store = DiskRenderedTemplateStore::new(dir.path().to_path_buf());
        assert!(store.lookup("qwen", "implement").is_none());
    }

    #[test]
    fn disk_store_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DiskRenderedTemplateStore>();
    }

    #[test]
    fn disk_store_rejects_path_traversal_in_provider_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = DiskRenderedTemplateStore::new(dir.path().to_path_buf());
        assert!(store.lookup("..", "implement").is_none());
        assert!(store.lookup("../etc", "implement").is_none());
        assert!(store.lookup("foo/bar", "implement").is_none());
    }

    #[test]
    fn disk_store_rejects_path_traversal_in_command() {
        let dir = tempfile::tempdir().unwrap();
        let store = DiskRenderedTemplateStore::new(dir.path().to_path_buf());
        assert!(store.lookup("qwen", "..").is_none());
        assert!(store.lookup("qwen", "../etc/passwd").is_none());
        assert!(store.lookup("qwen", "foo/bar").is_none());
    }

    #[test]
    fn disk_store_rejects_oversized_template() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("qwen")).unwrap();
        let big = vec![b'x'; (MAX_TEMPLATE_BYTES as usize) + 1];
        std::fs::write(dir.path().join("qwen").join("implement.md"), &big).unwrap();
        let store = DiskRenderedTemplateStore::new(dir.path().to_path_buf());
        assert!(store.lookup("qwen", "implement").is_none());
    }

    #[test]
    fn rendered_template_store_trait_is_object_safe() {
        let _: Box<dyn RenderedTemplateStore> = Box::new(FakeRenderedStore::new());
    }
}

#[cfg(test)]
pub(crate) use tests::FakeRenderedStore;
