//! The command-line frontend (ADR 006): clap definitions and the headless
//! subcommands. Depends on `maestro-core` only — no terminal-UI crate. The
//! `maestro` binary parses [`cli::Cli`], runs the subcommands that open the
//! TUI (`maestro`, `run`, `resume`) itself, and hands the rest to
//! [`dispatch`] (#1307).

#![warn(clippy::needless_pass_by_ref_mut)]
#![warn(clippy::redundant_clone)]
#![warn(clippy::significant_drop_tightening)]
#![warn(clippy::fallible_impl_from)]
#![warn(clippy::path_buf_push_overwrite)]
#![warn(clippy::branches_sharing_code)]

pub mod cli;
pub mod commands;

// Core modules (ADR 006); these imports keep the `crate::<module>` paths the
// commands used in the binary resolving here.
#[cfg(test)]
use maestro_core::system;
use maestro_core::{
    adapt, agent_provider, config, doctor, init, insight, notifications, orchestration, provider,
    sanitize, session, state, templates, turboquant, util, work,
};

#[cfg(test)]
mod integration_tests;

use cli::{Commands, PrdSourceArg, RoleArg};
use commands::*;

impl From<PrdSourceArg> for adapt::prd_source::PrdSource {
    fn from(arg: PrdSourceArg) -> Self {
        match arg {
            PrdSourceArg::Local => Self::Local,
            PrdSourceArg::Github => Self::Github,
            PrdSourceArg::Azure => Self::Azure,
            PrdSourceArg::Both => Self::Both,
        }
    }
}

impl From<RoleArg> for session::role::Role {
    fn from(arg: RoleArg) -> Self {
        match arg {
            RoleArg::Implementer => Self::Implementer,
            RoleArg::Orchestrator => Self::Orchestrator,
            RoleArg::Reviewer => Self::Reviewer,
            RoleArg::Docs => Self::Docs,
            RoleArg::DevOps => Self::DevOps,
        }
    }
}

/// Run one headless subcommand. `init_prompter` answers the interactive
/// `maestro init` questions; the binary supplies the terminal one.
pub async fn dispatch(
    command: Commands,
    init_prompter: &mut dyn InitPrompter,
) -> anyhow::Result<()> {
    match command {
        Commands::Init {
            reset,
            non_interactive,
        } => cmd_init(reset, non_interactive, init_prompter),
        Commands::Clean { dry_run, stale } => cmd_clean(dry_run, stale),
        Commands::Logs { session, export } => cmd_logs(session, export),
        Commands::TestSlack => cmd_test_slack().await,
        Commands::Completions { shell } => cli::cmd_completions(shell),
        Commands::Mangen { out_dir } => cli::cmd_mangen(&out_dir),
        Commands::Team { action } => commands::team::dispatch(action).await,
        Commands::SyncTemplates {
            provider,
            check,
            dry_run,
        } => cmd_sync_templates(SyncTemplatesArgs {
            provider,
            check,
            dry_run,
        }),
        Commands::Trust { yes } => commands::trust::cmd_trust(yes),
        Commands::Untrust => commands::trust::cmd_untrust(),
        Commands::Doctor => cmd_doctor(),
        Commands::Insight { action } => match action {
            cli::InsightAction::Scan => insight::run_cli(std::path::Path::new(".")),
        },
        Commands::Adapt {
            path,
            dry_run,
            no_issues,
            scan_only,
            model,
            source,
        } => {
            adapt::cmd_adapt(adapt::AdaptConfig {
                path,
                dry_run,
                no_issues,
                scan_only,
                model,
                prd_source: source.into(),
            })
            .await
        }
        Commands::Prd {
            path,
            model,
            force,
            source,
        } => {
            adapt::cmd_prd(adapt::PrdConfig {
                path,
                model,
                force,
                source: source.into(),
            })
            .await
        }
        Commands::Sanitize {
            path,
            output,
            severity,
            skip_ai,
            model,
        } => {
            let output_fmt = match output {
                cli::SanitizeOutputFormat::Text => sanitize::OutputFormat::Text,
                cli::SanitizeOutputFormat::Json => sanitize::OutputFormat::Json,
                cli::SanitizeOutputFormat::Markdown => sanitize::OutputFormat::Markdown,
            };
            let sev = match severity {
                cli::SanitizeSeverityFilter::Critical => sanitize::Severity::Critical,
                cli::SanitizeSeverityFilter::Warning => sanitize::Severity::Warning,
                cli::SanitizeSeverityFilter::Info => sanitize::Severity::Info,
            };
            sanitize::cmd_sanitize(sanitize::SanitizeConfig {
                path,
                output: output_fmt,
                severity: sev,
                skip_ai,
                model,
            })
            .await
        }
        Commands::TurboQuant { action } => match action {
            cli::TurboQuantAction::Benchmark {
                dim,
                count,
                bits,
                output,
            } => cmd_turboquant_benchmark(dim, count, bits, output),
        },
        Commands::Status => cmd_status(),
        Commands::Cost => cmd_cost(),
        Commands::Queue => cmd_queue().await,
        Commands::Add { issue_number } => cmd_add(issue_number).await,
        Commands::Run { .. } | Commands::Resume { .. } => {
            anyhow::bail!("this subcommand opens the TUI; the maestro binary runs it")
        }
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use crate::provider::types::ProviderKind;
    use clap::Parser;

    struct NoPrompter;

    impl InitPrompter for NoPrompter {
        fn choose_provider(&mut self, _: ProviderKind) -> anyhow::Result<ProviderKind> {
            anyhow::bail!("no prompt in tests")
        }
        fn prompt_azdo_organization(&mut self) -> anyhow::Result<String> {
            anyhow::bail!("no prompt in tests")
        }
        fn prompt_azdo_project(&mut self) -> anyhow::Result<String> {
            anyhow::bail!("no prompt in tests")
        }
    }

    async fn dispatch_args(args: &[&str]) -> anyhow::Result<()> {
        let parsed = cli::Cli::try_parse_from(args).expect("args parse");
        let command = parsed.command.expect("a subcommand");
        dispatch(command, &mut NoPrompter).await
    }

    /// `run` and `resume` open the TUI, so the binary runs them; reaching
    /// the CLI crate with one is a wiring bug and must not exit 0.
    #[tokio::test]
    async fn dispatch_refuses_the_subcommands_that_open_the_tui() {
        for args in [&["maestro", "run"][..], &["maestro", "resume"][..]] {
            let err = dispatch_args(args).await.expect_err("must refuse");
            assert!(err.to_string().contains("opens the TUI"), "{err}");
        }
    }
}
