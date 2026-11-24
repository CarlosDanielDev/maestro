//! #1286 judge round 2: the write refuses when the target file changed
//! between arming and the second ⏎. Launch's plan states the worst case,
//! never the card's prompt estimate (#1290).

use super::TeamWizardScreen;
use super::test_helpers::{make_issue_meta, make_test_team};
use super::types::{ComposeStep, LaunchInputKind, TeamWizardMode};
use crate::orchestration::dag::IssueState;
use crate::orchestration::team::SourceTier;
use crate::orchestration::types::{Primitive, TeamRole};
use crate::tui::ds::armed::ArmedState;
use crate::tui::navigation::InputMode;
use crate::tui::screens::Screen;
use crate::tui::screens::test_helpers::key_event;
use crate::tui::widgets::preset_card::prompt_estimate;
use crossterm::event::KeyCode;
use std::path::Path;
use std::time::Instant;

fn save_screen(user: &Path, project: &Path) -> TeamWizardScreen {
    let mut s = TeamWizardScreen::new(Default::default());
    s.set_preset_dirs_for_test(user.to_path_buf(), project.to_path_buf());
    s.set_clock_for_test(Instant::now());
    s.switch_mode(TeamWizardMode::Compose);
    s.set_compose_primitive_for_test(Primitive::SinglePass);
    s.set_compose_binding_for_test(TeamRole::Reviewer, "claude");
    s.set_compose_step_for_test(ComposeStep::Save);
    s.set_compose_name_for_test("x");
    s
}

fn enter(s: &mut TeamWizardScreen) {
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
}

fn assert_refused(s: &TeamWizardScreen, word: &str) {
    match s.compose_save_state() {
        ArmedState::Failed(why) => assert!(why.contains(word), "{why}"),
        other => panic!("expected a refused write, got {other:?}"),
    }
}

#[test]
fn armed_as_a_new_file_refuses_when_the_file_appears() {
    let (user, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let file = user.path().join("x.toml");
    let mut s = save_screen(user.path(), project.path());
    enter(&mut s);
    std::fs::write(&file, "theirs").unwrap();
    enter(&mut s);
    assert_refused(&s, "appeared");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "theirs");
}

#[test]
fn armed_as_a_replace_refuses_when_the_file_changes() {
    let (user, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let file = user.path().join("x.toml");
    std::fs::write(&file, "old").unwrap();
    let mut s = save_screen(user.path(), project.path());
    enter(&mut s);
    std::fs::write(&file, "changed by someone else").unwrap();
    enter(&mut s);
    assert_refused(&s, "changed");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "changed by someone else"
    );
}

#[test]
fn armed_as_a_replace_refuses_when_the_file_is_removed() {
    let (user, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let file = user.path().join("x.toml");
    std::fs::write(&file, "old").unwrap();
    let mut s = save_screen(user.path(), project.path());
    enter(&mut s);
    std::fs::remove_file(&file).unwrap();
    enter(&mut s);
    assert_refused(&s, "removed");
    assert!(!file.exists());
}

#[test]
fn a_refused_write_arms_again_on_the_next_enter_with_the_new_state() {
    let (user, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let file = user.path().join("x.toml");
    let mut s = save_screen(user.path(), project.path());
    enter(&mut s);
    std::fs::write(&file, "theirs").unwrap();
    enter(&mut s);
    enter(&mut s);
    assert!(matches!(s.compose_save_state(), ArmedState::Armed(_)));
    enter(&mut s);
    assert_eq!(*s.compose_save_state(), ArmedState::Done);
    assert_ne!(std::fs::read_to_string(&file).unwrap(), "theirs");
}

/// #1290 replaces the shared prompt-token estimate: Launch states the
/// exact worst case from the launch dialog's budget, and the token figure
/// is never drawn on the plan.
#[test]
fn launch_plan_states_the_worst_case_not_the_prompt_estimate() {
    let team = make_test_team(
        "solo",
        Primitive::SinglePass,
        &[(TeamRole::Reviewer, "codex")],
        SourceTier::User,
    );
    let mut s = TeamWizardScreen::new(Default::default());
    s.apply_resolved_teams(vec![team.clone()]);
    s.set_known_agents(vec!["claude".into(), "codex".into()]);
    s.set_launch_budget(crate::tui::widgets::launch_dialog::LaunchBudget {
        per_session_usd: 5.0,
        ..Default::default()
    });
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_team_for_test("solo");
    s.set_launch_input_for_test(LaunchInputKind::Issue, Some(1));
    let mut metas = std::collections::HashMap::new();
    metas.insert(1, make_issue_meta(1, IssueState::Open, None, &[]));
    s.apply_issue_metas(metas);
    s.set_launch_step_for_test(super::types::LaunchStep::Plan);
    s.build_plan_preview_for_test();
    let plan = s.launch_payload().plan.as_ref().expect("plan built");
    assert_eq!(plan.worst_case_usd, 5.0);
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 30)).unwrap();
    t.draw(|f| s.draw(f, f.area(), &crate::tui::theme::Theme::dark()))
        .unwrap();
    let drawn: String = (t.backend().buffer().content().iter())
        .map(|c| c.symbol())
        .collect();
    let token = crate::tui::ds::money::money(Some(prompt_estimate(&team).0));
    assert!(drawn.contains("$5.00"), "worst case drawn:\n{drawn}");
    assert!(!drawn.contains(&token), "prompt estimate {token} drawn");
}
