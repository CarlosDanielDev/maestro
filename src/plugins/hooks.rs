use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Hook points where plugins can execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    SessionStarted,
    SessionCompleted,
    TestsPassed,
    TestsFailed,
    BudgetThreshold,
    FileConflict,
    PrCreated,
    ContextOverflow,
}

impl HookPoint {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SessionStarted => "session_started",
            Self::SessionCompleted => "session_completed",
            Self::TestsPassed => "tests_passed",
            Self::TestsFailed => "tests_failed",
            Self::BudgetThreshold => "budget_threshold",
            Self::FileConflict => "file_conflict",
            Self::PrCreated => "pr_created",
            Self::ContextOverflow => "context_overflow",
        }
    }

    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "session_started" => Some(Self::SessionStarted),
            "session_completed" => Some(Self::SessionCompleted),
            "tests_passed" => Some(Self::TestsPassed),
            "tests_failed" => Some(Self::TestsFailed),
            "budget_threshold" => Some(Self::BudgetThreshold),
            "file_conflict" => Some(Self::FileConflict),
            "pr_created" => Some(Self::PrCreated),
            "context_overflow" => Some(Self::ContextOverflow),
            _ => None,
        }
    }
}

/// Context passed to plugins via environment variables.
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub vars: HashMap<String, String>,
}

impl HookContext {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    pub fn with_session(mut self, session_id: &str, issue_number: Option<u64>) -> Self {
        self.vars
            .insert("MAESTRO_SESSION_ID".into(), session_id.into());
        if let Some(num) = issue_number {
            self.vars
                .insert("MAESTRO_ISSUE_NUMBER".into(), num.to_string());
        }
        self
    }

    pub fn with_cost(mut self, cost_usd: f64) -> Self {
        self.vars
            .insert("MAESTRO_COST_USD".into(), format!("{:.2}", cost_usd));
        self
    }

    pub fn with_pr(mut self, pr_number: u64) -> Self {
        self.vars
            .insert("MAESTRO_PR_NUMBER".into(), pr_number.to_string());
        self
    }

    pub fn with_branch(mut self, branch: &str) -> Self {
        self.vars.insert("MAESTRO_BRANCH".into(), branch.into());
        self
    }

    pub fn with_files(mut self, files: &[String]) -> Self {
        self.vars.insert("MAESTRO_FILES".into(), files.join(","));
        self
    }

    pub fn with_var(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_point_round_trips() {
        let points = [
            HookPoint::SessionStarted,
            HookPoint::SessionCompleted,
            HookPoint::TestsPassed,
            HookPoint::TestsFailed,
            HookPoint::BudgetThreshold,
            HookPoint::FileConflict,
            HookPoint::PrCreated,
            HookPoint::ContextOverflow,
        ];
        for point in points {
            assert_eq!(HookPoint::from_str(point.as_str()), Some(point));
        }
    }

    #[test]
    fn hook_point_from_str_unknown() {
        assert_eq!(HookPoint::from_str("unknown"), None);
    }

    #[test]
    fn hook_context_builds_env_vars() {
        let ctx = HookContext::new()
            .with_session("abc-123", Some(42))
            .with_cost(3.50)
            .with_pr(99)
            .with_branch("maestro/issue-42")
            .with_files(&["src/main.rs".into(), "src/lib.rs".into()]);

        assert_eq!(ctx.vars["MAESTRO_SESSION_ID"], "abc-123");
        assert_eq!(ctx.vars["MAESTRO_ISSUE_NUMBER"], "42");
        assert_eq!(ctx.vars["MAESTRO_COST_USD"], "3.50");
        assert_eq!(ctx.vars["MAESTRO_PR_NUMBER"], "99");
        assert_eq!(ctx.vars["MAESTRO_BRANCH"], "maestro/issue-42");
        assert_eq!(ctx.vars["MAESTRO_FILES"], "src/main.rs,src/lib.rs");
    }
}
