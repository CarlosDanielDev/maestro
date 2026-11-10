//! Workspace trust gate (#1045).
//!
//! A repo-local `maestro.toml` can name executables (`agents.*.command`,
//! `plugins[].run`, `sessions.completion_gates.*`, `gates.test_command`,
//! `review.command` / `review.reviewers[].command`). Maestro must not run
//! any of them until the user consents — and the consent record must live
//! OUTSIDE the repo, so the config can never grant trust to itself. This
//! module stores consent in the user's maestro config dir (`trusted.toml`),
//! keyed by the canonicalized config path plus the SHA-256 of the file
//! content (the direnv `allow` model: re-prompt when the file changes).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Name of the trust record file inside the maestro config dir.
pub const TRUST_FILE_NAME: &str = "trusted.toml";

/// Hex-encoded SHA-256 of a config file's contents.
pub fn content_hash(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// True for the built-in default binary names (`claude`, `codex`, `qwen`,
/// `opencode`). A config that names its kind's standard binary adds nothing
/// an attacker can use — resolution goes through the user's own PATH — so
/// these values are exempt from the trust gate.
pub fn is_builtin_agent_binary(command: &str) -> bool {
    matches!(command.trim(), "claude" | "codex" | "qwen" | "opencode")
}

/// One executable-bearing value found in a repo config, kept verbatim so
/// the consent prompt can show the user exactly what would run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableFinding {
    /// Dotted config key, e.g. `agents.claude.command`.
    pub key: String,
    /// The declared value, unmodified.
    pub value: String,
}

/// Collect every executable-bearing value declared by `config`. Values that
/// name the built-in default binary for their agent kind are skipped (see
/// [`is_builtin_agent_binary`]).
pub fn inventory(config: &crate::config::Config) -> Vec<ExecutableFinding> {
    let mut out = Vec::new();
    for (id, agent) in &config.agents.entries {
        if let Some(command) = agent.command.as_deref()
            && !command.trim().is_empty()
            && !(agent.kind.is_subprocess() && is_builtin_agent_binary(command))
        {
            out.push(ExecutableFinding {
                key: format!("agents.{id}.command"),
                value: command.to_string(),
            });
        }
        if !agent.env.is_empty() {
            let joined = agent
                .env
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push(ExecutableFinding {
                key: format!("agents.{id}.env"),
                value: joined,
            });
        }
        if !agent.extra_args.is_empty() {
            out.push(ExecutableFinding {
                key: format!("agents.{id}.extra_args"),
                value: agent.extra_args.join(" "),
            });
        }
    }
    for plugin in &config.plugins {
        out.push(ExecutableFinding {
            key: format!("plugins.{}.run", plugin.name),
            value: plugin.run.clone(),
        });
    }
    for gate in &config.sessions.completion_gates.commands {
        out.push(ExecutableFinding {
            key: format!("sessions.completion_gates.{}", gate.name),
            value: gate.run.clone(),
        });
    }
    if config.gates.test_command != crate::config::GatesConfig::default().test_command {
        out.push(ExecutableFinding {
            key: "gates.test_command".to_string(),
            value: config.gates.test_command.clone(),
        });
    }
    if config.review.command != crate::config::ReviewConfig::default().command {
        out.push(ExecutableFinding {
            key: "review.command".to_string(),
            value: config.review.command.clone(),
        });
    }
    for reviewer in &config.review.reviewers {
        out.push(ExecutableFinding {
            key: format!("review.reviewers.{}.command", reviewer.name),
            value: reviewer.command.clone(),
        });
    }
    out
}

/// True when the config declares at least one executable-bearing value.
pub fn has_executable_keys(config: &crate::config::Config) -> bool {
    !inventory(config).is_empty()
}

