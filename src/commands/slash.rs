//! Minimal in-session slash-command dispatcher (#327).
//!
//! Today's surface is intentionally tiny: `/review <pr>`, `/help`. Growth
//! is additive (new `SlashCommand` enum variants), keeping the dispatcher
//! under the file-size cap.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #327. The dispatcher is wired into the
// session-input loop in Phase 2; tests exercise parsing and dispatch today.
#![allow(dead_code)]

use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Review {
        pr_number: u64,
        branch: Option<String>,
    },
    Help,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SlashParseError {
    Empty,
    NotASlashCommand,
    UnknownCommand(String),
    MissingArgument {
        command: String,
        argument: String,
    },
    InvalidArgument {
        command: String,
        value: String,
        reason: String,
    },
}

impl std::fmt::Display for SlashParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty slash input"),
            Self::NotASlashCommand => write!(f, "input does not start with '/'"),
            Self::UnknownCommand(c) => write!(f, "unknown slash command: /{c}"),
            Self::MissingArgument { command, argument } => {
                write!(f, "/{command} requires <{argument}>")
            }
            Self::InvalidArgument {
                command,
                value,
                reason,
            } => {
                write!(f, "/{command} got invalid value '{value}': {reason}")
            }
        }
    }
}

impl std::error::Error for SlashParseError {}

impl FromStr for SlashCommand {
    type Err = SlashParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(SlashParseError::Empty);
        }
        let stripped = trimmed
            .strip_prefix('/')
            .ok_or(SlashParseError::NotASlashCommand)?;

        let mut parts = stripped.split_whitespace();
        let head = parts.next().ok_or(SlashParseError::Empty)?;

        match head {
            "help" => Ok(Self::Help),
            "review" => {
                let pr_arg = parts
                    .next()
                    .ok_or_else(|| SlashParseError::MissingArgument {
                        command: "review".into(),
                        argument: "pr_number".into(),
                    })?;
                let pr_number = pr_arg
                    .strip_prefix('#')
                    .unwrap_or(pr_arg)
                    .parse::<u64>()
                    .map_err(|e| SlashParseError::InvalidArgument {
                        command: "review".into(),
                        value: pr_arg.into(),
                        reason: e.to_string(),
                    })?;
                let branch = match parts.next() {
                    None => None,
                    Some(b) => {
                        // Validate at the parsing seam so untrusted input
                        // cannot flow into telemetry/audit before downstream
                        // shellouts validate it.
                        crate::util::validate_branch_name(b).map_err(|e| {
                            SlashParseError::InvalidArgument {
                                command: "review".into(),
                                value: b.into(),
                                reason: e.to_string(),
                            }
                        })?;
                        Some(b.to_string())
                    }
                };
                Ok(Self::Review { pr_number, branch })
            }
            other => Err(SlashParseError::UnknownCommand(other.into())),
        }
    }
}

/// Outcome of dispatching a slash command. The TUI app turns this into UI
/// state changes (panel updates, screen pushes, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashOutcome {
    /// Show the static help text in the message log.
    Help(&'static str),
    /// Begin the review pipeline for a given PR. The app routes this to
    /// `ReviewDispatcher`.
    StartReview {
        pr_number: u64,
        branch: Option<String>,
    },
}

pub const HELP_TEXT: &str = "\
Available slash commands:
  /review <pr_number> [branch]  Run reviewers against a PR and post results
  /help                         Show this message
";

/// Trait so the dispatcher can be faked in higher-level tests.
pub trait SlashDispatcher: Send + Sync {
    fn dispatch(&self, command: SlashCommand) -> SlashOutcome;
}

/// Default dispatcher — pure mapping from command to outcome. The
/// side-effecting work (spawning Claude with `/review`, posting comments)
/// happens in the layer that consumes the outcome.
#[derive(Default)]
pub struct DefaultSlashDispatcher;

impl SlashDispatcher for DefaultSlashDispatcher {
    fn dispatch(&self, command: SlashCommand) -> SlashOutcome {
        match command {
            SlashCommand::Help => SlashOutcome::Help(HELP_TEXT),
            SlashCommand::Review { pr_number, branch } => {
                SlashOutcome::StartReview { pr_number, branch }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_help_command() {
        let cmd: SlashCommand = "/help".parse().expect("parse");
        assert_eq!(cmd, SlashCommand::Help);
    }

    #[test]
    fn parse_review_with_pr_number() {
        let cmd: SlashCommand = "/review 42".parse().expect("parse");
        assert_eq!(
            cmd,
            SlashCommand::Review {
                pr_number: 42,
                branch: None,
            }
        );
    }

    #[test]
    fn parse_review_with_hash_prefix() {
        let cmd: SlashCommand = "/review #99".parse().expect("parse");
        assert_eq!(
            cmd,
            SlashCommand::Review {
                pr_number: 99,
                branch: None,
            }
        );
    }

    #[test]
    fn parse_review_with_branch_argument() {
        let cmd: SlashCommand = "/review 42 feat/x".parse().expect("parse");
        assert_eq!(
            cmd,
            SlashCommand::Review {
                pr_number: 42,
                branch: Some("feat/x".into()),
            }
        );
    }

    #[test]
    fn parse_review_without_pr_number_errors() {
        let err = "/review".parse::<SlashCommand>().unwrap_err();
        assert!(matches!(err, SlashParseError::MissingArgument { .. }));
    }

    #[test]
    fn parse_unknown_command_errors() {
        let err = "/bogus".parse::<SlashCommand>().unwrap_err();
        assert!(matches!(err, SlashParseError::UnknownCommand(_)));
    }

    #[test]
    fn parse_non_slash_input_errors() {
        let err = "review 42".parse::<SlashCommand>().unwrap_err();
        assert_eq!(err, SlashParseError::NotASlashCommand);
    }

    #[test]
    fn parse_empty_input_errors() {
        let err = "   ".parse::<SlashCommand>().unwrap_err();
        assert_eq!(err, SlashParseError::Empty);
    }

    #[test]
    fn parse_invalid_pr_number_errors() {
        let err = "/review notanumber".parse::<SlashCommand>().unwrap_err();
        assert!(matches!(err, SlashParseError::InvalidArgument { .. }));
    }

    #[test]
    fn parse_review_rejects_branch_with_shell_metachars() {
        let err = "/review 1 foo;rm".parse::<SlashCommand>().unwrap_err();
        assert!(matches!(err, SlashParseError::InvalidArgument { .. }));
    }

    #[test]
    fn parse_review_rejects_branch_with_double_dots() {
        let err = "/review 1 feat/../etc".parse::<SlashCommand>().unwrap_err();
        assert!(matches!(err, SlashParseError::InvalidArgument { .. }));
    }

    #[test]
    fn dispatch_help_returns_help_text() {
        let d = DefaultSlashDispatcher;
        let outcome = d.dispatch(SlashCommand::Help);
        assert_eq!(outcome, SlashOutcome::Help(HELP_TEXT));
    }

    #[test]
    fn dispatch_review_returns_start_review_outcome() {
        let d = DefaultSlashDispatcher;
        let outcome = d.dispatch(SlashCommand::Review {
            pr_number: 7,
            branch: Some("main".into()),
        });
        assert_eq!(
            outcome,
            SlashOutcome::StartReview {
                pr_number: 7,
                branch: Some("main".into())
            }
        );
    }

    #[test]
    fn help_text_lists_review_and_help() {
        assert!(HELP_TEXT.contains("/review"));
        assert!(HELP_TEXT.contains("/help"));
    }
}
