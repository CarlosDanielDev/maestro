use super::types::{Session, SessionStatus};
use crate::state::progress::SessionProgress;

#[derive(Debug, Clone, PartialEq)]
pub enum ForkReason {
    ContextOverflow { context_pct: f64 },
}

#[derive(Debug)]
pub enum ForkResult {
    Forked {
        child: Box<Session>,
        continuation_prompt: String,
    },
    Denied {
        reason: String,
    },
}

/// Trait for session forking, enabling mock injection in tests.
pub trait SessionForker: Send {
    fn prepare_fork(
        &self,
        parent: &Session,
        progress: Option<&SessionProgress>,
        fork_reason: ForkReason,
    ) -> ForkResult;
    fn can_fork(&self, session: &Session) -> bool;
}

pub struct ForkPolicy {
    pub max_fork_depth: u8,
}

impl ForkPolicy {
    pub fn new(max_fork_depth: u8) -> Self {
        Self { max_fork_depth }
    }
}

impl SessionForker for ForkPolicy {
    fn can_fork(&self, session: &Session) -> bool {
        if session.status.is_terminal() {
            return false;
        }
        if matches!(
            session.status,
            SessionStatus::Queued | SessionStatus::Spawning
        ) {
            return false;
        }
        session.fork_depth < self.max_fork_depth
    }

    fn prepare_fork(
        &self,
        parent: &Session,
        progress: Option<&SessionProgress>,
        fork_reason: ForkReason,
    ) -> ForkResult {
        if !self.can_fork(parent) {
            return ForkResult::Denied {
                reason: format!("Max fork depth {} reached", self.max_fork_depth),
            };
        }

        let continuation = build_continuation_prompt(parent, progress, &fork_reason);

        let mut child = Session::new(
            format!(
                "{}\n\n--- FORK CONTEXT ---\n{}",
                parent.prompt, continuation
            ),
            parent.model.clone(),
            parent.mode.clone(),
            parent.issue_number,
        );
        child.parent_session_id = Some(parent.id);
        child.fork_depth = parent.fork_depth + 1;
        child.issue_title = parent.issue_title.clone();

        ForkResult::Forked {
            child: Box::new(child),
            continuation_prompt: continuation,
        }
    }
}

