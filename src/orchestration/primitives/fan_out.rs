use super::{
    MachineState, NextStep, PrimitiveMachine, PrimitiveOutput, done, fail, malformed,
    role_mismatch, subagent_failure,
};
use crate::orchestration::contracts::{SubagentError, SubagentResult};
use crate::orchestration::types::TeamRole;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct FanOutMachine {
    issue_number: u64,
    state: MachineState,
    queue: VecDeque<TeamRole>,
    outstanding: Vec<TeamRole>,
    results: Vec<(TeamRole, SubagentResult)>,
}

impl FanOutMachine {
    pub fn new(issue_number: u64) -> Self {
        Self {
            issue_number,
            state: MachineState::Dispatching,
            queue: VecDeque::from([TeamRole::Implementer, TeamRole::Reviewer, TeamRole::Docs]),
            outstanding: Vec::new(),
            results: Vec::new(),
        }
    }
}

impl PrimitiveMachine for FanOutMachine {
    fn next(&mut self) -> NextStep {
        match &self.state {
            MachineState::Dispatching => {
                if let Some(role) = self.queue.pop_front() {
                    self.outstanding.push(role);
                    return NextStep::Dispatch {
                        role,
                        instructions: format!(
                            "Contribute independently to issue #{}. Return only your structured result.",
                            self.issue_number
                        ),
                    };
                }
                if let Some(role) = self.outstanding.first() {
                    return NextStep::Dispatch {
                        role: *role,
                        instructions: format!(
                            "Awaiting existing {role:?} task for issue #{}.",
                            self.issue_number
                        ),
                    };
                }
                let output = PrimitiveOutput::FanOut {
                    results: self.results.clone(),
                };
                self.state = MachineState::Done(output.clone());
                NextStep::Done { output }
            }
            MachineState::Waiting(role) => {
                fail(&format!("unexpected fan-out waiting state: {role:?}"))
            }
            MachineState::Done(output) => done(output),
            MachineState::Failed(reason) => fail(reason),
        }
    }

    fn advance(&mut self, role: TeamRole, result: Result<SubagentResult, SubagentError>) {
        if !matches!(self.state, MachineState::Dispatching) {
            self.state = MachineState::Failed("advance called after terminal state".into());
            return;
        }
        let Some(position) = self
            .outstanding
            .iter()
            .position(|outstanding| *outstanding == role)
        else {
            let expected = self.outstanding.first().copied().unwrap_or(role);
            self.state = MachineState::Failed(role_mismatch(expected, role));
            return;
        };
        let result = match result {
            Ok(result) => result,
            Err(err) => {
                self.state = MachineState::Failed(subagent_failure(role, err));
                return;
            }
        };
        let valid = matches!(
            (&role, &result),
            (TeamRole::Implementer, SubagentResult::CodeChange { .. })
                | (TeamRole::Reviewer, SubagentResult::ReviewFindings { .. })
                | (TeamRole::Reviewer, SubagentResult::Generic { .. })
                | (TeamRole::Docs, SubagentResult::DocsChange { .. })
        );
        if !valid {
            self.state = MachineState::Failed(malformed(role, "role-compatible result", &result));
            return;
        }
        self.outstanding.remove(position);
        self.results.push((role, result));
        self.state = MachineState::Dispatching;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::contracts::ReviewVerdict;

    fn result_for(role: TeamRole) -> SubagentResult {
        match role {
            TeamRole::Implementer => SubagentResult::CodeChange {
                files_touched: vec![],
                summary: "code".into(),
                commit_sha: None,
            },
            TeamRole::Reviewer => SubagentResult::ReviewFindings {
                verdict: ReviewVerdict::Approved,
                findings: vec![],
            },
            TeamRole::Docs => SubagentResult::DocsChange {
                files_touched: vec![],
                summary: "docs".into(),
            },
            _ => SubagentResult::Generic {
                json: serde_json::json!({}),
            },
        }
    }

    #[test]
    fn fan_out_success_path() {
        let mut machine = FanOutMachine::new(2);
        for role in [TeamRole::Implementer, TeamRole::Reviewer, TeamRole::Docs] {
            assert!(
                matches!(machine.next(), NextStep::Dispatch { role: actual, .. } if actual == role)
            );
            machine.advance(role, Ok(result_for(role)));
        }
        assert!(matches!(machine.next(), NextStep::Done { .. }));
    }

    #[test]
    fn fan_out_can_dispatch_all_roles_before_advancing() {
        let mut machine = FanOutMachine::new(2);
        let mut dispatched = Vec::new();
        for _ in 0..3 {
            match machine.next() {
                NextStep::Dispatch { role, .. } => dispatched.push(role),
                other => panic!("unexpected step {other:?}"),
            }
        }
        assert_eq!(
            dispatched,
            vec![TeamRole::Implementer, TeamRole::Reviewer, TeamRole::Docs]
        );
        for role in dispatched {
            machine.advance(role, Ok(result_for(role)));
        }
        assert!(matches!(machine.next(), NextStep::Done { .. }));
    }

    #[test]
    fn fan_out_failure_path() {
        let mut machine = FanOutMachine::new(2);
        let _ = machine.next();
        machine.advance(
            TeamRole::Implementer,
            Err(SubagentError::Malformed("bad json".into())),
        );
        assert!(matches!(machine.next(), NextStep::Fail { reason } if reason.contains("bad json")));
    }

    #[test]
    fn fan_out_malformed_result_path() {
        let mut machine = FanOutMachine::new(2);
        let _ = machine.next();
        machine.advance(TeamRole::Implementer, Ok(result_for(TeamRole::Reviewer)));
        assert!(
            matches!(machine.next(), NextStep::Fail { reason } if reason.contains("malformed"))
        );
    }
}
