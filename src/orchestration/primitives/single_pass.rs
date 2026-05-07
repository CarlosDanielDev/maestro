use super::{
    MachineState, NextStep, PrimitiveMachine, PrimitiveOutput, dispatch, malformed,
    subagent_failure,
};
use crate::orchestration::contracts::{SubagentError, SubagentResult};
use crate::orchestration::types::TeamRole;

#[derive(Debug, Clone)]
pub struct SinglePassMachine {
    issue_number: u64,
    state: MachineState,
}

impl SinglePassMachine {
    pub fn new(issue_number: u64) -> Self {
        Self {
            issue_number,
            state: MachineState::Dispatching,
        }
    }
}

impl PrimitiveMachine for SinglePassMachine {
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
            MachineState::Dispatching => {
                self.state = MachineState::Waiting(TeamRole::Reviewer);
                dispatch(
                    TeamRole::Reviewer,
                    format!(
                        "Perform a single-pass review for issue #{}.",
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
        match result {
            Ok(result @ SubagentResult::ReviewFindings { .. })
            | Ok(result @ SubagentResult::Generic { .. }) => {
                self.state = MachineState::Done(PrimitiveOutput::SinglePass { role, result });
            }
            Ok(got) => {
                self.state =
                    MachineState::Failed(malformed(role, "review-findings or generic", &got));
            }
            Err(err) => self.state = MachineState::Failed(subagent_failure(role, err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::contracts::ReviewVerdict;

    fn review() -> SubagentResult {
        SubagentResult::ReviewFindings {
            verdict: ReviewVerdict::Comment,
            findings: vec![],
        }
    }

    #[test]
    fn single_pass_success_path() {
        let mut machine = SinglePassMachine::new(1);
        assert!(matches!(
            machine.next(),
            NextStep::Dispatch {
                role: TeamRole::Reviewer,
                ..
            }
        ));
        machine.advance(TeamRole::Reviewer, Ok(review()));
        assert!(matches!(machine.next(), NextStep::Done { .. }));
    }

    #[test]
    fn single_pass_failure_path() {
        let mut machine = SinglePassMachine::new(1);
        let _ = machine.next();
        machine.advance(
            TeamRole::Reviewer,
            Err(SubagentError::SubagentReported("nope".into())),
        );
        assert!(matches!(machine.next(), NextStep::Fail { reason } if reason.contains("nope")));
    }

    #[test]
    fn single_pass_malformed_result_path() {
        let mut machine = SinglePassMachine::new(1);
        let _ = machine.next();
        machine.advance(
            TeamRole::Reviewer,
            Ok(SubagentResult::DocsChange {
                files_touched: vec![],
                summary: "wrong".into(),
            }),
        );
        assert!(
            matches!(machine.next(), NextStep::Fail { reason } if reason.contains("malformed"))
        );
    }
}
