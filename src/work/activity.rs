//! Structured activity-log lines for the INTERACTION lifecycle (#742).
//!
//! Every lifecycle transition renders through [`InteractionActivity`] so the
//! dashboard's `[INTERACTION]` / `[TEARDOWN]` line formats are pinned in ONE
//! place (and unit-tested below) instead of scattered as ad-hoc `format!`
//! calls. The TUI maps [`ActivitySeverity`] onto its own `LogLevel`; this
//! module stays free of `tui` imports (layering).
//!
//! Tracing: [`InteractionActivity::emit_tracing`] mirrors each line into the
//! structured log. The lifecycle *spans* (`interaction.launch`,
//! `interaction.turn`, `interaction.terminator`, `interaction.teardown`)
//! live at the call sites that own the work — this enum only carries the
//! human-readable transition lines.

use std::path::PathBuf;

/// Severity of an activity line, mapped to the TUI's `LogLevel` at the edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivitySeverity {
    Info,
    Warn,
}

/// Compact rendering of `CloseReason` for the closing line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseReasonSummary {
    PrCreated { pr_number: u64 },
    UserQuit,
    AgentFailure,
}

impl std::fmt::Display for CloseReasonSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PrCreated { pr_number } => write!(f, "PrCreated #{pr_number}"),
            Self::UserQuit => write!(f, "UserQuit"),
            Self::AgentFailure => write!(f, "AgentFailure"),
        }
    }
}

/// One INTERACTION lifecycle transition (#742). `message()` is the pinned
/// dashboard line; `tag()`/`severity()` pick the label and level.
#[derive(Debug, Clone, PartialEq)]
pub enum InteractionActivity {
    /// Session launched fresh from the dialog. `transport` is the
    /// claude-transport discriminator (v0.30.5 coupling): "headless" or
    /// "interactive".
    Launched {
        issue: u64,
        produce_pr: bool,
        transport: String,
    },
    /// Re-entered a live session from the Issues list.
    Resumed {
        issue: u64,
    },
    /// One conversational turn finished streaming.
    TurnComplete {
        issue: u64,
        turn_index: usize,
        chunk_count: usize,
        duration_ms: i64,
    },
    /// A turn failed (spawn failure, non-zero exit, stream error).
    TurnFailed {
        issue: u64,
        detail: String,
    },
    /// Terminator fired — the session is closing.
    Closing {
        issue: u64,
        reason: CloseReasonSummary,
    },
    TeardownOk {
        issue: u64,
        path: PathBuf,
    },
    TeardownFail {
        issue: u64,
        path: PathBuf,
        error: String,
    },
    /// Teardown intentionally not run (no isolated worktree / pre-closed).
    TeardownSkipped {
        issue: u64,
        why: String,
    },
}

