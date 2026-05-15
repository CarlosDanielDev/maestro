//! Unit tests for `SyncRunner`. Split from `runner.rs` to stay under the
//! 400-line file-size gate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::SyncTemplatesArgs;
use super::runner::{SyncFs, SyncOutcome, SyncRunner, SyncTemplatesError};

const LOCKFILE_RELPATH: &str = ".maestro/templates.lock";

#[derive(Default, Clone)]
struct FakeFs {
    inner: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    write_calls: Arc<AtomicUsize>,
}

impl FakeFs {
    fn snapshot(&self) -> HashMap<PathBuf, Vec<u8>> {
        self.inner.lock().expect("FakeFs lock not poisoned").clone()
    }

    fn write_seed(&self, path: &Path, content: &[u8]) {
        self.inner
            .lock()
            .expect("FakeFs lock not poisoned")
            .insert(path.to_path_buf(), content.to_vec());
    }

    fn write_count(&self) -> usize {
        self.write_calls.load(Ordering::Relaxed)
    }
}

impl SyncFs for FakeFs {
    fn write(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
        self.write_calls.fetch_add(1, Ordering::Relaxed);
        self.inner
            .lock()
            .expect("FakeFs lock not poisoned")
            .insert(path.to_path_buf(), content.to_vec());
        Ok(())
    }
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.inner
            .lock()
            .expect("FakeFs lock not poisoned")
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cache_root() -> PathBuf {
    PathBuf::from("/fake/cache")
}

fn make_runner<'a>(repo: &'a Path, cache: &'a Path, fs: FakeFs) -> SyncRunner<'a> {
    SyncRunner::with_fs(repo, cache, Box::new(fs))
}

fn default_args() -> SyncTemplatesArgs {
    SyncTemplatesArgs {
        provider: None,
        check: false,
        dry_run: false,
    }
}

fn claude_only() -> SyncTemplatesArgs {
    SyncTemplatesArgs {
        provider: Some("claude".to_string()),
        check: false,
        dry_run: false,
    }
}

#[test]
fn run_default_writes_lockfile_byte_identical_across_two_invocations() {
    let repo = repo_root();
    let cache = cache_root();
    let fs1 = FakeFs::default();
    let fs2 = FakeFs::default();
    let _ = make_runner(&repo, &cache, fs1.clone())
        .run(&default_args())
        .expect("first run");
    let _ = make_runner(&repo, &cache, fs2.clone())
        .run(&default_args())
        .expect("second run");
    let lockfile_path = repo.join(LOCKFILE_RELPATH);
    let lf1 = fs1.snapshot();
    let lf2 = fs2.snapshot();
    let b1 = lf1.get(&lockfile_path).expect("lockfile in run 1");
    let b2 = lf2.get(&lockfile_path).expect("lockfile in run 2");
    assert_eq!(b1, b2, "lockfile bytes must be identical across runs");
}

#[test]
fn run_with_provider_filter_renders_only_named_provider() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let _ = make_runner(&repo, &cache, fs.clone())
        .run(&claude_only())
        .expect("run");
    let snap = fs.snapshot();
    for path in snap.keys() {
        assert!(
            path.starts_with(&repo),
            "with --provider claude, writes must be under repo_root: {path:?}"
        );
    }
    let cache_writes: Vec<_> = snap.keys().filter(|p| p.starts_with(&cache)).collect();
    assert!(
        cache_writes.is_empty(),
        "cache unexpected: {cache_writes:?}"
    );
}

#[test]
fn run_claude_only_writes_four_commands_plus_lockfile() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let outcome = make_runner(&repo, &cache, fs.clone())
        .run(&claude_only())
        .expect("run");
    let snap = fs.snapshot();
    let repo_writes: Vec<_> = snap.keys().filter(|p| p.starts_with(&repo)).collect();
    assert_eq!(
        repo_writes.len(),
        5,
        "4 claude commands + lockfile: {repo_writes:?}"
    );
    let lockfile_path = repo.join(LOCKFILE_RELPATH);
    assert!(snap.contains_key(&lockfile_path));
    assert!(matches!(outcome, SyncOutcome::Wrote(_)));
    if let SyncOutcome::Wrote(paths) = outcome {
        assert_eq!(paths.len(), 4, "Wrote reports 4 command paths");
    }
}

#[test]
fn run_with_check_returns_in_sync_when_baselines_match_render() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let _ = make_runner(&repo, &cache, fs.clone())
        .run(&claude_only())
        .expect("write run");
    let args = SyncTemplatesArgs {
        provider: Some("claude".into()),
        check: true,
        dry_run: false,
    };
    let outcome = make_runner(&repo, &cache, fs.clone())
        .run(&args)
        .expect("check");
    assert!(matches!(outcome, SyncOutcome::InSync), "got: {outcome:?}");
}

#[test]
fn run_with_check_returns_drift_when_baseline_is_modified() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    for cmd in ["implement", "pushup", "plan-feature", "simplify"] {
        let p = repo.join(format!(".claude/commands/{cmd}.md"));
        fs.write_seed(&p, b"CORRUPTED");
    }
    let args = SyncTemplatesArgs {
        provider: Some("claude".into()),
        check: true,
        dry_run: false,
    };
    let outcome = make_runner(&repo, &cache, fs.clone())
        .run(&args)
        .expect("check");
    match outcome {
        SyncOutcome::DriftDetected { paths, diffs } => {
            assert!(!paths.is_empty(), "drift paths must be non-empty");
            assert!(!diffs.is_empty(), "drift diffs must be non-empty");
        }
        other => panic!("expected DriftDetected, got: {other:?}"),
    }
}

