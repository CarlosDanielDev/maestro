use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

use crate::init::{
    FsProjectDetector, ProjectDetector, RenderOutcome, render_or_merge, walk::find_project_root,
};

/// Public entry point used by the CLI. Forwards to [`cmd_init_inner`]
/// against the real filesystem and converts the logical exit code into
/// either `Ok(())` (success) or a process-exit (failure).
pub fn cmd_init(reset: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("reading current working directory")?;
    let root = find_project_root(&cwd);
    let detector = FsProjectDetector::new();
    let code = cmd_init_inner(reset, &root, &detector)?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

/// Pure orchestration helper: writes (or merges) `maestro.toml` and
/// returns the logical exit code. Tests drive this directly with a
/// `FakeProjectDetector` and a `tempfile::TempDir`.
///
/// Exit codes:
/// - 0 — wrote/merged successfully
/// - 2 — `maestro.toml` already exists and `reset` is `false`
pub fn cmd_init_inner(
    reset: bool,
    project_root: &Path,
    detector: &dyn ProjectDetector,
) -> Result<i32> {
    let target = project_root.join("maestro.toml");

    if target.exists() && !reset {
        eprintln!(
            "maestro.toml already exists at {}. Use --reset to refresh detection.",
            target.display()
        );
        return Ok(2);
    }

    let existing = if reset && target.exists() {
        Some(
            std::fs::read_to_string(&target)
                .with_context(|| format!("reading existing {}", target.display()))?,
        )
    } else {
        None
    };

    let outcome = render_or_merge(detector, project_root, existing.as_deref())?;

    match outcome {
        RenderOutcome::Fresh { stacks, content } => {
            // create_new: atomically reject if the file appeared after
            // our pre-check, instead of silently clobbering.
            match std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
            {
                Ok(mut f) => f
                    .write_all(content.as_bytes())
                    .with_context(|| format!("writing {}", target.display()))?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    eprintln!(
                        "maestro.toml already exists at {}. Use --reset to refresh detection.",
                        target.display()
                    );
                    return Ok(2);
                }
                Err(e) => {
                    return Err(
                        anyhow::Error::from(e).context(format!("writing {}", target.display()))
                    );
                }
            }
            if stacks.is_empty() {
                eprintln!(
                    "Warning: no project markers detected. Wrote a generic template at {}; \
                     fill in build/test/run commands manually.",
                    target.display()
                );
            } else {
                let names: Vec<&str> = stacks.iter().map(|s| s.id()).collect();
                println!(
                    "Detected: {}. Created {}",
                    names.join(", "),
                    target.display()
                );
            }
        }
        RenderOutcome::Merged { stacks, report } => {
            std::fs::write(&target, &report.merged_toml)
                .with_context(|| format!("writing {}", target.display()))?;
            let names: Vec<&str> = if stacks.is_empty() {
                vec!["none"]
            } else {
                stacks.iter().map(|s| s.id()).collect()
            };
            println!(
                "Reset complete: detected {}, added {} key(s), preserved {} customized key(s).",
                names.join(", "),
                report.keys_added.len(),
                report.keys_preserved.len()
            );
        }
    }

    Ok(0)
}