fn build_continuation_prompt(
    parent: &Session,
    progress: Option<&SessionProgress>,
    reason: &ForkReason,
) -> String {
    let mut prompt = format!(
        "This session is a continuation of session {} due to context overflow.\n\
         Please continue from where the previous session left off.",
        parent.id,
    );

    match reason {
        ForkReason::ContextOverflow { context_pct } => {
            prompt.push_str(&format!(
                "\nContext window usage reached {:.0}%.",
                context_pct * 100.0
            ));
        }
    }

    if let Some(prog) = progress {
        prompt.push_str(&format!("\nPrevious session phase: {}", prog.phase.label()));
        if !prog.files_at_checkpoint.is_empty() {
            prompt.push_str(&format!(
                "\nFiles modified: {}",
                prog.files_at_checkpoint.join(", ")
            ));
        }
        prompt.push_str(&format!("\nTools used: {}", prog.tools_used_count));
    }

    if !parent.files_touched.is_empty() {
        prompt.push_str(&format!(
            "\nAll files touched: {}",
            parent.files_touched.join(", ")
        ));
    }

    prompt.push_str(
        "\n\nIMPORTANT: Review the files already modified by the previous session \
         to understand what has been done. Then continue with the remaining work. \
         Do NOT redo work that has already been completed.",
    );

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::progress::{SessionPhase, SessionProgress};

    fn make_session(status: SessionStatus, fork_depth: u8) -> Session {
        let mut s = Session::new(
            "implement feature X".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(42),
        );
        s.status = status;
        s.fork_depth = fork_depth;
        s
    }

    #[test]
    fn can_fork_true_for_running_session_under_max_depth() {
        let policy = ForkPolicy::new(5);
        let session = make_session(SessionStatus::Running, 0);
        assert!(policy.can_fork(&session));
    }

    #[test]
    fn can_fork_false_at_max_depth() {
        let policy = ForkPolicy::new(5);
        let session = make_session(SessionStatus::Running, 5);
        assert!(!policy.can_fork(&session));
    }

    #[test]
    fn can_fork_false_for_completed_session() {
        let policy = ForkPolicy::new(5);
        let session = make_session(SessionStatus::Completed, 0);
        assert!(!policy.can_fork(&session));
    }

    #[test]
    fn can_fork_false_for_errored_session() {
        let policy = ForkPolicy::new(5);
        let session = make_session(SessionStatus::Errored, 0);
        assert!(!policy.can_fork(&session));
    }

    #[test]
    fn can_fork_false_for_killed_session() {
        let policy = ForkPolicy::new(5);
        let session = make_session(SessionStatus::Killed, 0);
        assert!(!policy.can_fork(&session));
    }

    #[test]
    fn can_fork_false_for_queued_session() {
        let policy = ForkPolicy::new(5);
        let session = make_session(SessionStatus::Queued, 0);
        assert!(!policy.can_fork(&session));
    }

    #[test]
    fn can_fork_false_for_spawning_session() {
        let policy = ForkPolicy::new(5);
        let session = make_session(SessionStatus::Spawning, 0);
        assert!(!policy.can_fork(&session));
    }

    #[test]
    fn fork_policy_zero_max_depth_blocks_all_forks() {
        let policy = ForkPolicy::new(0);
        let session = make_session(SessionStatus::Running, 0);
        assert!(!policy.can_fork(&session));
    }

    #[test]
    fn prepare_fork_creates_child_with_parent_id() {
        let policy = ForkPolicy::new(5);
        let parent = make_session(SessionStatus::Running, 0);
        let result = policy.prepare_fork(
            &parent,
            None,
            ForkReason::ContextOverflow { context_pct: 0.75 },
        );
        match result {
            ForkResult::Forked { child, .. } => {
                assert_eq!(child.parent_session_id, Some(parent.id));
            }
            ForkResult::Denied { reason } => panic!("Fork denied: {}", reason),
        }
    }

    #[test]
    fn prepare_fork_increments_fork_depth() {
        let policy = ForkPolicy::new(5);
        let parent = make_session(SessionStatus::Running, 2);
        let result = policy.prepare_fork(
            &parent,
            None,
            ForkReason::ContextOverflow { context_pct: 0.75 },
        );
        match result {
            ForkResult::Forked { child, .. } => assert_eq!(child.fork_depth, 3),
            ForkResult::Denied { reason } => panic!("Fork denied: {}", reason),
        }
    }

    #[test]
    fn prepare_fork_child_starts_queued() {
        let policy = ForkPolicy::new(5);
        let parent = make_session(SessionStatus::Running, 0);
        let result = policy.prepare_fork(
            &parent,
            None,
            ForkReason::ContextOverflow { context_pct: 0.75 },
        );
        match result {
            ForkResult::Forked { child, .. } => {
                assert_eq!(child.status, SessionStatus::Queued);
            }
            ForkResult::Denied { reason } => panic!("Fork denied: {}", reason),
        }
    }

    #[test]
    fn prepare_fork_continuation_prompt_contains_parent_id() {
        let policy = ForkPolicy::new(5);
        let parent = make_session(SessionStatus::Running, 0);
        let parent_id_str = parent.id.to_string();
        let result = policy.prepare_fork(
            &parent,
            None,
            ForkReason::ContextOverflow { context_pct: 0.75 },
        );
        match result {
            ForkResult::Forked {
                continuation_prompt,
                ..
            } => {
                assert!(continuation_prompt.contains(&parent_id_str));
            }
            ForkResult::Denied { reason } => panic!("Fork denied: {}", reason),
        }
    }

    #[test]
    fn prepare_fork_continuation_prompt_includes_files_when_progress_provided() {
        let policy = ForkPolicy::new(5);
        let parent = make_session(SessionStatus::Running, 0);
        let mut progress = SessionProgress::new();
        progress.phase = SessionPhase::Implementing;
        progress.files_at_checkpoint = vec!["src/lib.rs".into()];
        let result = policy.prepare_fork(
            &parent,
            Some(&progress),
            ForkReason::ContextOverflow { context_pct: 0.75 },
        );
        match result {
            ForkResult::Forked {
                continuation_prompt,
                ..
            } => {
                assert!(continuation_prompt.contains("src/lib.rs"));
                assert!(continuation_prompt.contains("IMPLEMENTING"));
            }
            ForkResult::Denied { reason } => panic!("Fork denied: {}", reason),
        }
    }

    #[test]
    fn prepare_fork_continuation_prompt_includes_fork_reason() {
        let policy = ForkPolicy::new(5);
        let parent = make_session(SessionStatus::Running, 0);
        let result = policy.prepare_fork(
            &parent,
            None,
            ForkReason::ContextOverflow { context_pct: 0.75 },
        );
        match result {
            ForkResult::Forked {
                continuation_prompt,
                ..
            } => {
                assert!(continuation_prompt.contains("context overflow"));
            }
            ForkResult::Denied { reason } => panic!("Fork denied: {}", reason),
        }
    }

    #[test]
    fn prepare_fork_preserves_issue_number() {
        let policy = ForkPolicy::new(5);
        let parent = make_session(SessionStatus::Running, 0);
        let result = policy.prepare_fork(
            &parent,
            None,
            ForkReason::ContextOverflow { context_pct: 0.75 },
        );
        match result {
            ForkResult::Forked { child, .. } => {
                assert_eq!(child.issue_number, Some(42));
            }
            ForkResult::Denied { reason } => panic!("Fork denied: {}", reason),
        }
    }

    #[test]
    fn prepare_fork_preserves_model_and_mode() {
        let policy = ForkPolicy::new(5);
        let parent = make_session(SessionStatus::Running, 0);
        let result = policy.prepare_fork(
            &parent,
            None,
            ForkReason::ContextOverflow { context_pct: 0.75 },
        );
        match result {
            ForkResult::Forked { child, .. } => {
                assert_eq!(child.model, "opus");
                assert_eq!(child.mode, "orchestrator");
            }
            ForkResult::Denied { reason } => panic!("Fork denied: {}", reason),
        }
    }

    #[test]
    fn prepare_fork_denied_at_max_depth() {
        let policy = ForkPolicy::new(2);
        let parent = make_session(SessionStatus::Running, 2);
        let result = policy.prepare_fork(
            &parent,
            None,
            ForkReason::ContextOverflow { context_pct: 0.75 },
        );
        assert!(matches!(result, ForkResult::Denied { .. }));
    }
}
