use super::{
    MachineState, NextStep, PrimitiveMachine, PrimitiveOutput, dispatch, malformed, review_summary,
    subagent_failure,
};
use crate::orchestration::contracts::{ReviewVerdict, SubagentError, SubagentResult};
use crate::orchestration::types::TeamRole;

#[derive(Debug, Clone)]
pub struct PipelineMachine {
    issue_number: u64,
    state: MachineState,
    code_summary: Option<String>,
    review_summary: Option<String>,
}

impl PipelineMachine {
    pub fn new(issue_number: u64) -> Self {
        Self {
            issue_number,
            state: MachineState::Dispatching,
            code_summary: None,
            review_summary: None,
        }
    }
}

impl PrimitiveMachine for PipelineMachine {
    fn next(&mut self) -> NextStep {
        if let Some(step) = self.state.next_terminal(|role| {
            format!(
                "Awaiting existing {role:?} task for issue #{}.",
                self.issue_number
            )
        }) {
            return step;
        }

        match &self.state {
            MachineState::Dispatching if self.code_summary.is_none() => {
                self.state = MachineState::Waiting(TeamRole::Implementer);
                dispatch(
                    TeamRole::Implementer,
                    format!(
                        "Implement issue #{} and return CodeChange.",
                        self.issue_number
                    ),
                )
            }
            MachineState::Dispatching if self.review_summary.is_none() => {
                self.state = MachineState::Waiting(TeamRole::Reviewer);
                let summary = self
                    .code_summary
                    .as_deref()
                    .unwrap_or("code change complete");
                dispatch(
                    TeamRole::Reviewer,
                    format!(
                        "Review issue #{} using this summary only: {summary}. Return ReviewFindings.",
                        self.issue_number
                    ),
                )
            }
            MachineState::Dispatching => {
                self.state = MachineState::Waiting(TeamRole::Docs);
                let code = self
                    .code_summary
                    .as_deref()
                    .unwrap_or("code change complete");
                let review = self.review_summary.as_deref().unwrap_or("review complete");
                dispatch(
                    TeamRole::Docs,
                    format!(
                        "Update docs for issue #{} using summaries only. Code: {code}. Review: {review}. Return DocsChange.",
                        self.issue_number
                    ),
                )
            }
            MachineState::Waiting(_) | MachineState::Done(_) | MachineState::Failed(_) => {
                unreachable!("terminal states returned above")
            }
        }
    }

    fn advance(&mut self, role: TeamRole, result: Result<SubagentResult, SubagentError>) {
        if !self.state.take_waiting(role) {
            return;
        }
        let result = match result {
            Ok(result) => result,
            Err(err) => {
                self.state = MachineState::Failed(subagent_failure(role, err));
                return;
            }
        };
        match (role, result) {
            (
                TeamRole::Implementer,
                SubagentResult::CodeChange {
                    summary,
                    files_touched: _,
                    commit_sha: _,
                },
            ) => {
                self.code_summary = Some(summary);
                self.state = MachineState::Dispatching;
            }
            (
                TeamRole::Reviewer,
                result @ SubagentResult::ReviewFindings {
                    verdict,
                    findings: _,
                },
            ) => {
                if verdict == ReviewVerdict::RequestChanges {
                    self.state = MachineState::Failed("review requested changes".into());
                } else if let Some(summary) = review_summary(&result) {
                    self.review_summary = Some(summary);
                    self.state = MachineState::Dispatching;
                }
            }
            (
                TeamRole::Docs,
                SubagentResult::DocsChange {
                    summary,
                    files_touched: _,
                },
            ) => {
                self.state = MachineState::Done(PrimitiveOutput::Pipeline {
                    code_summary: self.code_summary.clone().unwrap_or_default(),
                    review_summary: self.review_summary.clone().unwrap_or_default(),
                    docs_summary: summary,
                });
            }
            (role, got) => {
                self.state = MachineState::Failed(malformed(role, role.allowed_results()[0], &got));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(summary: &str) -> SubagentResult {
        SubagentResult::CodeChange {
            files_touched: vec!["src/lib.rs".into()],
            summary: summary.into(),
            commit_sha: None,
        }
    }

    fn review(verdict: ReviewVerdict) -> SubagentResult {
        SubagentResult::ReviewFindings {
            verdict,
            findings: vec![],
        }
    }

    fn docs(summary: &str) -> SubagentResult {
        SubagentResult::DocsChange {
            files_touched: vec!["README.md".into()],
            summary: summary.into(),
        }
    }

    #[test]
    fn pipeline_success_path() {
        let mut machine = PipelineMachine::new(662);
        assert!(matches!(
            machine.next(),
            NextStep::Dispatch {
                role: TeamRole::Implementer,
                ..
            }
        ));
        machine.advance(TeamRole::Implementer, Ok(code("implemented")));
        assert!(matches!(
            machine.next(),
            NextStep::Dispatch {
                role: TeamRole::Reviewer,
                ..
            }
        ));
        machine.advance(TeamRole::Reviewer, Ok(review(ReviewVerdict::Approved)));
        assert!(matches!(
            machine.next(),
            NextStep::Dispatch {
                role: TeamRole::Docs,
                ..
            }
        ));
        machine.advance(TeamRole::Docs, Ok(docs("documented")));
        assert!(matches!(machine.next(), NextStep::Done { .. }));
    }

    #[test]
    fn pipeline_failure_path() {
        let mut machine = PipelineMachine::new(662);
        let _ = machine.next();
        machine.advance(
            TeamRole::Implementer,
            Err(SubagentError::Provider("boom".into())),
        );
        assert!(matches!(machine.next(), NextStep::Fail { reason } if reason.contains("boom")));
    }

    #[test]
    fn pipeline_malformed_result_path() {
        let mut machine = PipelineMachine::new(662);
        let _ = machine.next();
        machine.advance(TeamRole::Implementer, Ok(review(ReviewVerdict::Approved)));
        assert!(
            matches!(machine.next(), NextStep::Fail { reason } if reason.contains("malformed"))
        );
    }
}
