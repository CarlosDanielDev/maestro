//! Startup migration for v0.25.1 default-config fields.
//!
//! Scope is intentionally limited to single-step key injection for fields
//! introduced in v0.25.1. A versioned multi-step migration framework is
//! explicitly out of scope (tracked separately).
//!
//! The planner (`plan_v0_25_1_migration`) is pure; the driver
//! (`run_startup_migration`) handles I/O.

use anyhow::{Context, Result};
use std::path::Path;
use toml::Value;

/// Maximum size of `maestro.toml` the migrator will consider. Anything larger
/// is almost certainly not a config; the cap prevents memory exhaustion on
/// hostile or accidental inputs (e.g. `cp /dev/zero maestro.toml`).
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Outcome of inspecting an on-disk TOML for v0.25.1 default-field migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MigrationOutcome {
    /// File parsed cleanly; no missing keys; nothing to write.
    AlreadyCurrent,
    /// File parsed cleanly; one or more keys filled with defaults.
    Migrated {
        new_toml: String,
        added_keys: Vec<String>,
    },
}

/// Pure planner. Never touches the filesystem.
///
/// Surfaces parse errors (caller decides whether to skip or propagate). The
/// driver swallows them so that a malformed `maestro.toml` is never rewritten.
pub(crate) fn plan_v0_25_1_migration(existing_toml: &str) -> Result<MigrationOutcome> {
    let mut value: Value = toml::from_str(existing_toml).context("parsing maestro.toml")?;
    let Some(root) = value.as_table_mut() else {
        anyhow::bail!("maestro.toml root must be a table");
    };

    let mut added_keys: Vec<String> = Vec::new();

    let views_entry = root
        .entry("views".to_string())
        .or_insert_with(|| Value::Table(Default::default()));
    let Some(views) = views_entry.as_table_mut() else {
        anyhow::bail!("[views] must be a table");
    };

    if !views.contains_key("agent_graph_enabled") {
        views.insert("agent_graph_enabled".to_string(), Value::Boolean(true));
        added_keys.push("views.agent_graph_enabled".to_string());
    }

    if added_keys.is_empty() {
        return Ok(MigrationOutcome::AlreadyCurrent);
    }

    let new_toml = toml::to_string_pretty(&value).context("serializing migrated TOML")?;
    Ok(MigrationOutcome::Migrated {
        new_toml,
        added_keys,
    })
}

/// Driver. Reads `path`, decides whether to migrate, and writes the result.
///
/// Every failure path is logged and swallowed so startup is never blocked by
/// migration troubles.
pub fn run_startup_migration(path: &Path) {
    run_startup_migration_with_writer(path, &mut std::io::stderr().lock());
}

/// Writer-injection variant for tests. Production callers go through
/// `run_startup_migration`.
pub(crate) fn run_startup_migration_with_writer(path: &Path, stderr: &mut dyn std::io::Write) {
    // Stat first via symlink_metadata so we can refuse symlinks, non-regular
    // files (FIFOs, sockets, devices), and oversized files without ever
    // opening them. Defense-in-depth against hostile CWDs.
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Fresh install: no file → no migration.
            return;
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "skipping startup migration: could not stat maestro.toml"
            );
            return;
        }
    };
    if metadata.file_type().is_symlink() {
        tracing::warn!(
            path = %path.display(),
            "skipping startup migration: maestro.toml is a symlink"
        );
        return;
    }
    if !metadata.is_file() {
        tracing::warn!(
            path = %path.display(),
            "skipping startup migration: maestro.toml is not a regular file"
        );
        return;
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        tracing::warn!(
            path = %path.display(),
            size = metadata.len(),
            cap = MAX_CONFIG_BYTES,
            "skipping startup migration: maestro.toml exceeds size cap"
        );
        return;
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            // Race: the file disappeared / lost read permission between stat
            // and open. Log + continue.
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "skipping startup migration: could not read maestro.toml"
            );
            return;
        }
    };

    let plan = match plan_v0_25_1_migration(&content) {
        Ok(p) => p,
        Err(e) => {
            // Malformed TOML: the existing parse error will surface via the
            // normal Config::load path. Migrator must NOT touch the file.
            // Use Display (not Debug) on the error to avoid echoing attacker-
            // controlled bytes via parse-error snippets.
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "skipping startup migration: maestro.toml did not parse"
            );
            return;
        }
    };

    let (new_toml, added_keys) = match plan {
        MigrationOutcome::AlreadyCurrent => return,
        MigrationOutcome::Migrated {
            new_toml,
            added_keys,
        } => (new_toml, added_keys),
    };

    match atomic_write(path, &new_toml) {
        Ok(()) => {
            for key in &added_keys {
                // Notice format is asserted by driver_writes_key_and_emits_notice_when_key_absent.
                let _ = writeln!(stderr, "[maestro] config migrated: added {key} = true");
            }
        }
        Err(e) => {
            // In-memory default applies on the next Config::load.
            let _ = writeln!(
                stderr,
                "[maestro] warning: could not write migrated maestro.toml: {e}"
            );
        }
    }
}

/// Write `content` to `path` atomically. Uses `tempfile::NamedTempFile` to
/// create a unique-name temp file with `O_EXCL` semantics in the same
/// directory, then `persist` (rename) it over the destination. This closes
/// the TOCTOU window that a deterministic `<path>.tmp` filename would leave
/// open on shared filesystems.
fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod tests;
