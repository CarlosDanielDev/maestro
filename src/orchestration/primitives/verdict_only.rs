use super::{
    MachineState, NextStep, PrimitiveMachine, PrimitiveOutput, dispatch, malformed,
    subagent_failure,
};
use crate::orchestration::contracts::{SubagentError, SubagentResult};
use crate::orchestration::types::TeamRole;

#[derive(Debug, Clone)]
pub struct VerdictOnlyMachine {
    issue_number: u64,
    state: MachineState,
}

impl VerdictOnlyMachine {
    pub fn new(issue_number: u64) -> Self {
        Self {
            issue_number,
            state: MachineState::Dispatching,
        }
    }
}

impl PrimitiveMachine for VerdictOnlyMachine {
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
                self.state = MachineState::Waiting(TeamRole::Triager);
                dispatch(
                    TeamRole::Triager,
                    format!(
                        "Produce a verdict for issue #{} without code changes.",
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
            Ok(SubagentResult::Verdict {
                decision,
                rationale,
                new_issues,
            }) => {
                self.state = MachineState::Done(PrimitiveOutput::Verdict {
                    decision,
                    rationale,
                    new_issues,
                });
            }
            Ok(got) => self.state = MachineState::Failed(malformed(role, "verdict", &got)),
            Err(err) => self.state = MachineState::Failed(subagent_failure(role, err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict() -> SubagentResult {
        SubagentResult::Verdict {
            decision: "promote".into(),
            rationale: "clear value".into(),
            new_issues: vec![],
        }
    }

    #[test]
    fn verdict_only_success_path() {
        let mut machine = VerdictOnlyMachine::new(3);
        assert!(matches!(
            machine.next(),
            NextStep::Dispatch {
                role: TeamRole::Triager,
                ..
            }
        ));
        machine.advance(TeamRole::Triager, Ok(verdict()));
        assert!(matches!(machine.next(), NextStep::Done { .. }));
    }

    #[test]
    fn verdict_only_failure_path() {
        let mut machine = VerdictOnlyMachine::new(3);
        let _ = machine.next();
        machine.advance(
            TeamRole::Triager,
            Err(SubagentError::Timeout { seconds: 1 }),
        );
        assert!(
            matches!(machine.next(), NextStep::Fail { reason } if reason.contains("timed out"))
        );
    }

    #[test]
    fn verdict_only_malformed_result_path() {
        let mut machine = VerdictOnlyMachine::new(3);
        let _ = machine.next();
        machine.advance(
            TeamRole::Triager,
            Ok(SubagentResult::Generic {
                json: serde_json::json!({}),
            }),
        );
        assert!(
            matches!(machine.next(), NextStep::Fail { reason } if reason.contains("malformed"))
        );
    }
}