impl InteractionActivity {
    /// Dashboard label: `INTERACTION` for lifecycle lines, `TEARDOWN` for the
    /// destructive-cleanup result lines.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::TeardownOk { .. } | Self::TeardownFail { .. } | Self::TeardownSkipped { .. } => {
                "TEARDOWN"
            }
            _ => "INTERACTION",
        }
    }

    pub fn severity(&self) -> ActivitySeverity {
        match self {
            Self::TurnFailed { .. } | Self::TeardownFail { .. } => ActivitySeverity::Warn,
            _ => ActivitySeverity::Info,
        }
    }

    /// The pinned, human-readable dashboard line (spec §7 formats).
    pub fn message(&self) -> String {
        match self {
            Self::Launched {
                issue,
                produce_pr,
                transport,
            } => format!(
                "#{issue} launched (mode: produce_pr={produce_pr}, interaction=true, transport={transport})"
            ),
            Self::Resumed { issue } => format!("#{issue} resumed"),
            Self::TurnComplete {
                issue,
                turn_index,
                chunk_count,
                duration_ms,
            } => format!(
                "#{issue} turn {turn_index}: {chunk_count} chunks streamed ({duration_ms} ms)"
            ),
            Self::TurnFailed { issue, detail } => format!("#{issue} turn failed: {detail}"),
            Self::Closing { issue, reason } => {
                format!("#{issue} closing (reason: {reason}); wiping worktree")
            }
            Self::TeardownOk { issue, path } => {
                format!(
                    "#{issue} worktree removed at {}; branch deleted",
                    path.display()
                )
            }
            Self::TeardownFail { issue, path, error } => format!(
                "#{issue} worktree teardown FAILED: {error}; worktree kept at {}",
                path.display()
            ),
            Self::TeardownSkipped { issue, why } => format!("#{issue} {why}; teardown skipped"),
        }
    }

    /// Mirror the transition into the structured log with stable fields.
    pub fn emit_tracing(&self) {
        match self {
            Self::Launched {
                issue,
                produce_pr,
                transport,
            } => tracing::info!(issue, produce_pr, transport, "interaction launched"),
            Self::Resumed { issue } => tracing::info!(issue, "interaction resumed"),
            Self::TurnComplete {
                issue,
                turn_index,
                chunk_count,
                duration_ms,
            } => tracing::info!(
                issue,
                turn_index,
                chunk_count,
                duration_ms,
                "interaction turn complete"
            ),
            Self::TurnFailed { issue, detail } => {
                tracing::warn!(issue, detail, "interaction turn failed")
            }
            Self::Closing { issue, reason } => {
                tracing::info!(issue, reason = %reason, "interaction closing")
            }
            Self::TeardownOk { issue, path } => {
                tracing::info!(issue, path = %path.display(), outcome = "ok", "interaction teardown")
            }
            Self::TeardownFail { issue, path, error } => {
                tracing::warn!(issue, path = %path.display(), error, outcome = "failed", "interaction teardown")
            }
            Self::TeardownSkipped { issue, why } => {
                tracing::info!(issue, why, outcome = "skipped", "interaction teardown")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launched_line_is_pinned() {
        let line = InteractionActivity::Launched {
            issue: 42,
            produce_pr: true,
            transport: "interactive".into(),
        }
        .message();
        assert_eq!(
            line,
            "#42 launched (mode: produce_pr=true, interaction=true, transport=interactive)"
        );
    }

    #[test]
    fn resumed_line_is_pinned() {
        assert_eq!(
            InteractionActivity::Resumed { issue: 42 }.message(),
            "#42 resumed"
        );
    }

    #[test]
    fn turn_complete_line_is_pinned() {
        let line = InteractionActivity::TurnComplete {
            issue: 42,
            turn_index: 3,
            chunk_count: 17,
            duration_ms: 850,
        }
        .message();
        assert_eq!(line, "#42 turn 3: 17 chunks streamed (850 ms)");
    }

    #[test]
    fn turn_failed_line_is_pinned_and_warn() {
        let activity = InteractionActivity::TurnFailed {
            issue: 42,
            detail: "agent exit 1".into(),
        };
        assert_eq!(activity.message(), "#42 turn failed: agent exit 1");
        assert_eq!(activity.severity(), ActivitySeverity::Warn);
    }

    #[test]
    fn closing_line_is_pinned() {
        let line = InteractionActivity::Closing {
            issue: 42,
            reason: CloseReasonSummary::PrCreated { pr_number: 7 },
        }
        .message();
        assert_eq!(line, "#42 closing (reason: PrCreated #7); wiping worktree");
    }

    #[test]
    fn teardown_ok_line_is_pinned_with_teardown_tag() {
        let activity = InteractionActivity::TeardownOk {
            issue: 42,
            path: PathBuf::from("/tmp/maestro/issue-42"),
        };
        assert_eq!(activity.tag(), "TEARDOWN");
        assert_eq!(
            activity.message(),
            "#42 worktree removed at /tmp/maestro/issue-42; branch deleted"
        );
    }

    #[test]
    fn teardown_fail_line_is_pinned_warn_with_teardown_tag() {
        let activity = InteractionActivity::TeardownFail {
            issue: 42,
            path: PathBuf::from("/tmp/maestro/issue-42"),
            error: "path still exists".into(),
        };
        assert_eq!(activity.tag(), "TEARDOWN");
        assert_eq!(activity.severity(), ActivitySeverity::Warn);
        assert_eq!(
            activity.message(),
            "#42 worktree teardown FAILED: path still exists; worktree kept at /tmp/maestro/issue-42"
        );
    }

    #[test]
    fn teardown_skipped_line_is_pinned() {
        let activity = InteractionActivity::TeardownSkipped {
            issue: 42,
            why: "no isolated worktree".into(),
        };
        assert_eq!(activity.tag(), "TEARDOWN");
        assert_eq!(
            activity.message(),
            "#42 no isolated worktree; teardown skipped"
        );
    }
}
