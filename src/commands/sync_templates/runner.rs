//! `SyncRunner` — orchestration core for `maestro sync-templates`.
//!
//! Pure orchestration: filesystem access is behind the `SyncFs` trait, so
//! every test injects an in-memory `FakeFs` and observes writes/snapshots
//! without touching disk. The runner re-renders each command for each
//! registered provider, applies the auto-generated banner, hashes the bytes,
//! and either writes (default), compares for drift (`--check`), or reports
//! the plan (`--dry-run`).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::{Path, PathBuf};

use crate::templates::{TemplateError, TemplateProviderRules, render_command_for_rules};

use super::SyncTemplatesArgs;
use super::banner::with_banner;
use super::diff::line_diff;
use super::lockfile::{FileEntry, Lockfile, sha256_hex};
use super::registry::{COMMANDS, PROVIDERS, ProviderEntry, entries_for};

const LOCKFILE_RELPATH: &str = ".maestro/templates.lock";
const TEMPLATES_ROOT: &str = ".maestro/templates";

#[derive(Debug, thiserror::Error)]
pub enum SyncTemplatesError {
    #[error("rendering `{command}` for provider `{provider}`: {source}")]
    Render {
        provider: String,
        command: String,
        #[source]
        source: Box<TemplateError>,
    },
    #[error("unknown provider `{0}` (known: claude, codex, opencode, qwen, ollama, minimax)")]
    UnknownProvider(String),
    #[error("writing `{path}`: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("serializing lockfile: {0}")]
    LockfileSerialize(Box<toml::ser::Error>),
}

pub trait SyncFs: Send + Sync {
    fn write(&self, path: &Path, content: &[u8]) -> std::io::Result<()>;
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>>;
}

pub struct RealFs;

impl SyncFs for RealFs {
    fn write(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Ok(meta) = std::fs::symlink_metadata(path)
            && meta.file_type().is_symlink()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "refusing to overwrite symlink at `{}`; delete the link first",
                    path.display()
                ),
            ));
        }
        std::fs::write(path, content)
    }
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SyncOutcome {
    InSync,
    Wrote(Vec<PathBuf>),
    DryRunPlanned(Vec<PathBuf>),
    DriftDetected {
        paths: Vec<PathBuf>,
        diffs: Vec<String>,
    },
}

impl SyncOutcome {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::DriftDetected { .. } => 1,
            _ => 0,
        }
    }
}

pub struct SyncRunner<'a> {
    repo_root: &'a Path,
    cache_root: &'a Path,
    fs: Box<dyn SyncFs + 'a>,
}

impl<'a> SyncRunner<'a> {
    pub fn new(repo_root: &'a Path, cache_root: &'a Path) -> Self {
        Self {
            repo_root,
            cache_root,
            fs: Box::new(RealFs),
        }
    }

    #[cfg(test)]
    pub fn with_fs(repo_root: &'a Path, cache_root: &'a Path, fs: Box<dyn SyncFs + 'a>) -> Self {
        Self {
            repo_root,
            cache_root,
            fs,
        }
    }

    pub fn run(&self, args: &SyncTemplatesArgs) -> Result<SyncOutcome, SyncTemplatesError> {
        if let Some(filter) = args.provider.as_deref()
            && !PROVIDERS.iter().any(|e| e.id == filter)
        {
            return Err(SyncTemplatesError::UnknownProvider(filter.to_string()));
        }

        let plans = self.build_plans(args.provider.as_deref())?;

        if args.check {
            return self.run_check(&plans);
        }
        if args.dry_run {
            return Ok(SyncOutcome::DryRunPlanned(
                plans
                    .iter()
                    .map(|p| p.target.path().to_path_buf())
                    .collect(),
            ));
        }
        self.run_write(&plans)
    }

    fn build_plans(
        &self,
        provider_filter: Option<&str>,
    ) -> Result<Vec<RenderPlan>, SyncTemplatesError> {
        let mut plans = Vec::new();
        let templates_root = self.repo_root.join(TEMPLATES_ROOT);
        for entry in entries_for(provider_filter) {
            let rules = (entry.rules)();
            if rules.is_null() {
                tracing::warn!(
                    provider = entry.id,
                    "skipping provider: no template_rules registered (NullRules)"
                );
                continue;
            }
            for command in COMMANDS {
                let rendered =
                    render_command_for_rules(&templates_root, rules, command).map_err(|e| {
                        SyncTemplatesError::Render {
                            provider: entry.id.to_string(),
                            command: (*command).to_string(),
                            source: Box::new(e),
                        }
                    })?;
                let content = with_banner(&rendered, command);
                let target = self.resolve_target(entry, rules, command);
                plans.push(RenderPlan {
                    provider_id: entry.id,
                    command,
                    target,
                    content,
                });
            }
        }
        Ok(plans)
    }

