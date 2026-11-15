//! Integration tests for the full session lifecycle.
//!
//! No process spawning. Events are injected via handle_event().
//! All external dependencies are replaced with in-process mocks.

#[cfg(test)]
mod adapt_pipeline;
#[cfg(test)]
mod adopt_pipeline;
#[cfg(test)]
mod build_hygiene;
#[cfg(test)]
mod canonical_command_specs;
#[cfg(test)]
mod completion_pipeline;
#[cfg(test)]
mod concurrent_sessions;
#[cfg(test)]
mod docs_gen;
#[cfg(test)]
mod doctor_run_health_check;
#[cfg(test)]
mod init;
#[cfg(test)]
mod orchestration_dispatch;
#[cfg(test)]
mod orchestration_dispatch_routing;
#[cfg(test)]
mod orchestration_mock_task;
#[cfg(test)]
mod orchestration_pipeline;
#[cfg(test)]
mod orchestration_smoke;
#[cfg(test)]
mod session_lifecycle;
#[cfg(test)]
mod stream_parsing;
#[cfg(test)]
mod templates_render;
#[cfg(test)]
mod templates_runtime;
#[cfg(test)]
mod upgrade;
#[cfg(test)]
mod worktree_lifecycle;

/// Shared test helpers used across integration test modules.
#[cfg(test)]
mod helpers {
    use tokio::sync::mpsc;

    use crate::provider::types::Issue;
    use crate::session::pool::SessionPool;
    use crate::session::types::Session;
    use crate::session::worktree::MockWorktreeManager;

    pub fn make_pool(max: usize) -> SessionPool {
        let (tx, _rx) = mpsc::unbounded_channel();
        SessionPool::new(max, Box::new(MockWorktreeManager::new()), tx)
    }

    pub fn make_pool_with_worktree(max: usize, wt: MockWorktreeManager) -> SessionPool {
        let (tx, _rx) = mpsc::unbounded_channel();
        SessionPool::new(max, Box::new(wt), tx)
    }

    pub fn make_session(prompt: &str) -> Session {
        Session::new(
            prompt.to_string(),
            "opus".to_string(),
            "orchestrator".to_string(),
            None,
            None,
        )
    }

    /// Create a session pre-set to Running status (for event handling tests).
    pub fn make_running_session(prompt: &str) -> Session {
        let mut s = make_session(prompt);
        s.status = crate::session::types::SessionStatus::Running;
        s.started_at = Some(chrono::Utc::now());
        s
    }

    pub fn make_session_with_issue(issue: u64) -> Session {
        Session::new(
            format!("work on issue {}", issue),
            "opus".to_string(),
            "orchestrator".to_string(),
            Some(issue),
            None,
        )
    }

    /// Create a session with an issue number pre-set to Running status.
    pub fn make_running_session_with_issue(issue: u64) -> Session {
        let mut s = make_session_with_issue(issue);
        s.status = crate::session::types::SessionStatus::Running;
        s.started_at = Some(chrono::Utc::now());
        s
    }

    pub fn make_gh_issue(number: u64) -> Issue {
        Issue {
            number,
            title: format!("Implement feature #{}", number),
            body: String::new(),
            labels: vec!["maestro:in-progress".to_string()],
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{}", number),
            milestone: None,
            assignees: vec![],
        }
    }
}
