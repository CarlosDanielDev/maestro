use crate::orchestration::{SubagentError, SubagentResult, TeamRole};
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub(crate) struct MockResponse {
    pub role: TeamRole,
    pub result: Result<SubagentResult, SubagentError>,
}

#[derive(Debug, Default)]
pub(crate) struct MockTaskQueue {
    responses: Mutex<VecDeque<MockResponse>>,
}

impl MockTaskQueue {
    pub(crate) fn new(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }

    pub(crate) fn dispatch(
        &self,
        role: TeamRole,
        _instructions: &str,
    ) -> Result<SubagentResult, SubagentError> {
        let mut responses = self
            .responses
            .lock()
            .map_err(|_| SubagentError::Other("mock queue lock poisoned".into()))?;
        let response = responses
            .pop_front()
            .ok_or_else(|| SubagentError::Other("mock queue exhausted".into()))?;
        if response.role != role {
            return Err(SubagentError::ResultShapeMismatch {
                role,
                expected: format!("{:?}", role.allowed_results()),
                got: format!("response for {:?}", response.role),
            });
        }
        response.result
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.responses.lock().map(|r| r.is_empty()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::ReviewVerdict;

    fn code_change() -> SubagentResult {
        SubagentResult::CodeChange {
            files_touched: vec!["src/lib.rs".into()],
            summary: "implemented".into(),
            commit_sha: None,
        }
    }

    #[test]
    fn mock_queue_returns_canned_result() {
        let queue = MockTaskQueue::new(vec![MockResponse {
            role: TeamRole::Implementer,
            result: Ok(code_change()),
        }]);
        assert!(matches!(
            queue.dispatch(TeamRole::Implementer, "do work"),
            Ok(SubagentResult::CodeChange { .. })
        ));
        assert!(queue.is_empty());
    }

    #[test]
    fn mock_queue_surfaces_failure() {
        let queue = MockTaskQueue::new(vec![MockResponse {
            role: TeamRole::Reviewer,
            result: Err(SubagentError::Provider("provider down".into())),
        }]);
        assert!(matches!(
            queue.dispatch(TeamRole::Reviewer, "review"),
            Err(SubagentError::Provider(message)) if message == "provider down"
        ));
    }

    #[test]
    fn mock_queue_rejects_unexpected_role() {
        let queue = MockTaskQueue::new(vec![MockResponse {
            role: TeamRole::Reviewer,
            result: Ok(SubagentResult::ReviewFindings {
                verdict: ReviewVerdict::Approved,
                findings: vec![],
            }),
        }]);
        assert!(matches!(
            queue.dispatch(TeamRole::Implementer, "implement"),
            Err(SubagentError::ResultShapeMismatch { .. })
        ));
    }
}
