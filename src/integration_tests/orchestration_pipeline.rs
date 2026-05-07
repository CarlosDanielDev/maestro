use crate::integration_tests::orchestration_mock_task::{MockResponse, MockTaskQueue};
use crate::orchestration::{
    NextStep, Primitive, ReviewVerdict, SubagentResult, TeamRole, make_machine,
};

#[test]
fn pipeline_machine_runs_end_to_end_with_mock_task_queue() {
    let queue = MockTaskQueue::new(vec![
        MockResponse {
            role: TeamRole::Implementer,
            result: Ok(SubagentResult::CodeChange {
                files_touched: vec!["src/orchestration/primitives/pipeline.rs".into()],
                summary: "implemented primitive state machine".into(),
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
                summary: "documented orchestration behavior".into(),
            }),
        },
    ]);

    let mut machine = make_machine(Primitive::Pipeline, 662);

    loop {
        match machine.next() {
            NextStep::Dispatch { role, instructions } => {
                let result = queue.dispatch(role, &instructions);
                machine.advance(role, result);
            }
            NextStep::Done { .. } => break,
            NextStep::Fail { reason } => panic!("pipeline failed: {reason}"),
        }
    }

    assert!(queue.is_empty());
}