#[test]
fn run_with_dry_run_makes_no_writes() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let args = SyncTemplatesArgs {
        provider: None,
        check: false,
        dry_run: true,
    };
    let outcome = make_runner(&repo, &cache, fs.clone())
        .run(&args)
        .expect("dry run");
    assert_eq!(fs.write_count(), 0, "dry_run must make zero writes");
    assert!(
        matches!(outcome, SyncOutcome::DryRunPlanned(ref paths) if !paths.is_empty()),
        "got: {outcome:?}"
    );
}

#[test]
fn run_with_unknown_provider_returns_unknown_provider_error() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let args = SyncTemplatesArgs {
        provider: Some("fictional".into()),
        check: false,
        dry_run: false,
    };
    let result = make_runner(&repo, &cache, fs).run(&args);
    assert!(
        matches!(result, Err(SyncTemplatesError::UnknownProvider(ref id)) if id == "fictional"),
        "expected UnknownProvider, got: {result:?}"
    );
}

#[test]
fn run_skips_provider_whose_template_rules_are_null() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let args = SyncTemplatesArgs {
        provider: Some("opencode".into()),
        check: false,
        dry_run: false,
    };
    let outcome = make_runner(&repo, &cache, fs.clone())
        .run(&args)
        .expect("opencode run must not error");
    let snap = fs.snapshot();
    let lockfile_path = repo.join(LOCKFILE_RELPATH);
    assert_eq!(snap.len(), 1);
    assert!(snap.contains_key(&lockfile_path));
    assert!(matches!(outcome, SyncOutcome::Wrote(ref paths) if paths.is_empty()));
}

#[test]
fn run_writes_banner_at_top_of_each_rendered_file() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let _ = make_runner(&repo, &cache, fs.clone())
        .run(&claude_only())
        .expect("run");
    let snap = fs.snapshot();
    let command_files: Vec<_> = snap
        .iter()
        .filter(|(p, _)| {
            p.extension().and_then(|e| e.to_str()) == Some("md")
                && p.starts_with(repo.join(".claude/commands"))
        })
        .collect();
    assert!(!command_files.is_empty());
    for (path, bytes) in &command_files {
        let content = std::str::from_utf8(bytes).expect("written file must be valid UTF-8");
        assert!(
            content.starts_with("<!-- AUTO-GENERATED by maestro sync-templates from"),
            "file {path:?} missing banner; first 120 chars: {:.120}",
            content
        );
    }
}

#[test]
fn run_writes_http_provider_output_to_cache_root() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let args = SyncTemplatesArgs {
        provider: Some("qwen".into()),
        check: false,
        dry_run: false,
    };
    let _ = make_runner(&repo, &cache, fs.clone())
        .run(&args)
        .expect("run");
    let snap = fs.snapshot();
    for cmd in ["implement", "pushup", "plan-feature", "simplify"] {
        let expected = cache.join("qwen").join(format!("{cmd}.md"));
        assert!(
            snap.contains_key(&expected),
            "missing cache file: {expected:?}"
        );
    }
    let claude_dir = repo.join(".claude/commands");
    for path in snap.keys() {
        if path.starts_with(&claude_dir) {
            panic!("qwen render leaked into repo claude path: {path:?}");
        }
    }
}

#[test]
fn run_lockfile_entries_use_repo_relative_paths_not_absolute() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let _ = make_runner(&repo, &cache, fs.clone())
        .run(&claude_only())
        .expect("run");
    let snap = fs.snapshot();
    let lockfile_path = repo.join(LOCKFILE_RELPATH);
    let lockfile_str =
        std::str::from_utf8(snap.get(&lockfile_path).expect("lockfile")).expect("utf8");
    let repo_abs = repo.to_string_lossy().into_owned();
    assert!(
        !lockfile_str.contains(repo_abs.as_str()),
        "lockfile must use repo-relative paths, not absolute"
    );
    assert!(
        lockfile_str.contains(".claude/commands/"),
        "lockfile must contain relative .claude/commands/ paths"
    );
}

#[test]
fn run_lockfile_does_not_contain_cache_paths() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let _ = make_runner(&repo, &cache, fs.clone())
        .run(&default_args())
        .expect("run");
    let snap = fs.snapshot();
    let lockfile_path = repo.join(LOCKFILE_RELPATH);
    let lockfile_str =
        std::str::from_utf8(snap.get(&lockfile_path).expect("lockfile")).expect("utf8");
    for cache_id in ["qwen", "ollama", "minimax"] {
        assert!(
            !lockfile_str.contains(cache_id),
            "cache-only provider `{cache_id}` must not appear in lockfile"
        );
    }
}

#[test]
fn run_check_does_not_write_any_files() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let args = SyncTemplatesArgs {
        provider: None,
        check: true,
        dry_run: false,
    };
    let _ = make_runner(&repo, &cache, fs.clone())
        .run(&args)
        .expect("check run");
    assert_eq!(fs.write_count(), 0, "--check must not write any files");
}

#[test]
fn run_check_does_not_write_lockfile() {
    let repo = repo_root();
    let cache = cache_root();
    let fs = FakeFs::default();
    let args = SyncTemplatesArgs {
        provider: None,
        check: true,
        dry_run: false,
    };
    let _ = make_runner(&repo, &cache, fs.clone())
        .run(&args)
        .expect("check run");
    let snap = fs.snapshot();
    let lockfile_path = repo.join(LOCKFILE_RELPATH);
    assert!(
        !snap.contains_key(&lockfile_path),
        "--check must not write the lockfile"
    );
}
