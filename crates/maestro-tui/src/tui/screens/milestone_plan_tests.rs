use super::*;

fn issue(number: u64, body: &str) -> Issue {
    Issue {
        number,
        title: format!("Issue {number}"),
        body: body.to_string(),
        labels: vec![],
        state: "open".to_string(),
        html_url: String::new(),
        milestone: Some(1),
        assignees: vec![],
    }
}

fn blocked_by(number: u64, by: &[u64]) -> Issue {
    let refs: Vec<String> = by.iter().map(|b| format!("- #{b}")).collect();
    issue(number, &format!("## Blocked By\n\n{}\n", refs.join("\n")))
}

/// Seven issues, one of each case.
fn fixture() -> (Vec<Issue>, Vec<Issue>) {
    let mut labelled = issue(13, "");
    labelled.labels = vec!["maestro:in-progress".into()];
    let mut closed = issue(15, "");
    closed.state = "closed".into();
    let mut other = issue(500, "");
    other.milestone = Some(2);
    let issues = vec![
        issue(10, "## Blocked By\n\n- None\n"),
        blocked_by(11, &[10]),
        // #999 is in no fetch: taken as closed, so #12 is ready.
        blocked_by(12, &[999]),
        labelled,
        blocked_by(14, &[500]),
        closed,
        issue(16, ""),
    ];
    (issues, vec![other])
}

fn plan() -> MilestonePlan {
    let (issues, known) = fixture();
    plan_milestone(&issues, &known, &HashSet::from([16]), 5.0).expect("no cycle")
}

#[test]
fn milestone_start_count() {
    // START = open, no open blocker, no claim: #10 and #12 only.
    assert_eq!(plan().start_count(), 2);
}

#[test]
fn readiness_names_each_case() {
    let p = plan();
    assert_eq!(p.readiness(10), IssueReadiness::Ready);
    assert_eq!(p.readiness(11), IssueReadiness::Waits(10));
    assert_eq!(p.readiness(12), IssueReadiness::Ready);
    assert_eq!(p.readiness(13), IssueReadiness::Claimed);
    assert_eq!(
        p.readiness(14),
        IssueReadiness::BlockedOutside {
            by: 500,
            milestone: Some(2)
        }
    );
    assert_eq!(p.readiness(15), IssueReadiness::Closed);
    assert_eq!(
        p.readiness(16),
        IssueReadiness::Claimed,
        "live session claims"
    );
}

#[test]
fn plan_waves_come_from_run_plan() {
    let p = plan();
    assert_eq!(p.plan.waves, vec![vec![10, 12], vec![11]]);
    assert_eq!(p.runs(), 3);
    assert_eq!(p.open, 6);
    assert_eq!(p.plan.worst_case_usd, 15.0);
}
