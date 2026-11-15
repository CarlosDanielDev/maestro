//! Helpers for mutating `TeamRun` state. These are small, synchronous
//! utilities; L2 orchestration wires in later chunks.

#![allow(dead_code)]

use crate::state::types::{IssueNumber, IssueRunState, TeamRun};
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub fn mark_in_flight(
    run: &mut TeamRun,
    issue: IssueNumber,
    session_id: Uuid,
    started_at: DateTime<Utc>,
) -> Result<()> {
    match run.state.get_mut(&issue) {
        Some(state @ IssueRunState::Queued) => {
            *state = IssueRunState::InFlight {
                session_id,
                started_at,
            };
            Ok(())
        }
        Some(other) => Err(anyhow!("issue {issue} not queued (found {other:?})")),
        None => Err(anyhow!("issue {issue} not in run")),
    }
}

pub fn mark_succeeded(
    run: &mut TeamRun,
    issue: IssueNumber,
    output: crate::orchestration::types::TeamOutput,
) -> Result<()> {
    match run.state.get_mut(&issue) {
        Some(state @ IssueRunState::InFlight { .. }) | Some(state @ IssueRunState::Queued) => {
            *state = IssueRunState::Succeeded { output };
            Ok(())
        }
        Some(other) => Err(anyhow!("issue {issue} cannot succeed from state {other:?}")),
        None => Err(anyhow!("issue {issue} not in run")),
    }
}

pub fn mark_failed(
    run: &mut TeamRun,
    issue: IssueNumber,
    reason: impl Into<String>,
    attempts: u8,
) -> Result<()> {
    match run.state.get_mut(&issue) {
        Some(state) => {
            *state = IssueRunState::Failed {
                reason: reason.into(),
                attempts,
            };
            Ok(())
        }
        None => Err(anyhow!("issue {issue} not in run")),
    }
}

pub fn mark_blocked(
    run: &mut TeamRun,
    issue: IssueNumber,
    blocking: Vec<IssueNumber>,
) -> Result<()> {
    match run.state.get_mut(&issue) {
        Some(state) => {
            *state = IssueRunState::Blocked { blocking };
            Ok(())
        }
        None => Err(anyhow!("issue {issue} not in run")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::types::TeamOutput;
    use std::collections::HashMap;

    fn sample_run() -> TeamRun {
        TeamRun {
            id: Uuid::new_v4(),
            team_name: "t".into(),
            started_at: Utc::now(),
            plan: vec![vec![1]],
            state: HashMap::from([(1u64, IssueRunState::Queued)]),
        }
    }

    #[test]
    fn transitions_from_queued_to_inflight_and_success() {
        let mut run = sample_run();
        mark_in_flight(&mut run, 1, Uuid::nil(), Utc::now()).unwrap();
        assert!(matches!(
            run.state.get(&1),
            Some(IssueRunState::InFlight { .. })
        ));
        mark_succeeded(
            &mut run,
            1,
            TeamOutput::Pr {
                number: 1,
                branch: "b".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            run.state.get(&1),
            Some(IssueRunState::Succeeded { .. })
        ));
    }

    #[test]
    fn mark_failed_overwrites_state() {
        let mut run = sample_run();
        mark_failed(&mut run, 1, "oops", 1).unwrap();
        match run.state.get(&1) {
            Some(IssueRunState::Failed { reason, attempts }) => {
                assert!(reason.contains("oops"));
                assert_eq!(*attempts, 1);
            }
            other => panic!("unexpected state {other:?}"),
        }
    }
}