/// True when any enabled agent overrides its binary with a repo-chosen
/// command (i.e. session spawn would execute a program the config picked).
/// Built-in default binary names are exempt (see
/// [`is_builtin_agent_binary`]).
pub fn has_config_supplied_commands(config: &crate::config::Config) -> bool {
    config.agents.entries.values().any(|agent| {
        agent.enabled
            && agent.command.as_deref().is_some_and(|c| {
                !(c.trim().is_empty() || agent.kind.is_subprocess() && is_builtin_agent_binary(c))
            })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustDecision {
    Trusted,
    Untrusted,
}

// ---------------------------------------------------------------------------
// On-disk trust store
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrustFile {
    #[serde(default)]
    trusted: Vec<TrustRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrustRecord {
    path: String,
    hash: String,
    trusted_at: i64,
}

/// Consent records for repo configs, persisted in the user's maestro
/// config dir — never inside the repo being evaluated.
pub struct TrustStore {
    path: PathBuf,
}

impl TrustStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// Store at the platform config dir (e.g. `~/.config/maestro` on Linux,
    /// `~/Library/Application Support/io.maestro.maestro` on macOS).
    pub fn with_default_path() -> Result<Self> {
        Ok(Self::at(default_path()?))
    }

    pub fn is_trusted(&self, config_path: &Path, hash: &str) -> bool {
        let Some(key) = canonical_key(config_path) else {
            return false;
        };
        self.load()
            .trusted
            .iter()
            .any(|r| r.path == key && r.hash == hash)
    }

    /// Record consent for `config_path` at `hash`. Read-modify-write with an
    /// atomic replace so concurrent maestro processes cannot corrupt the file.
    pub fn record(&self, config_path: &Path, hash: &str) -> Result<()> {
        let key = canonical_key(config_path)
            .context("canonicalizing config path for the trust record")?;
        let mut file = self.load();
        if file.trusted.iter().any(|r| r.path == key && r.hash == hash) {
            return Ok(());
        }
        file.trusted.retain(|r| r.path != key);
        file.trusted.push(TrustRecord {
            path: key,
            hash: hash.to_string(),
            trusted_at: unix_now(),
        });
        self.save(&file)
    }

    /// Remove any consent for `config_path`. Returns true when a record was
    /// removed.
    pub fn revoke(&self, config_path: &Path) -> Result<bool> {
        let Some(key) = canonical_key(config_path) else {
            return Ok(false);
        };
        let mut file = self.load();
        let before = file.trusted.len();
        file.trusted.retain(|r| r.path != key);
        let removed = file.trusted.len() != before;
        if removed {
            self.save(&file)?;
        }
        Ok(removed)
    }

    fn load(&self) -> TrustFile {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, file: &TrustFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(file).context("serializing trust records")?;
        crate::config::io::atomic_write(&self.path, &content)
            .with_context(|| format!("writing {}", self.path.display()))?;
        Ok(())
    }
}

/// Path of the trust record file in the user's maestro config dir.
pub fn default_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("io", "maestro", "maestro")
        .context("cannot resolve the maestro config directory")?;
    Ok(dirs.config_dir().join(TRUST_FILE_NAME))
}

/// Store key for a config path: canonicalized when the file exists (the
/// production case), literal path string otherwise, so trust records never
/// depend on the current directory.
fn canonical_key(config_path: &Path) -> Option<String> {
    match std::fs::canonicalize(config_path) {
        Ok(canonical) => Some(canonical.to_string_lossy().into_owned()),
        Err(_) => {
            let literal = config_path.to_string_lossy().into_owned();
            if literal.is_empty() {
                None
            } else {
                Some(literal)
            }
        }
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Decisions
// ---------------------------------------------------------------------------

/// Consult `store` for `config` loaded from `config_path`. Configs without
/// executable-bearing keys are always trusted. When the config did not come
/// from an existing file (synthetic configs in tests), there is no
/// repo-supplied value on disk to distrust and the decision is trusted;
/// a file that exists but cannot be read fails closed as untrusted.
pub fn evaluate(
    config_path: &Path,
    config: &crate::config::Config,
    store: &TrustStore,
) -> TrustDecision {
    if !has_executable_keys(config) {
        return TrustDecision::Trusted;
    }
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return TrustDecision::Trusted;
    };
    if store.is_trusted(config_path, &content_hash(&content)) {
        TrustDecision::Trusted
    } else {
        TrustDecision::Untrusted
    }
}

/// [`evaluate`] against the user's default trust store. A store that cannot
/// be resolved (no config dir) fails closed as untrusted.
pub fn evaluate_with_default_store(
    config_path: &Path,
    config: &crate::config::Config,
) -> TrustDecision {
    match TrustStore::with_default_path() {
        Ok(store) => evaluate(config_path, config, &store),
        Err(_) => match has_executable_keys(config) {
            true => TrustDecision::Untrusted,
            false => TrustDecision::Trusted,
        },
    }
}

/// Evaluate trust for the config the current directory resolves to.
/// Used by surfaces that only receive `&Config` (doctor).
pub fn evaluate_current_dir(config: &crate::config::Config) -> TrustDecision {
    let path = Path::new("maestro.toml");
    if path.exists() {
        evaluate_with_default_store(path, config)
    } else {
        evaluate_with_default_store(Path::new(".maestro/config.toml"), config)
    }
}

#[path = "trust_consent.rs"]
mod trust_consent;

pub use trust_consent::{
    ensure_interactive_consent_with_default_store, refuse_untrusted_with_default_store,
};

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
