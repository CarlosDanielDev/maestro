//! `maestro sync-templates` subcommand.
//!
//! Renders every canonical command under `.maestro/templates/commands/` for
//! every registered provider, writes outputs to the provider's `target_dir()`
//! (for repo-discovered providers like Claude) or to a per-provider cache
//! directory (for HTTP-only providers like Qwen/Ollama/MiniMax), and records
//! SHA-256 checksums of the *repo-relative* writes in `.maestro/templates.lock`
//! for CI drift detection.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

pub mod banner;
pub mod diff;
pub mod lockfile;
pub mod registry;
pub mod runner;
#[cfg(test)]
mod runner_tests;

use anyhow::{Context, Result};
use std::path::PathBuf;

pub use runner::{SyncOutcome, SyncRunner, SyncTemplatesError};

pub struct SyncTemplatesArgs {
    pub provider: Option<String>,
    pub check: bool,
    pub dry_run: bool,
}

/// CLI entry point. Exits the process with code 1 on drift.
pub fn cmd_sync_templates(args: SyncTemplatesArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("reading current working directory")?;
    let cache_root = default_cache_root()
        .context("resolving XDG cache directory (set HOME or run from a configured environment)")?;
    let runner = SyncRunner::new(&cwd, &cache_root);
    let outcome = runner.run(&args)?;
    print_outcome(&outcome);
    if outcome.exit_code() != 0 {
        std::process::exit(outcome.exit_code());
    }
    Ok(())
}

fn default_cache_root() -> Result<PathBuf> {
    directories::ProjectDirs::from("io", "maestro", "maestro")
        .map(|p| p.cache_dir().join("rendered-templates"))
        .context("could not resolve XDG cache directory")
}

fn print_outcome(outcome: &SyncOutcome) {
    match outcome {
        SyncOutcome::InSync => println!("sync-templates: in sync"),
        SyncOutcome::Wrote(paths) => {
            println!("sync-templates: wrote {} file(s)", paths.len());
            for p in paths {
                println!("  + {}", p.display());
            }
        }
        SyncOutcome::DryRunPlanned(paths) => {
            println!("sync-templates: dry-run, {} planned write(s)", paths.len());
            for p in paths {
                println!("  ~ {}", p.display());
            }
        }
        SyncOutcome::DriftDetected { paths, diffs } => {
            eprintln!("sync-templates: DRIFT detected in {} file(s)", paths.len());
            for (p, d) in paths.iter().zip(diffs.iter()) {
                eprintln!("\n--- {}", p.display());
                eprintln!("{d}");
            }
        }
    }
}
