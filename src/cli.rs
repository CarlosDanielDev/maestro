use clap::{Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[command(
    name = "maestro",
    version,
    about = "Multi-session Claude Code orchestrator"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
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
    Init,
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
    /// Generate man page (hidden, for packaging)
    #[command(hide = true)]
    Mangen {
        /// Output directory for man page
        out_dir: std::path::PathBuf,
    },
}

pub fn generate_completions<W: std::io::Write>(shell: Shell, out: &mut W) {
    use clap::CommandFactory;
    use clap_complete::generate;

    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "maestro", out);
}

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
        let cli =
            Cli::try_parse_from(["maestro", "run", "--prompt", "hello", "--once"]).unwrap();
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
            assert!(!once, "--once must default to false when only --issue is given");
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

    #[test]
    fn mangen_fails_on_unwritable_path() {
        let result = cmd_mangen(Path::new("/nonexistent/path/maestro-test-qa-9f3a"));
        assert!(
            result.is_err(),
            "mangen must fail when out_dir cannot be created"
        );
    }
}
