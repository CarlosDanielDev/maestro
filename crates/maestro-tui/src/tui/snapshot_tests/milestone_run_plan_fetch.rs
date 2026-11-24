//! #1269 judge rounds: a failed fetch, a cycle, live claims, and fresh
//! data winning over stale copies, through the real merge path.

use super::*;

/// A milestone whose issue fetch failed shows the error, not an empty
/// milestone, and its START is unknown. Its issues are unknown blockers for
/// the others, so nothing that waits on them is counted.
#[tokio::test]
async fn failed_milestone_fetch_shows_as_failed() {
    let mut app = milestone_app();
    let fetched = milestones()
        .into_iter()
        .map(|m| {
            let ms = crate::provider::types::Milestone {
                number: m.number,
                title: m.title,
                description: m.description,
                state: m.state,
                open_issues: m.open_issues,
                closed_issues: m.closed_issues,
            };
            let issues = if m.number == 61 {
                Err("gh: HTTP 502".to_string())
            } else {
                Ok(m.issues)
            };
            (ms, issues)
        })
        .collect();
    app.handle_data_event(crate::tui::app::TuiDataEvent::Milestones(Ok(
        crate::tui::app::MilestoneFetch {
            milestones: fetched,
            blockers: Vec::new(),
            skipped: Vec::new(),
        },
    )));
    let screen = app.screen_state.milestone_screen.as_mut().unwrap();
    assert_eq!(screen.start_count(0), None, "unknown, not 0");
    screen.selected = 0;
    let t = draw(&mut app, 100, 24);
    let rows: Vec<String> = {
        let buf = t.backend().buffer();
        (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect())
            .collect()
    };
    assert!(
        rows.iter()
            .any(|r| r.contains("issues not loaded (gh: HTTP 502)")),
        "{}",
        rows.join("\n")
    );
    // #968 waits on #831 of the failed milestone: held, not ready.
    let screen = app.screen_state.milestone_screen.as_ref().unwrap();
    let plan = screen.plan_for(62).unwrap().as_ref().unwrap();
    assert!(plan.plan.waves.iter().flatten().all(|n| *n != 968));
    // R on the failed milestone toasts and opens nothing.
    key(&mut app, KeyCode::Char('R')).await;
    assert_eq!(app.tui_mode, TuiMode::MilestoneView);
    assert!(!spawned(&app));
}

/// A Blocked By cycle: the tab names its members, and `R` toasts and opens
/// nothing.
#[tokio::test]
async fn cycle_draws_its_members_and_r_opens_nothing() {
    let mut app = milestone_app();
    let cyc = vec![
        issue(41, "First", 70, &body(&[42], &[])),
        issue(42, "Second", 70, &body(&[41], &[])),
    ];
    let entry = MilestoneEntry {
        number: 70,
        title: "Loop".into(),
        description: String::new(),
        state: "open".into(),
        open_issues: 2,
        closed_issues: 0,
        issues: cyc,
    };
    let mut s = MilestoneScreen::new(vec![entry]);
    s.selected = 0;
    app.screen_state.milestone_screen = Some(s);
    let t = draw(&mut app, 100, 24);
    let buf = t.backend().buffer();
    let text: String = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .map(|(x, y)| buf[(x, y)].symbol().to_string())
        .collect();
    assert!(text.contains("not planned"), "{text}");
    assert!(text.contains("41") && text.contains("42"), "members named");
    key(&mut app, KeyCode::Char('R')).await;
    assert_eq!(app.tui_mode, TuiMode::MilestoneView, "nothing opens");
    assert!(app.screen_state.queue_confirmation_screen.is_none());
    assert!(!spawned(&app));
}

/// START and the Run queue use the same claims: a live session on #964
/// takes it out of both.
#[tokio::test]
async fn live_session_claims_reach_start_and_run_queue() {
    let mut app = milestone_app();
    app.pool.enqueue(crate::session::types::Session::new(
        "task".into(),
        "model".into(),
        "orchestrator".into(),
        Some(964),
        None,
    ));
    let _ = draw(&mut app, 80, 24);
    let screen = app.screen_state.milestone_screen.as_ref().unwrap();
    // #964 is claimed; #969 no longer shares wave 1 with it.
    let start = &screen.plan_for(62).unwrap().as_ref().unwrap().plan.waves[0];
    assert_eq!(start, &vec![965, 969], "START sees the live claim");
    handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE),
    )
    .await;
    let run = app.screen_state.queue_confirmation_screen.as_ref().unwrap();
    let wave1: Vec<u64> = run
        .run_queue()
        .unwrap()
        .sessions()
        .iter()
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(wave1, vec![965, 969]);
}

// ---- #1269 judge round 3: fresh data wins through the real merge path ----

fn fetch_of(
    milestones: Vec<MilestoneEntry>,
    blockers: Vec<Issue>,
) -> crate::tui::app::MilestoneFetch {
    let milestones = milestones
        .into_iter()
        .map(|m| {
            let ms = crate::provider::types::Milestone {
                number: m.number,
                title: m.title,
                description: m.description,
                state: m.state,
                open_issues: m.open_issues,
                closed_issues: m.closed_issues,
            };
            (ms, Ok(m.issues))
        })
        .collect();
    crate::tui::app::MilestoneFetch {
        milestones,
        blockers,
        skipped: Vec::new(),
    }
}

