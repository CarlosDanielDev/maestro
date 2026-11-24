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

fn files(number: u64, file: &str) -> Issue {
    issue(number, &format!("## Files to Modify\n\n- `{file}`\n"))
}

/// One issue of each case. `known` holds #500 (open, milestone 2) and #600
/// (closed).
fn fixture() -> (Vec<Issue>, Vec<Issue>) {
    let mut labelled = issue(13, "");
    labelled.labels = vec!["maestro:in-progress".into()];
    let mut other = issue(500, "");
    other.milestone = Some(2);
    let mut done = issue(600, "");
    done.state = "closed".into();
    let issues = vec![
        files(10, "src/a.rs"),
        blocked_by(11, &[10]),
        // #999 is in no fetch: its state is unknown, so #12 is held.
        blocked_by(12, &[999]),
        labelled,
        blocked_by(14, &[500]),
        // A closed blocker is satisfied.
        blocked_by(15, &[600]),
        issue(16, ""),
        // Same file as #10: pushed to wave 2.
        files(17, "src/a.rs"),
    ];
    (issues, vec![other, done])
}

fn plan() -> MilestonePlan {
    let (issues, known) = fixture();
    plan_milestone(&issues, &known, &HashSet::from([16]), 5.0).expect("no cycle")
}

#[test]
fn milestone_start_count() {
    // START = wave 1: open, no open or unknown blocker, no claim, no
    // earlier issue on the same file. #10 and #15 only.
    assert_eq!(plan().plan.waves[0], vec![10, 15]);
    assert_eq!(plan().start_count(), 2);
}

/// Rewritten (#1269 judge round): the first version took a blocker that no
/// fetch returned as closed and counted #12 as ready. It must be held.
#[test]
fn unfetched_blocker_holds_the_issue_with_its_reason() {
    let p = plan();
    assert_eq!(
        p.readiness(12),
        IssueReadiness::BlockedOutside {
            by: 999,
            milestone: None
        }
    );
    assert!(
        p.plan.waves.iter().flatten().all(|n| *n != 12),
        "not launched"
    );
}

#[test]
fn readiness_names_each_case() {
    let p = plan();
    assert_eq!(p.readiness(10), IssueReadiness::Ready);
    assert_eq!(p.readiness(11), IssueReadiness::Waits(10));
    assert_eq!(p.readiness(13), IssueReadiness::Claimed);
    assert_eq!(
        p.readiness(14),
        IssueReadiness::BlockedOutside {
            by: 500,
            milestone: Some(2)
        }
    );
    assert_eq!(p.readiness(15), IssueReadiness::Ready);
    assert_eq!(
        p.readiness(16),
        IssueReadiness::Claimed,
        "live session claims"
    );
    assert_eq!(p.readiness(17), IssueReadiness::Waits(10), "same file");
}

#[test]
fn open_blocker_with_no_milestone_holds_the_issue() {
    let mut loose = issue(700, "");
    loose.milestone = None;
    let issues = vec![blocked_by(20, &[700])];
    let p = plan_milestone(&issues, &[loose], &HashSet::new(), 5.0).unwrap();
    assert_eq!(
        p.readiness(20),
        IssueReadiness::BlockedOutside {
            by: 700,
            milestone: None
        }
    );
    assert_eq!(p.start_count(), 0);
}

#[test]
fn cycle_is_an_error_naming_its_members() {
    let issues = vec![blocked_by(1, &[2]), blocked_by(2, &[1])];
    let err = plan_milestone(&issues, [], &HashSet::new(), 5.0)
        .unwrap_err()
        .to_string();
    assert!(err.contains('1') && err.contains('2'), "{err}");
}

#[test]
fn plan_waves_come_from_run_plan() {
    let p = plan();
    assert_eq!(p.plan.waves, vec![vec![10, 15], vec![11, 17]]);
    assert_eq!(p.runs(), 4);
    assert_eq!(p.open, 8);
    assert_eq!(p.plan.worst_case_usd, 20.0);
}
