//! #1246: Esc on the Executing step cancels the team run, and the failure
//! banner names the wave that stopped the run. Both tests draw the wizard.

use super::test_helpers::{make_health_check, make_issue_meta, make_test_team};
use super::types::{LaunchInputKind, LaunchPhase, TeamWizardMode};
use super::{Screen, TeamWizardScreen};
use crate::orchestration::dag::IssueState;
use crate::orchestration::scheduler::Scheduler;
use crate::orchestration::team::SourceTier;
use crate::orchestration::types::{Primitive, TeamInput, TeamRole};
use crate::provider::types::ProviderKind;
use crate::session::team_runner::{
    CancellationToken, IssueFailure, TeamLauncher, TeamOutcome, run_team_until,
};
use crate::state::types::IssueNumber;
use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::key_event;
use crate::tui::theme::Theme;
use async_trait::async_trait;
use crossterm::event::KeyCode;
use ratatui::{Terminal, backend::TestBackend};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

/// Records spawns; its sessions never finish (a Paused or stuck NeedsPr
/// session), so only a cancel can end the run.
#[derive(Default)]
struct StuckLauncher {
    spawned: Mutex<Vec<IssueNumber>>,
}

#[async_trait]
impl TeamLauncher for StuckLauncher {
    async fn spawn_for_issue(&self, issue: IssueNumber, _agent: String) -> Result<Uuid, String> {
        self.spawned.lock().unwrap().push(issue);
        Ok(Uuid::nil())
    }

    async fn wait_until_terminal(&self, _issue: IssueNumber) -> Result<(), String> {
        std::future::pending().await
    }
}

fn team() -> crate::orchestration::team::ResolvedTeam {
    make_test_team(
        "default-coder",
        Primitive::Pipeline,
        &[(TeamRole::Implementer, "claude")],
        SourceTier::BuiltIn,
    )
}

fn metas() -> HashMap<IssueNumber, crate::orchestration::dag::IssueMeta> {
    let mut metas = HashMap::new();
    metas.insert(10, make_issue_meta(10, IssueState::Open, Some(1), &[]));
    metas.insert(11, make_issue_meta(11, IssueState::Open, Some(1), &[10]));
    metas.insert(12, make_issue_meta(12, IssueState::Open, Some(1), &[10]));
    metas
}

fn executing_screen() -> TeamWizardScreen {
    let mut s = TeamWizardScreen::new(ProviderKind::default());
    s.apply_resolved_teams(vec![team()]);
    s.apply_health_check(vec![make_health_check("claude", true)]);
    s.set_known_agents(vec!["claude".into()]);
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_team_for_test("default-coder");
    s.set_launch_input_for_test(LaunchInputKind::IssueSet, None);
    s.set_launch_manual_issues_for_test(vec![10, 11, 12]);
    s.apply_issue_metas(metas());
    s.build_plan_preview_for_test();
    s.set_launch_phase_for_test(LaunchPhase::Executing);
    s
}

fn drawn(s: &mut TeamWizardScreen) -> String {
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();
    t.draw(|f| s.draw(f, f.area(), &theme)).unwrap();
    t.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

async fn settle() {
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn esc_on_executing_cancels_the_run_and_starts_no_later_wave() {
    let mut s = executing_screen();
    let cancel = CancellationToken::new();
    s.set_run_cancel(cancel.clone());
    let launcher = Arc::new(StuckLauncher::default());
    let scheduler = Scheduler::from_input(
        team(),
        TeamInput::IssueSet {
            primary_milestone: Some(1),
            issues: vec![10, 11, 12],
        },
        metas(),
        3,
    )
    .expect("scheduler");
    let run = tokio::spawn(run_team_until(
        Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
        scheduler,
        "claude".into(),
        cancel,
    ));
    settle().await;
    assert_eq!(*launcher.spawned.lock().unwrap(), vec![10]);
    // #1290: the shell footer draws the screen's hints.
    assert!(
        s.footer_hints().contains(&("Esc", "cancel run")),
        "footer must offer the way out"
    );

    s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    let outcome = tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("Esc must end the run")
        .expect("run task");
    // What `TuiDataEvent::TeamLaunchResult` does in data_handler.
    s.apply_launch_result(outcome.into_apply_result());

    assert_eq!(s.launch_phase(), LaunchPhase::Failed);
    let screen = drawn(&mut s);
    assert!(
        screen.contains("cancelled by user while waiting on wave 1"),
        "banner must show the cancel reason:\n{screen}"
    );
    assert_eq!(
        *launcher.spawned.lock().unwrap(),
        vec![10],
        "wave 2 spawned after cancel"
    );
}

#[test]
fn failed_wave_banner_shows_skip_reason() {
    let mut s = executing_screen();
    let mut outcome = TeamOutcome::new("default-coder".into());
    outcome.failed.push(IssueFailure {
        issue: 10,
        reason: "session was killed".into(),
    });
    outcome.skipped_due_to_upstream = vec![11, 12];
    outcome.skip_reason = Some("wave 1 failed: #10 session was killed".into());
    s.apply_launch_result(outcome.into_apply_result());

    let screen = drawn(&mut s);
    assert!(
        screen.contains("wave 1 failed: #10 session was killed"),
        "banner must name the failed wave:\n{screen}"
    );
    assert!(screen.contains("2 downstream issue(s) skipped"));
}