    fn resolve_target(
        &self,
        entry: &ProviderEntry,
        rules: &dyn TemplateProviderRules,
        command: &str,
    ) -> RenderTarget {
        match rules.target_dir() {
            Some(dir) => RenderTarget::Repo(self.repo_root.join(dir).join(format!("{command}.md"))),
            None => {
                RenderTarget::Cache(self.cache_root.join(entry.id).join(format!("{command}.md")))
            }
        }
    }

    fn run_check(&self, plans: &[RenderPlan]) -> Result<SyncOutcome, SyncTemplatesError> {
        let mut drift_paths = Vec::new();
        let mut drift_diffs = Vec::new();
        for plan in plans {
            let RenderTarget::Repo(ref path) = plan.target else {
                continue;
            };
            let actual = match self.fs.read(path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    drift_paths.push(path.clone());
                    drift_diffs.push(format!("missing on disk: {}", path.display()));
                    continue;
                }
            };
            if actual != plan.content.as_bytes() {
                let actual_str = String::from_utf8_lossy(&actual).into_owned();
                let diff = line_diff(&actual_str, &plan.content);
                drift_paths.push(path.clone());
                drift_diffs.push(if diff.is_empty() {
                    "bytes differ (no line-level diff — likely trailing-newline or non-UTF-8)"
                        .to_string()
                } else {
                    diff
                });
            }
        }
        if drift_paths.is_empty() {
            Ok(SyncOutcome::InSync)
        } else {
            Ok(SyncOutcome::DriftDetected {
                paths: drift_paths,
                diffs: drift_diffs,
            })
        }
    }

    fn run_write(&self, plans: &[RenderPlan]) -> Result<SyncOutcome, SyncTemplatesError> {
        let mut written = Vec::new();
        let mut lockfile = Lockfile::new();
        for plan in plans {
            let path = plan.target.path();
            self.fs
                .write(path, plan.content.as_bytes())
                .map_err(|source| SyncTemplatesError::Write {
                    path: path.to_path_buf(),
                    source,
                })?;
            written.push(path.to_path_buf());
            if let RenderTarget::Repo(_) = plan.target
                && let Some(rel) = relative_under(path, self.repo_root)
            {
                lockfile.insert(
                    rel,
                    FileEntry {
                        provider: plan.provider_id.to_string(),
                        command: plan.command.to_string(),
                        sha256: sha256_hex(plan.content.as_bytes()),
                    },
                );
            }
        }
        let lockfile_path = self.repo_root.join(LOCKFILE_RELPATH);
        let toml = lockfile.to_toml_string()?;
        self.fs
            .write(&lockfile_path, toml.as_bytes())
            .map_err(|source| SyncTemplatesError::Write {
                path: lockfile_path,
                source,
            })?;
        Ok(SyncOutcome::Wrote(written))
    }
}

enum RenderTarget {
    Repo(PathBuf),
    Cache(PathBuf),
}

impl RenderTarget {
    fn path(&self) -> &Path {
        match self {
            Self::Repo(p) | Self::Cache(p) => p,
        }
    }
}

struct RenderPlan {
    provider_id: &'static str,
    command: &'static str,
    target: RenderTarget,
    content: String,
}

fn relative_under(path: &Path, root: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod exit_code_tests {
    use super::SyncOutcome;

    #[test]
    fn drift_maps_to_one() {
        assert_eq!(
            SyncOutcome::DriftDetected {
                paths: vec![],
                diffs: vec![]
            }
            .exit_code(),
            1
        );
    }

    #[test]
    fn wrote_maps_to_zero() {
        assert_eq!(SyncOutcome::Wrote(vec![]).exit_code(), 0);
    }

    #[test]
    fn in_sync_maps_to_zero() {
        assert_eq!(SyncOutcome::InSync.exit_code(), 0);
    }

    #[test]
    fn dry_run_maps_to_zero() {
        assert_eq!(SyncOutcome::DryRunPlanned(vec![]).exit_code(), 0);
    }
}