/// A stale "closed" copy of #831 (cache, or a looked-up blocker) never
/// beats the fresh list that has it open: #968 stays held.
#[test]
fn stale_closed_blocker_loses_to_fresh_open_list() {
    let mut app = milestone_app();
    let mut stale = issue(831, "Fix the toast", 61, "");
    stale.state = "closed".into();
    app.state.issue_cache.insert(831, stale.clone());
    let event = crate::tui::app::TuiDataEvent::Milestones(Ok(fetch_of(milestones(), vec![stale])));
    app.handle_data_event(event);
    let screen = app.screen_state.milestone_screen.as_ref().unwrap();
    let plan = screen.plan_for(62).unwrap().as_ref().unwrap();
    assert_eq!(
        plan.readiness(968),
        crate::tui::screens::milestone_plan::IssueReadiness::BlockedOutside {
            by: 831,
            milestone: Some(61)
        }
    );
    assert_eq!(
        app.state.issue_cache[&831].state, "open",
        "cache takes the fresh copy"
    );
}

/// The fetch reads no cache: a blocker the cache has as closed but GitHub
/// now has open (reopened) holds its issue.
#[tokio::test]
async fn reopened_blocker_is_read_fresh_and_holds_its_issue() {
    use crate::provider::github::client::mock::MockGitHubClient;
    let mut app = milestone_app();
    let mut stale = issue(700, "Old blocker", 0, "");
    stale.milestone = None;
    stale.state = "closed".into();
    app.state.issue_cache.insert(700, stale);
    let client = MockGitHubClient::new();
    client.set_milestones(vec![crate::provider::types::Milestone {
        number: 62,
        title: "v0.33.0".into(),
        description: String::new(),
        state: "open".into(),
        open_issues: 1,
        closed_issues: 0,
    }]);
    let mut reopened = issue(700, "Old blocker", 0, "");
    reopened.milestone = None;
    // The mock's list returns every issue it holds; GitHub would list
    // only #20 and the lookup would return #700 open.
    client.set_issues(vec![
        issue(20, "Needs 700", 62, &body(&[700], &[])),
        reopened,
    ]);
    let fetch = crate::tui::screens::milestone_plan::fetch_milestones(&client)
        .await
        .unwrap();
    app.handle_data_event(crate::tui::app::TuiDataEvent::Milestones(Ok(fetch)));
    let screen = app.screen_state.milestone_screen.as_ref().unwrap();
    let plan = screen.plan_for(62).unwrap().as_ref().unwrap();
    assert_ne!(
        plan.readiness(20),
        crate::tui::screens::milestone_plan::IssueReadiness::Ready
    );
    let wave1 = plan.plan.waves.first().cloned().unwrap_or_default();
    assert!(!wave1.contains(&20), "#20 does not launch: {wave1:?}");
}

/// `other/repo#5` holds its issue even when this repo's #5 is closed.
#[test]
fn cross_repo_blocker_holds_its_issue_through_the_fetch() {
    let mut app = milestone_app();
    let mut entries = milestones();
    entries[1].issues.push(issue(
        980,
        "Needs the SDK",
        62,
        "## Blocked By\n\n- acme/sdk#5\n",
    ));
    let mut local_five = issue(5, "Old", 0, "");
    local_five.state = "closed".into();
    app.handle_data_event(crate::tui::app::TuiDataEvent::Milestones(Ok(fetch_of(
        entries,
        vec![local_five],
    ))));
    let screen = app.screen_state.milestone_screen.as_ref().unwrap();
    let plan = screen.plan_for(62).unwrap().as_ref().unwrap();
    assert_eq!(
        plan.readiness(980),
        crate::tui::screens::milestone_plan::IssueReadiness::OtherRepo
    );
}

/// The preview's facts line names another repository's blocker as written.
#[test]
fn preview_names_the_other_repo_blocker() {
    use crate::tui::screens::milestone::MilestoneTab;
    let entry = MilestoneEntry {
        number: 62,
        title: "v0.33.0".into(),
        description: String::new(),
        state: "open".into(),
        open_issues: 1,
        closed_issues: 0,
        issues: vec![issue(
            980,
            "Needs the SDK",
            62,
            "## Blocked By\n\n- #964\n- acme/sdk#5\n",
        )],
    };
    let mut s = MilestoneScreen::new(vec![entry]);
    s.set_tab(MilestoneTab::IssuePreview);
    let t = draw_screen(&mut s, 200, 24);
    let buf = t.backend().buffer();
    let text: String = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .map(|(x, y)| buf[(x, y)].symbol().to_string())
        .collect();
    assert!(
        text.contains("blocked by #964, acme/sdk#5 (other repo)"),
        "{text}"
    );
    assert!(!text.contains("blocked by #964, #5"));
}
