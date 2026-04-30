use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

/// Output format for sanitize reports (CLI-local, converted to sanitize::OutputFormat in main).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SanitizeOutputFormat {
    Text,
    Json,
    Markdown,
}

/// CLI-local PRD source (converted to adapt::prd_source::PrdSource in main).
///
/// Kept in this file because `build.rs` includes `src/cli.rs` directly with
/// `#[path]` and must not pull in the rest of the crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
#[clap(rename_all = "snake_case")]
pub enum PrdSourceArg {
    #[default]
    Local,
    Github,
    Azure,
    Both,
}

/// Severity filter for sanitize reports (CLI-local, converted to sanitize::Severity in main).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SanitizeSeverityFilter {
    Critical,
    Warning,
    Info,
}

/// CLI-local Role argument (converted to `session::role::Role` in main).
///
/// Kept in this file because `build.rs` includes `src/cli.rs` directly with
/// `#[path]` and must not pull in the rest of the crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum RoleArg {
    Implementer,
    Orchestrator,
    Reviewer,
    Docs,
    DevOps,
}

#[derive(Parser)]
#[command(
    name = "maestro",
    version,
    about = "Multi-session Claude Code orchestrator"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Auto-accept review corrections without confirmation.
    /// Session-only; cannot be persisted as a default. ⚠ DANGER:
    /// edits, commits, and pushes are applied without per-suggestion review.
    /// See issue #328 for the full safety rails.
    #[arg(long = "bypass-review", global = true)]
    pub bypass_review: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run sessions from GitHub issues or a prompt
    Run {
        /// Prompt to send to Claude
        #[arg(short, long)]
        prompt: Option<String>,

        /// GitHub issue number(s), comma-separated
        #[arg(short, long)]
        issue: Option<String>,

        /// Milestone to fetch all issues from
        #[arg(short = 'M', long)]
        milestone: Option<String>,

        /// Model to use (opus, sonnet, haiku)
        #[arg(short, long)]
        model: Option<String>,

        /// Session mode (orchestrator, vibe, review, or custom)
        #[arg(long)]
        mode: Option<String>,

        /// Max concurrent sessions (overrides config)
        #[arg(long)]
        max_concurrent: Option<usize>,

        /// Resume from previous state after a crash
        #[arg(long)]
        resume: bool,

        /// Skip preflight doctor checks before launching sessions
        #[arg(long)]
        skip_doctor: bool,

        /// Image file(s) to attach as visual context (can be repeated)
        #[arg(long = "image")]
        images: Vec<std::path::PathBuf>,

        /// Exit after all sessions complete (CI/scripting mode, no dashboard return)
        #[arg(long)]
        once: bool,

        /// Continuous mode: auto-advance to next ready issue after each completion (use with --milestone)
        #[arg(short = 'C', long)]
        continuous: bool,

        /// Enable a feature flag (repeatable, e.g. --enable-flag ci_auto_fix)
        #[arg(long = "enable-flag", value_name = "FLAG")]
        enable_flags: Vec<String>,

        /// Disable a feature flag (repeatable, e.g. --disable-flag auto_fork)
        #[arg(long = "disable-flag", value_name = "FLAG")]
        disable_flags: Vec<String>,

        /// Override role classification for the spawned session(s).
        /// If omitted, the role is derived from the prompt text.
        #[arg(long, value_enum)]
        role: Option<RoleArg>,

        /// Skip the startup splash screen
        #[arg(long)]
        no_splash: bool,
    },
    /// Show queued/pending issues from GitHub
    Queue,
    /// Add an issue to the work queue manually
    Add {
        /// Issue number to add
        issue_number: u64,
    },
    /// Show current state without TUI
    Status,
    /// Show spending report
    Cost,
    /// Initialize maestro.toml in current directory
    Init {
        /// Re-run technology detection on an existing maestro.toml,
        /// merging detected defaults without overwriting customized keys.
        #[arg(long)]
        reset: bool,
    },
    /// Clean orphaned worktrees left by crashed sessions
    Clean {
        /// Show what would be cleaned without actually doing it
        #[arg(long)]
        dry_run: bool,
    },
    /// Show session transcript logs
    Logs {
        /// Show full log for a specific session ID
        #[arg(long)]
        session: Option<String>,
        /// Export as JSON
        #[arg(long)]
        export: Option<String>,
    },
    /// Resume interrupted sessions from saved state
    Resume {
        /// Resume a specific session by ID
        #[arg(long)]
        session: Option<String>,
    },
    /// Test Slack webhook configuration
    TestSlack,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Check environment setup and required tools
    Doctor,
    /// Onboard an existing project to the maestro workflow
    Adapt {
        /// Path to the project to onboard (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        path: std::path::PathBuf,

        /// Preview what would be created without making changes
        #[arg(long)]
        dry_run: bool,

        /// Analyze and plan but do not create GitHub issues
        #[arg(long)]
        no_issues: bool,

        /// Only run Phase 1 (project scanning), output profile as JSON
        #[arg(long)]
        scan_only: bool,

        /// AI model to use for analysis and planning
        #[arg(short, long)]
        model: Option<String>,

        /// Where the PRD lives: local file, GitHub issue, Azure DevOps, or both
        #[arg(long, value_enum, default_value_t = PrdSourceArg::Local)]
        source: PrdSourceArg,
    },
    /// Generate a Product Requirements Document from project analysis
    Prd {
        /// Path to the project (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        path: std::path::PathBuf,

        /// AI model to use for analysis and generation
        #[arg(short, long)]
        model: Option<String>,

        /// Overwrite existing PRD without confirmation
        #[arg(long)]
        force: bool,

        /// Where the PRD lives: local file, GitHub issue, Azure DevOps, or both
        #[arg(long, value_enum, default_value_t = PrdSourceArg::Local)]
        source: PrdSourceArg,
    },
    /// Analyze codebase for dead code and code smells
    Sanitize {
        /// Path to scan (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        path: std::path::PathBuf,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = SanitizeOutputFormat::Text)]
        output: SanitizeOutputFormat,

        /// Minimum severity to report
        #[arg(short, long, value_enum, default_value_t = SanitizeSeverityFilter::Info)]
        severity: SanitizeSeverityFilter,

        /// Skip AI-powered analysis (Phase 2)
        #[arg(long)]
        skip_ai: bool,

        /// AI model to use for analysis
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Run TurboQuant vector quantization benchmarks
    TurboQuant {
        #[command(subcommand)]
        action: TurboQuantAction,
    },
    /// Generate man page (hidden, for packaging)
    #[command(hide = true)]
    Mangen {
        /// Output directory for man page
        out_dir: std::path::PathBuf,
    },
}

