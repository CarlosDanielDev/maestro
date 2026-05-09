//! `team` subcommand surface for the `maestro` CLI.
//!
//! Kept separate from `src/cli.rs` so the latter stays compact, but
//! self-contained (no cross-crate imports) because `build.rs` includes
//! both files via `#[path]` to generate man pages and shell completions.

use clap::{Subcommand, ValueEnum};

/// Where a new team preset is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum TeamTier {
    User,
    Project,
}

#[derive(Subcommand, Debug)]
pub enum TeamSubcommand {
    /// List all resolved teams (built-in, user, project) with tier and primitive
    List {
        /// Output as JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Create a new team preset by extending an existing one
    New {
        /// Name for the new preset (filename stem)
        name: String,
        /// Parent preset to extend (must already resolve)
        #[arg(long)]
        extends: String,
        /// Where to save the preset
        #[arg(long, value_enum, default_value_t = TeamTier::User)]
        tier: TeamTier,
        /// Override the implementer agent
        #[arg(long)]
        implementer: Option<String>,
        /// Override the reviewer agent
        #[arg(long)]
        reviewer: Option<String>,
        /// Override the docs agent
        #[arg(long)]
        docs: Option<String>,
    },
    /// Launch a team on an issue or set of issues
    Launch {
        /// Preset name to launch
        preset: String,
        /// Single issue number (mutually exclusive with --issues)
        #[arg(long, conflicts_with = "issues")]
        issue: Option<u64>,
        /// Comma-separated issue numbers (mutually exclusive with --issue)
        #[arg(long, value_delimiter = ',', conflicts_with = "issue")]
        issues: Vec<u64>,
        /// Headless: skip the wizard, run plan to completion, exit non-zero on any failure
        #[arg(long)]
        yes: bool,
        /// Cap concurrent in-flight issues
        #[arg(long, default_value_t = 3)]
        max_parallel: usize,
    },
    /// Manage user-tier team presets
    Manage {
        /// Print user-tier presets and exit (no interactive prompts)
        #[arg(long)]
        list: bool,
    },
    /// Print a team's resolved bindings with provenance per field
    Explain {
        /// Preset name
        name: String,
        /// Output as JSON instead of text
        #[arg(long)]
        json: bool,
    },
}
