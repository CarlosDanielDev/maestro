//! End-to-end orchestration smoke test (#665 Task 6.4).
//!
//! Drives a 2-issue pipeline plan through the L3 scheduler using the
//! existing `MockTaskQueue` from `orchestration_mock_task` to substitute
//! L1 dispatch. Validates that:
//! - `Scheduler::from_input` builds the expected 2-level plan
//! - the ready set respects dependency order
//! - both issues reach `Succeeded` after the canned responses replay
//!
//! Companion to `orchestration_pipeline.rs`, which exercises a single
//! issue. This test catches regressions where the cross-issue ready-set
//! logic and the per-issue primitive machine drift apart.

use crate::integration_tests::orchestration_mock_task::{MockResponse, MockTaskQueue};
use crate::orchestration::dag::{IssueMeta, IssueState};
use crate::orchestration::scheduler::Scheduler;
use crate::orchestration::team::{ResolvedTeam, RoleBinding, SourceTier};
use crate::orchestration::types::{Primitive, TeamInput, TeamOutput, TeamRole};
use crate::orchestration::{NextStep, ReviewVerdict, SubagentResult, make_machine};
use crate::state::types::{IssueNumber, IssueRunState};
use std::collections::HashMap;

fn pipeline_team() -> ResolvedTeam {
    let mut bindings = HashMap::new();
    for role in [TeamRole::Implementer, TeamRole::Reviewer, TeamRole::Docs] {
        bindings.insert(
            role,
            RoleBinding {
                agent: "claude".into(),
                mode: None,
                model_override: None,
                prompt_addendum: None,
                fallback_agent: None,
            },
        );
    }
    ResolvedTeam {
        name: "default-coder".into(),
        primitive: Primitive::Pipeline,
        min_agents: vec!["claude".into()],
        bindings,
        source_tier: SourceTier::BuiltIn,
    }
}

fn pipeline_responses(branch: &str) -> Vec<MockResponse> {
    vec![
        MockResponse {
            role: TeamRole::Implementer,
            result: Ok(SubagentResult::CodeChange {
                files_touched: vec!["src/lib.rs".into()],
                summary: format!("implement {branch}"),
                commit_sha: None,
            }),
        },
        MockResponse {
            role: TeamRole::Reviewer,
            result: Ok(SubagentResult::ReviewFindings {
                verdict: ReviewVerdict::Approved,
                findings: vec![],
            }),
        },
        MockResponse {
            role: TeamRole::Docs,
            result: Ok(SubagentResult::DocsChange {
                files_touched: vec!["README.md".into()],
                summary: format!("doc {branch}"),
            }),
        },
    ]
}

fn drive_machine_to_terminal(
    primitive: Primitive,
    issue: IssueNumber,
    queue: &MockTaskQueue,
) -> TeamOutput {
    let mut machine = make_machine(primitive, issue);
    loop {
        match machine.next() {
            NextStep::Dispatch { role, instructions } => {
                let result = queue.dispatch(role, &instructions);
                machine.advance(role, result);
            }
            NextStep::Done { .. } => {
                return TeamOutput::Pr {
                    number: issue,
                    branch: format!("feat/issue-{issue}"),
                };
            }
            NextStep::Fail { reason } => panic!("issue #{issue} failed: {reason}"),
        }
    }
}

fn meta(n: IssueNumber, blocked_by: Vec<IssueNumber>) -> IssueMeta {
    IssueMeta {
        number: n,
        state: IssueState::Open,
        milestone: Some(99),
        blocked_by,
    }
}

#[test]
fn two_issue_pipeline_plan_has_two_levels_and_both_reach_succeeded() {
    let mut metas = HashMap::new();
    metas.insert(1, meta(1, vec![]));
    metas.insert(2, meta(2, vec![1]));

    let mut sched = Scheduler::from_input(
        pipeline_team(),
        TeamInput::IssueSet {
            primary_milestone: Some(99),
            issues: vec![1, 2],
        },
        metas,
        2,
    )
    .expect("scheduler must build the 2-issue plan");

    assert_eq!(
        sched.run.plan.len(),
        2,
        "expected two scheduler levels for #1 → #2 chain"
    );

    let level1_ready = sched.next_ready();
    assert_eq!(level1_ready, vec![1], "level 1 must offer only #1");

    // Drive #1 through the pipeline machine via a mock task queue.
    let queue1 = MockTaskQueue::new(pipeline_responses("issue-1"));
    let output1 = drive_machine_to_terminal(Primitive::Pipeline, 1, &queue1);
    assert!(queue1.is_empty(), "queue1 must be drained");
    sched
        .run
        .state
        .insert(1, IssueRunState::Succeeded { output: output1 });

    // After #1 succeeds, level 2 (#2) must become ready.
    let level2_ready = sched.next_ready();
    assert_eq!(
        level2_ready,
        vec![2],
        "after #1 Succeeded, #2 must be the next ready issue"
    );

    let queue2 = MockTaskQueue::new(pipeline_responses("issue-2"));
    let output2 = drive_machine_to_terminal(Primitive::Pipeline, 2, &queue2);
    assert!(queue2.is_empty(), "queue2 must be drained");
    sched
        .run
        .state
        .insert(2, IssueRunState::Succeeded { output: output2 });

    assert!(
        sched.next_ready().is_empty(),
        "no more ready work after both succeed"
    );
    for n in [1u64, 2u64] {
        assert!(
            matches!(
                sched.run.state.get(&n),
                Some(IssueRunState::Succeeded { .. })
            ),
            "issue #{n} must reach Succeeded; got {:?}",
            sched.run.state.get(&n)
        );
    }
}

#[test]
fn first_issue_failure_blocks_dependent_issue() {
    let mut metas = HashMap::new();
    metas.insert(10, meta(10, vec![]));
    metas.insert(11, meta(11, vec![10]));

    let mut sched = Scheduler::from_input(
        pipeline_team(),
        TeamInput::IssueSet {
            primary_milestone: Some(99),
            issues: vec![10, 11],
        },
        metas,
        2,
    )
    .unwrap();

    sched.run.state.insert(
        10,
        IssueRunState::Failed {
            reason: "implementer crashed".into(),
            attempts: 1,
        },
    );

    let ready = sched.next_ready();
    assert!(
        ready.is_empty(),
        "with #10 Failed, #11 must NOT become ready (got {ready:?})"
    );
}