/// Output format for TurboQuant benchmark reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BenchmarkOutputFormat {
    Text,
    Json,
}

#[derive(Subcommand)]
pub enum TurboQuantAction {
    /// Run compression benchmarks
    Benchmark {
        /// Vector dimensionality
        #[arg(long, default_value_t = 768)]
        dim: usize,
        /// Number of vectors to benchmark
        #[arg(long, default_value_t = 10000)]
        count: usize,
        /// Bit width for quantization
        #[arg(long, default_value_t = 4)]
        bits: u8,
        /// Output format
        #[arg(long, value_enum, default_value_t = BenchmarkOutputFormat::Text)]
        output: BenchmarkOutputFormat,
    },
}

#[allow(dead_code)]
pub fn generate_completions<W: std::io::Write>(shell: Shell, out: &mut W) {
    use clap::CommandFactory;
    use clap_complete::generate;

    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "maestro", out);
}

#[allow(dead_code)]
pub fn cmd_completions(shell: Shell) -> anyhow::Result<()> {
    generate_completions(shell, &mut std::io::stdout());
    Ok(())
}

pub fn cmd_mangen(out_dir: &std::path::Path) -> anyhow::Result<()> {
    use clap::CommandFactory;

    std::fs::create_dir_all(out_dir)?;
    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buffer = Vec::new();
    man.render(&mut buffer)?;
    std::fs::write(out_dir.join("maestro.1"), buffer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::path::{Path, PathBuf};

    // ------------------------------------------------------------------
    // Clap struct integrity
    // ------------------------------------------------------------------

    #[test]
    fn cli_debug_assert() {
        <Cli as clap::CommandFactory>::command().debug_assert();
    }

    // ------------------------------------------------------------------
    // Init subcommand parsing (#505)
    // ------------------------------------------------------------------

    #[test]
    fn init_subcommand_no_flag_parses_reset_false() {
        let cli = Cli::try_parse_from(["maestro", "init"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Init { reset: false })));
    }

    #[test]
    fn init_subcommand_reset_flag_parses_reset_true() {
        let cli = Cli::try_parse_from(["maestro", "init", "--reset"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Init { reset: true })));
    }

    // ------------------------------------------------------------------
    // Completions subcommand parsing
    // ------------------------------------------------------------------

    #[test]
    fn completions_subcommand_accepts_bash() {
        let cli = Cli::try_parse_from(["maestro", "completions", "bash"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Completions { shell: Shell::Bash })
        ));
    }

    #[test]
    fn completions_subcommand_accepts_zsh() {
        let cli = Cli::try_parse_from(["maestro", "completions", "zsh"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Completions { shell: Shell::Zsh })
        ));
    }

    #[test]
    fn completions_subcommand_accepts_fish() {
        let cli = Cli::try_parse_from(["maestro", "completions", "fish"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Completions { shell: Shell::Fish })
        ));
    }

    #[test]
    fn completions_subcommand_rejects_invalid_shell() {
        let result = Cli::try_parse_from(["maestro", "completions", "invalid-shell"]);
        assert!(result.is_err(), "invalid shell must be rejected by clap");
    }

    // ------------------------------------------------------------------
    // Mangen subcommand parsing
    // ------------------------------------------------------------------

    #[test]
    fn mangen_subcommand_parses_out_dir() {
        let cli = Cli::try_parse_from(["maestro", "mangen", "/tmp/man"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Mangen { out_dir }) if out_dir == PathBuf::from("/tmp/man")
        ));
    }

    #[test]
    fn mangen_subcommand_requires_out_dir_argument() {
        let result = Cli::try_parse_from(["maestro", "mangen"]);
        assert!(result.is_err(), "mangen without out_dir must fail");
    }

    #[test]
    fn mangen_is_hidden_from_help() {
        let help = <Cli as clap::CommandFactory>::command()
            .render_help()
            .to_string();
        assert!(
            !help.contains("mangen"),
            "mangen must not appear in --help output"
        );
    }

    // ------------------------------------------------------------------
    // generate_completions output
    // ------------------------------------------------------------------

    #[test]
    fn completions_bash_output_contains_program_name() {
        let mut buf = Vec::<u8>::new();
        generate_completions(Shell::Bash, &mut buf);
        let script = String::from_utf8(buf).expect("bash output must be valid UTF-8");
        assert!(
            script.contains("maestro"),
            "bash completion must reference the binary name 'maestro'"
        );
    }

    #[test]
    fn completions_zsh_output_is_non_empty() {
        let mut buf = Vec::<u8>::new();
        generate_completions(Shell::Zsh, &mut buf);
        assert!(!buf.is_empty());
    }

    #[test]
    fn completions_fish_output_is_valid_utf8() {
        let mut buf = Vec::<u8>::new();
        generate_completions(Shell::Fish, &mut buf);
        assert!(
            String::from_utf8(buf).is_ok(),
            "fish completion output must be valid UTF-8"
        );
    }

    // ------------------------------------------------------------------
    // cmd_mangen filesystem behavior
    // ------------------------------------------------------------------

    #[test]
    fn mangen_file_is_named_after_binary() {
        let dir = tempfile::TempDir::new().unwrap();
        cmd_mangen(dir.path()).unwrap();
        assert!(
            dir.path().join("maestro.1").exists(),
            "man page file must be named maestro.1"
        );
    }

    #[test]
    fn mangen_output_contains_roff_title_heading() {
        let dir = tempfile::TempDir::new().unwrap();
        cmd_mangen(dir.path()).unwrap();
        let contents = std::fs::read_to_string(dir.path().join("maestro.1")).unwrap();
        assert!(!contents.is_empty(), "man page must not be empty");
        assert!(
            contents.contains(".TH"),
            "man page must contain roff .TH macro"
        );
    }

    // ------------------------------------------------------------------
    // --once flag parsing
    // ------------------------------------------------------------------

    #[test]
    fn run_once_flag_defaults_to_false() {
        let cli = Cli::try_parse_from(["maestro", "run", "--prompt", "hello"]).unwrap();
        if let Some(Commands::Run { once, .. }) = cli.command {
            assert!(!once, "--once must default to false");
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_once_flag_is_set_when_provided() {
        let cli = Cli::try_parse_from(["maestro", "run", "--prompt", "hello", "--once"]).unwrap();
        if let Some(Commands::Run { once, .. }) = cli.command {
            assert!(once, "--once must be true when flag is provided");
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_once_flag_is_false_with_issue_only() {
        let cli = Cli::try_parse_from(["maestro", "run", "--issue", "42"]).unwrap();
        if let Some(Commands::Run { once, .. }) = cli.command {
            assert!(
                !once,
                "--once must default to false when only --issue is given"
            );
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_once_flag_coexists_with_other_flags() {
        let cli = Cli::try_parse_from([
            "maestro", "run", "--prompt", "x", "--once", "--model", "haiku", "--resume",
        ])
        .unwrap();
        if let Some(Commands::Run {
            once,
            model,
            resume,
            ..
        }) = cli.command
        {
            assert!(once);
            assert_eq!(model.as_deref(), Some("haiku"));
            assert!(resume);
        } else {
            panic!("Expected Commands::Run");
        }
    }

    // ------------------------------------------------------------------
    // --continuous / -C flag parsing
    // ------------------------------------------------------------------

    #[test]
    fn run_continuous_flag_defaults_to_false() {
        let cli = Cli::try_parse_from(["maestro", "run", "--prompt", "hello"]).unwrap();
        if let Some(Commands::Run { continuous, .. }) = cli.command {
            assert!(!continuous, "--continuous must default to false");
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_continuous_long_flag_is_set_when_provided() {
        let cli =
            Cli::try_parse_from(["maestro", "run", "--prompt", "hello", "--continuous"]).unwrap();
        if let Some(Commands::Run { continuous, .. }) = cli.command {
            assert!(
                continuous,
                "--continuous must be true when flag is provided"
            );
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_continuous_short_flag_is_set_when_provided() {
        let cli = Cli::try_parse_from(["maestro", "run", "--prompt", "hello", "-C"]).unwrap();
        if let Some(Commands::Run { continuous, .. }) = cli.command {
            assert!(continuous, "-C must set continuous to true");
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_continuous_coexists_with_once_flag() {
        let cli =
            Cli::try_parse_from(["maestro", "run", "--prompt", "x", "--continuous", "--once"])
                .unwrap();
        if let Some(Commands::Run {
            continuous, once, ..
        }) = cli.command
        {
            assert!(continuous, "--continuous must be true");
            assert!(once, "--once must be true");
        } else {
            panic!("Expected Commands::Run");
        }
    }

    // ------------------------------------------------------------------
    // --enable-flag / --disable-flag parsing (Issue #143)
    // ------------------------------------------------------------------

    #[test]
    fn run_enable_flag_parses_single_value() {
        let cli = Cli::try_parse_from([
            "maestro",
            "run",
            "--prompt",
            "hello",
            "--enable-flag",
            "ci_auto_fix",
        ])
        .unwrap();
        if let Some(Commands::Run { enable_flags, .. }) = cli.command {
            assert_eq!(enable_flags, vec!["ci_auto_fix"]);
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_disable_flag_parses_single_value() {
        let cli = Cli::try_parse_from([
            "maestro",
            "run",
            "--prompt",
            "hello",
            "--disable-flag",
            "auto_fork",
        ])
        .unwrap();
        if let Some(Commands::Run { disable_flags, .. }) = cli.command {
            assert_eq!(disable_flags, vec!["auto_fork"]);
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_enable_flag_accumulates_multiple_values() {
        let cli = Cli::try_parse_from([
            "maestro",
            "run",
            "--prompt",
            "hello",
            "--enable-flag",
            "ci_auto_fix",
            "--enable-flag",
            "review_council",
        ])
        .unwrap();
        if let Some(Commands::Run { enable_flags, .. }) = cli.command {
            assert_eq!(enable_flags, vec!["ci_auto_fix", "review_council"]);
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_disable_flag_accumulates_multiple_values() {
        let cli = Cli::try_parse_from([
            "maestro",
            "run",
            "--prompt",
            "hello",
            "--disable-flag",
            "continuous_mode",
            "--disable-flag",
            "auto_fork",
        ])
        .unwrap();
        if let Some(Commands::Run { disable_flags, .. }) = cli.command {
            assert_eq!(disable_flags, vec!["continuous_mode", "auto_fork"]);
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_enable_and_disable_flags_coexist() {
        let cli = Cli::try_parse_from([
            "maestro",
            "run",
            "--prompt",
            "hello",
            "--enable-flag",
            "ci_auto_fix",
            "--disable-flag",
            "auto_fork",
        ])
        .unwrap();
        if let Some(Commands::Run {
            enable_flags,
            disable_flags,
            ..
        }) = cli.command
        {
            assert_eq!(enable_flags, vec!["ci_auto_fix"]);
            assert_eq!(disable_flags, vec!["auto_fork"]);
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_flags_default_to_empty_vecs() {
        let cli = Cli::try_parse_from(["maestro", "run", "--prompt", "hello"]).unwrap();
        if let Some(Commands::Run {
            enable_flags,
            disable_flags,
            ..
        }) = cli.command
        {
            assert!(enable_flags.is_empty());
            assert!(disable_flags.is_empty());
        } else {
            panic!("Expected Commands::Run");
        }
    }

    // ------------------------------------------------------------------
    // Prd subcommand parsing
    // ------------------------------------------------------------------

    #[test]
    fn prd_subcommand_parses_with_no_flags() {
        let cli = Cli::try_parse_from(["maestro", "prd"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Prd { .. })));
    }

    #[test]
    fn prd_path_defaults_to_current_dir() {
        let cli = Cli::try_parse_from(["maestro", "prd"]).unwrap();
        if let Some(Commands::Prd { path, .. }) = cli.command {
            assert_eq!(path, PathBuf::from("."));
        } else {
            panic!("Expected Commands::Prd");
        }
    }

    #[test]
    fn prd_force_defaults_to_false() {
        let cli = Cli::try_parse_from(["maestro", "prd"]).unwrap();
        if let Some(Commands::Prd { force, .. }) = cli.command {
            assert!(!force);
        } else {
            panic!("Expected Commands::Prd");
        }
    }

    #[test]
    fn prd_force_is_set_when_provided() {
        let cli = Cli::try_parse_from(["maestro", "prd", "--force"]).unwrap();
        if let Some(Commands::Prd { force, .. }) = cli.command {
            assert!(force);
        } else {
            panic!("Expected Commands::Prd");
        }
    }

    #[test]
    fn prd_model_accepts_value() {
        let cli = Cli::try_parse_from(["maestro", "prd", "--model", "opus"]).unwrap();
        if let Some(Commands::Prd { model, .. }) = cli.command {
            assert_eq!(model.as_deref(), Some("opus"));
        } else {
            panic!("Expected Commands::Prd");
        }
    }

    #[test]
    fn mangen_fails_on_unwritable_path() {
        let result = cmd_mangen(Path::new("/nonexistent/path/maestro-test-qa-9f3a"));
        assert!(
            result.is_err(),
            "mangen must fail when out_dir cannot be created"
        );
    }

    // ------------------------------------------------------------------
    // Sanitize subcommand parsing
    // ------------------------------------------------------------------

    #[test]
    fn sanitize_subcommand_parses_with_no_flags() {
        let cli = Cli::try_parse_from(["maestro", "sanitize"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Sanitize { .. })));
    }

    #[test]
    fn sanitize_output_defaults_to_text() {
        let cli = Cli::try_parse_from(["maestro", "sanitize"]).unwrap();
        if let Some(Commands::Sanitize { output, .. }) = cli.command {
            assert_eq!(output, SanitizeOutputFormat::Text);
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_output_accepts_json() {
        let cli = Cli::try_parse_from(["maestro", "sanitize", "--output", "json"]).unwrap();
        if let Some(Commands::Sanitize { output, .. }) = cli.command {
            assert_eq!(output, SanitizeOutputFormat::Json);
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_output_accepts_markdown() {
        let cli = Cli::try_parse_from(["maestro", "sanitize", "--output", "markdown"]).unwrap();
        if let Some(Commands::Sanitize { output, .. }) = cli.command {
            assert_eq!(output, SanitizeOutputFormat::Markdown);
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_output_rejects_invalid_value() {
        let result = Cli::try_parse_from(["maestro", "sanitize", "--output", "html"]);
        assert!(result.is_err(), "--output html must be rejected by clap");
    }

    #[test]
    fn sanitize_severity_defaults_to_info() {
        let cli = Cli::try_parse_from(["maestro", "sanitize"]).unwrap();
        if let Some(Commands::Sanitize { severity, .. }) = cli.command {
            assert_eq!(severity, SanitizeSeverityFilter::Info);
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_severity_accepts_warning() {
        let cli = Cli::try_parse_from(["maestro", "sanitize", "--severity", "warning"]).unwrap();
        if let Some(Commands::Sanitize { severity, .. }) = cli.command {
            assert_eq!(severity, SanitizeSeverityFilter::Warning);
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_severity_accepts_critical() {
        let cli = Cli::try_parse_from(["maestro", "sanitize", "--severity", "critical"]).unwrap();
        if let Some(Commands::Sanitize { severity, .. }) = cli.command {
            assert_eq!(severity, SanitizeSeverityFilter::Critical);
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_severity_rejects_invalid_value() {
        let result = Cli::try_parse_from(["maestro", "sanitize", "--severity", "bogus"]);
        assert!(result.is_err());
    }

    #[test]
    fn sanitize_skip_ai_defaults_to_false() {
        let cli = Cli::try_parse_from(["maestro", "sanitize"]).unwrap();
        if let Some(Commands::Sanitize { skip_ai, .. }) = cli.command {
            assert!(!skip_ai, "--skip-ai must default to false");
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_skip_ai_is_set_when_provided() {
        let cli = Cli::try_parse_from(["maestro", "sanitize", "--skip-ai"]).unwrap();
        if let Some(Commands::Sanitize { skip_ai, .. }) = cli.command {
            assert!(skip_ai, "--skip-ai must be true when flag is provided");
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_model_defaults_to_none() {
        let cli = Cli::try_parse_from(["maestro", "sanitize"]).unwrap();
        if let Some(Commands::Sanitize { model, .. }) = cli.command {
            assert!(model.is_none());
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_model_accepts_value() {
        let cli = Cli::try_parse_from(["maestro", "sanitize", "--model", "opus"]).unwrap();
        if let Some(Commands::Sanitize { model, .. }) = cli.command {
            assert_eq!(model.as_deref(), Some("opus"));
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_path_defaults_to_current_dir() {
        let cli = Cli::try_parse_from(["maestro", "sanitize"]).unwrap();
        if let Some(Commands::Sanitize { path, .. }) = cli.command {
            assert_eq!(path, PathBuf::from("."));
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_path_accepts_value() {
        let cli = Cli::try_parse_from(["maestro", "sanitize", "--path", "/src"]).unwrap();
        if let Some(Commands::Sanitize { path, .. }) = cli.command {
            assert_eq!(path, PathBuf::from("/src"));
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    #[test]
    fn sanitize_all_flags_coexist() {
        let cli = Cli::try_parse_from([
            "maestro",
            "sanitize",
            "--path",
            "/project",
            "--output",
            "json",
            "--severity",
            "warning",
            "--skip-ai",
            "--model",
            "haiku",
        ])
        .unwrap();
        if let Some(Commands::Sanitize {
            path,
            output,
            severity,
            skip_ai,
            model,
        }) = cli.command
        {
            assert_eq!(path, PathBuf::from("/project"));
            assert_eq!(output, SanitizeOutputFormat::Json);
            assert_eq!(severity, SanitizeSeverityFilter::Warning);
            assert!(skip_ai);
            assert_eq!(model.as_deref(), Some("haiku"));
        } else {
            panic!("Expected Commands::Sanitize");
        }
    }

    // ------------------------------------------------------------------
    // Adapt subcommand parsing
    // ------------------------------------------------------------------

    #[test]
    fn adapt_subcommand_parses_with_no_flags() {
        let cli = Cli::try_parse_from(["maestro", "adapt"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Adapt { .. })));
    }

    #[test]
    fn adapt_path_defaults_to_current_dir() {
        let cli = Cli::try_parse_from(["maestro", "adapt"]).unwrap();
        if let Some(Commands::Adapt { path, .. }) = cli.command {
            assert_eq!(path, PathBuf::from("."));
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn adapt_path_accepts_value() {
        let cli = Cli::try_parse_from(["maestro", "adapt", "--path", "/project"]).unwrap();
        if let Some(Commands::Adapt { path, .. }) = cli.command {
            assert_eq!(path, PathBuf::from("/project"));
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn adapt_dry_run_defaults_to_false() {
        let cli = Cli::try_parse_from(["maestro", "adapt"]).unwrap();
        if let Some(Commands::Adapt { dry_run, .. }) = cli.command {
            assert!(!dry_run);
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn adapt_dry_run_is_set_when_provided() {
        let cli = Cli::try_parse_from(["maestro", "adapt", "--dry-run"]).unwrap();
        if let Some(Commands::Adapt { dry_run, .. }) = cli.command {
            assert!(dry_run);
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn adapt_no_issues_defaults_to_false() {
        let cli = Cli::try_parse_from(["maestro", "adapt"]).unwrap();
        if let Some(Commands::Adapt { no_issues, .. }) = cli.command {
            assert!(!no_issues);
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn adapt_no_issues_is_set_when_provided() {
        let cli = Cli::try_parse_from(["maestro", "adapt", "--no-issues"]).unwrap();
        if let Some(Commands::Adapt { no_issues, .. }) = cli.command {
            assert!(no_issues);
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn adapt_scan_only_defaults_to_false() {
        let cli = Cli::try_parse_from(["maestro", "adapt"]).unwrap();
        if let Some(Commands::Adapt { scan_only, .. }) = cli.command {
            assert!(!scan_only);
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn adapt_scan_only_is_set_when_provided() {
        let cli = Cli::try_parse_from(["maestro", "adapt", "--scan-only"]).unwrap();
        if let Some(Commands::Adapt { scan_only, .. }) = cli.command {
            assert!(scan_only);
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn adapt_model_defaults_to_none() {
        let cli = Cli::try_parse_from(["maestro", "adapt"]).unwrap();
        if let Some(Commands::Adapt { model, .. }) = cli.command {
            assert!(model.is_none());
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn adapt_model_accepts_value() {
        let cli = Cli::try_parse_from(["maestro", "adapt", "--model", "opus"]).unwrap();
        if let Some(Commands::Adapt { model, .. }) = cli.command {
            assert_eq!(model.as_deref(), Some("opus"));
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    // --- Issue #390: --source flag ---

    #[test]
    fn prd_source_defaults_to_local() {
        let cli = Cli::try_parse_from(["maestro", "prd"]).unwrap();
        if let Some(Commands::Prd { source, .. }) = cli.command {
            assert_eq!(source, PrdSourceArg::Local);
        } else {
            panic!("Expected Commands::Prd");
        }
    }

    #[test]
    fn prd_source_accepts_github() {
        let cli = Cli::try_parse_from(["maestro", "prd", "--source", "github"]).unwrap();
        if let Some(Commands::Prd { source, .. }) = cli.command {
            assert_eq!(source, PrdSourceArg::Github);
        } else {
            panic!("Expected Commands::Prd");
        }
    }

    #[test]
    fn prd_source_accepts_azure() {
        let cli = Cli::try_parse_from(["maestro", "prd", "--source", "azure"]).unwrap();
        if let Some(Commands::Prd { source, .. }) = cli.command {
            assert_eq!(source, PrdSourceArg::Azure);
        } else {
            panic!("Expected Commands::Prd");
        }
    }

    #[test]
    fn prd_source_accepts_both() {
        let cli = Cli::try_parse_from(["maestro", "prd", "--source", "both"]).unwrap();
        if let Some(Commands::Prd { source, .. }) = cli.command {
            assert_eq!(source, PrdSourceArg::Both);
        } else {
            panic!("Expected Commands::Prd");
        }
    }

    #[test]
    fn adapt_source_flag_works_too() {
        let cli = Cli::try_parse_from(["maestro", "adapt", "--source", "github"]).unwrap();
        if let Some(Commands::Adapt { source, .. }) = cli.command {
            assert_eq!(source, PrdSourceArg::Github);
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    #[test]
    fn prd_source_invalid_value_is_rejected() {
        let result = Cli::try_parse_from(["maestro", "prd", "--source", "trello"]);
        assert!(result.is_err(), "unknown source should be rejected by clap");
    }

    #[test]
    fn adapt_all_flags_coexist() {
        let cli = Cli::try_parse_from([
            "maestro",
            "adapt",
            "--path",
            "/project",
            "--dry-run",
            "--no-issues",
            "--scan-only",
            "--model",
            "haiku",
        ])
        .unwrap();
        if let Some(Commands::Adapt {
            path,
            dry_run,
            no_issues,
            scan_only,
            model,
            ..
        }) = cli.command
        {
            assert_eq!(path, PathBuf::from("/project"));
            assert!(dry_run);
            assert!(no_issues);
            assert!(scan_only);
            assert_eq!(model.as_deref(), Some("haiku"));
        } else {
            panic!("Expected Commands::Adapt");
        }
    }

    // ------------------------------------------------------------------
    // --role flag parsing (Issue #538)
    // ------------------------------------------------------------------

    #[test]
    fn run_role_flag_defaults_to_none() {
        let cli = Cli::try_parse_from(["maestro", "run", "--prompt", "hello"]).unwrap();
        if let Some(Commands::Run { role, .. }) = cli.command {
            assert!(role.is_none(), "--role must default to None");
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_role_flag_accepts_orchestrator() {
        let cli = Cli::try_parse_from([
            "maestro",
            "run",
            "--prompt",
            "hello",
            "--role",
            "orchestrator",
        ])
        .unwrap();
        if let Some(Commands::Run { role, .. }) = cli.command {
            assert_eq!(role, Some(RoleArg::Orchestrator));
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_role_flag_accepts_implementer() {
        let cli = Cli::try_parse_from([
            "maestro",
            "run",
            "--prompt",
            "hello",
            "--role",
            "implementer",
        ])
        .unwrap();
        if let Some(Commands::Run { role, .. }) = cli.command {
            assert_eq!(role, Some(RoleArg::Implementer));
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_role_flag_accepts_reviewer() {
        let cli =
            Cli::try_parse_from(["maestro", "run", "--prompt", "x", "--role", "reviewer"]).unwrap();
        if let Some(Commands::Run { role, .. }) = cli.command {
            assert_eq!(role, Some(RoleArg::Reviewer));
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_role_flag_accepts_docs() {
        let cli =
            Cli::try_parse_from(["maestro", "run", "--prompt", "x", "--role", "docs"]).unwrap();
        if let Some(Commands::Run { role, .. }) = cli.command {
            assert_eq!(role, Some(RoleArg::Docs));
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_role_flag_accepts_dev_ops_snake_case() {
        let cli =
            Cli::try_parse_from(["maestro", "run", "--prompt", "x", "--role", "dev_ops"]).unwrap();
        if let Some(Commands::Run { role, .. }) = cli.command {
            assert_eq!(
                role,
                Some(RoleArg::DevOps),
                "--role dev_ops must parse as DevOps"
            );
        } else {
            panic!("Expected Commands::Run");
        }
    }

    #[test]
    fn run_role_flag_rejects_invalid_value() {
        let result =
            Cli::try_parse_from(["maestro", "run", "--prompt", "hello", "--role", "bogus"]);
        assert!(result.is_err(), "--role bogus must be rejected by clap");
    }

    #[test]
    fn run_role_flag_rejects_devops_without_underscore() {
        // clap rename_all = "snake_case" exposes "dev_ops" only, not "devops".
        let result =
            Cli::try_parse_from(["maestro", "run", "--prompt", "hello", "--role", "devops"]);
        assert!(
            result.is_err(),
            "--role devops (without underscore) must be rejected; only dev_ops is valid"
        );
    }

    #[test]
    fn resume_subcommand_has_no_role_flag() {
        // Resume inherits the saved session's role; --role on resume is intentionally not exposed.
        let result = Cli::try_parse_from(["maestro", "resume", "--role", "orchestrator"]);
        assert!(
            result.is_err(),
            "resume must NOT accept --role (saved role is inherited)"
        );
    }
}
