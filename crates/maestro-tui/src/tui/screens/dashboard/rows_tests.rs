use super::*;
use crate::provider::github::types::awaiting_pending_pr;

fn session(status: SessionStatus, issue: u64) -> Session {
    let mut s = Session::new(
        "task".into(),
        "claude-opus-4-5".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    );
    s.status = status;
    s
}

fn group_of(list: &WorkList, key: WorkKey) -> Option<Group> {
    [Group::Sessions, Group::PrPipeline, Group::Finished]
        .into_iter()
        .find(|g| list.group_rows(*g).contains(&key))
}

/// #1280: one table, every status, each in the group the design names.
#[test]
fn work_list_groups() {
    use SessionStatus::*;
    let cases = [
        (Queued, Group::Sessions),
        (Spawning, Group::Sessions),
        (Running, Group::Sessions),
        (GatesRunning, Group::Sessions),
        (NeedsReview, Group::Sessions),
        // Terminal but actionable (re-run gates): stays in Sessions.
        (FailedGates, Group::Sessions),
        (Errored, Group::Sessions),
        (Paused, Group::Sessions),
        (Stalled, Group::Sessions),
        (Retrying, Group::Sessions),
        (CiFix, Group::Sessions),
        (NeedsPr, Group::Sessions),
        (ConflictFix, Group::Sessions),
        (Interactive, Group::Sessions),
        (Completed, Group::Finished),
        (Killed, Group::Finished),
    ];
    let sessions: Vec<Session> = cases
        .iter()
        .enumerate()
        .map(|(i, (st, _))| session(*st, 100 + i as u64))
        .collect();
    let refs: Vec<&Session> = sessions.iter().collect();
    let pending = [awaiting_pending_pr(545), awaiting_pending_pr(42)];
    let mut list = WorkList::default();
    list.rebuild(&refs, &[540, 538], &pending);

    for (s, (status, want)) in sessions.iter().zip(cases) {
        assert_eq!(
            group_of(&list, WorkKey::Session(s.id)),
            Some(want),
            "{status:?}"
        );
    }
    assert_eq!(
        list.group_rows(Group::PrPipeline),
        &[
            WorkKey::OpenPr(538),
            WorkKey::OpenPr(540),
            WorkKey::PendingPr(545),
            WorkKey::PendingPr(42),
        ],
        "open PRs by number, then the restored pending PRs in queue order"
    );
    assert_eq!(list.rows[0], WorkKey::Header(Group::Sessions));
    assert_eq!(
        list.selected,
        Some(WorkKey::Session(sessions[0].id)),
        "the first row that is not a header"
    );
}

#[test]
fn work_list_empty_keeps_three_headers_and_no_selection() {
    let mut list = WorkList::default();
    list.rebuild(&[], &[], &[]);
    assert_eq!(
        list.rows,
        vec![
            WorkKey::Header(Group::Sessions),
            WorkKey::Header(Group::PrPipeline),
            WorkKey::Header(Group::Finished),
        ]
    );
    assert_eq!(list.selected, None);
}

#[test]
fn selection_is_a_key_and_survives_reordering() {
    let a = session(SessionStatus::Running, 1);
    let b = session(SessionStatus::Running, 2);
    let mut list = WorkList::default();
    list.rebuild(&[&a, &b], &[], &[]);
    list.move_down();
    assert_eq!(list.selected_session(), Some(b.id));
    // `a` finishes: it moves to Finished, `b` stays selected.
    let mut a_done = a.clone();
    a_done.status = SessionStatus::Completed;
    list.rebuild(&[&a_done, &b], &[], &[]);
    assert_eq!(list.selected_session(), Some(b.id));
    // `b` is gone: the selection falls back to the first row.
    list.rebuild(&[&a_done], &[], &[]);
    assert_eq!(list.selected_session(), Some(a.id));
}

#[test]
fn moves_skip_headers_and_stop_at_the_ends() {
    let a = session(SessionStatus::Running, 1);
    let done = session(SessionStatus::Completed, 2);
    let pending = [awaiting_pending_pr(545)];
    let mut list = WorkList::default();
    list.rebuild(&[&a, &done], &[], &pending);
    list.move_up();
    assert_eq!(list.selected, Some(WorkKey::Session(a.id)));
    list.move_down();
    assert_eq!(list.selected, Some(WorkKey::PendingPr(545)));
    list.move_down();
    assert_eq!(list.selected, Some(WorkKey::Session(done.id)));
    list.move_down();
    assert_eq!(list.selected, Some(WorkKey::Session(done.id)));
}

#[test]
fn pipeline_stage_marks_the_failed_step() {
    assert_eq!(pipeline_stage(SessionStatus::Running), (0, None));
    assert_eq!(pipeline_stage(SessionStatus::FailedGates), (1, Some(1)));
    assert_eq!(pipeline_stage(SessionStatus::CiFix), (4, Some(4)));
    assert_eq!(pipeline_stage(SessionStatus::Completed), (5, None));
}
