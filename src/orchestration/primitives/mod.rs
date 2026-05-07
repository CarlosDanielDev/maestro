//! Per-issue primitive state machines for L2 orchestration.

mod fan_out;
mod pipeline;
mod single_pass;
mod verdict_only;

use crate::orchestration::contracts::{NewIssueDraft, SubagentError, SubagentResult};
use crate::orchestration::types::{Primitive, TeamRole};

pub use fan_out::FanOutMachine;
pub use pipeline::PipelineMachine;
pub use single_pass::SinglePassMachine;
pub use verdict_only::VerdictOnlyMachine;

#[derive(Debug, Clone, PartialEq)]
pub enum NextStep {
    Dispatch {
        role: TeamRole,
        instructions: String,
    },
    Done {
        output: PrimitiveOutput,
    },
    Fail {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrimitiveOutput {
    Pipeline {
        code_summary: String,
        review_summary: String,
        docs_summary: String,
    },
    FanOut {
        results: Vec<(TeamRole, SubagentResult)>,
    },
    SinglePass {
        role: TeamRole,
        result: SubagentResult,
    },
    Verdict {
        decision: String,
        rationale: String,
        new_issues: Vec<NewIssueDraft>,
    },
}

pub trait PrimitiveMachine {
    fn next(&mut self) -> NextStep;
    fn advance(&mut self, role: TeamRole, result: Result<SubagentResult, SubagentError>);
}

pub fn make_machine(primitive: Primitive, issue_number: u64) -> Box<dyn PrimitiveMachine> {
    match primitive {
        Primitive::Pipeline => Box::new(PipelineMachine::new(issue_number)),
        Primitive::FanOut => Box::new(FanOutMachine::new(issue_number)),
        Primitive::SinglePass => Box::new(SinglePassMachine::new(issue_number)),
        Primitive::VerdictOnly => Box::new(VerdictOnlyMachine::new(issue_number)),
    }
}

#[derive(Debug, Clone)]
enum MachineState {
    Dispatching,
    Waiting(TeamRole),
    Done(PrimitiveOutput),
    Failed(String),
}

impl MachineState {
    fn next_terminal(
        &self,
        waiting_instructions: impl FnOnce(TeamRole) -> String,
    ) -> Option<NextStep> {
        match self {
            Self::Waiting(role) => Some(dispatch(*role, waiting_instructions(*role))),
            Self::Done(output) => Some(done(output)),
            Self::Failed(reason) => Some(fail(reason)),
            Self::Dispatching => None,
        }
    }

    fn take_waiting(&mut self, role: TeamRole) -> bool {
        match self {
            Self::Waiting(expected) if *expected == role => true,
            Self::Waiting(expected) => {
                *self = Self::Failed(role_mismatch(*expected, role));
                false
            }
            Self::Done(_) | Self::Failed(_) | Self::Dispatching => {
                *self = Self::Failed("advance called while not waiting".into());
                false
            }
        }
    }
}

fn dispatch(role: TeamRole, instructions: impl Into<String>) -> NextStep {
    NextStep::Dispatch {
        role,
        instructions: instructions.into(),
    }
}

fn done(output: &PrimitiveOutput) -> NextStep {
    NextStep::Done {
        output: output.clone(),
    }
}

fn fail(reason: &str) -> NextStep {
    NextStep::Fail {
        reason: reason.to_string(),
    }
}

fn role_mismatch(expected: TeamRole, got: TeamRole) -> String {
    format!("expected result for {expected:?}, got {got:?}")
}

fn subagent_failure(role: TeamRole, err: SubagentError) -> String {
    format!("{role:?} failed: {err}")
}

fn malformed(role: TeamRole, expected: &str, got: &SubagentResult) -> String {
    format!("{role:?} returned malformed result: expected {expected}, got {got:?}")
}

fn review_summary(result: &SubagentResult) -> Option<String> {
    match result {
        SubagentResult::ReviewFindings { verdict, findings } => {
            let notes = findings
                .iter()
                .map(|finding| finding.note.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            Some(format!("{verdict:?}: {notes}"))
        }
        _ => None,
    }
}
