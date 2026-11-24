//! #1286: Compose as four steps ending in an armed write, and the Teams
//! overview selection. Key tests go through `handle_key` + `ui::draw` at
//! 80x24 (the SizeGuard swallows keys below it).

use super::compose::{next_compose_step, previous_compose_step};
use super::test_helpers::{make_health_check, make_test_team};
use super::types::{ComposeStep, TeamWizardMode};
use super::{TeamWizardScreen, roles_to_bind};
use crate::orchestration::team::SourceTier;
use crate::orchestration::types::{Primitive, TeamRole};
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::armed::ArmedState;
use crate::tui::navigation::InputMode;
use crate::tui::screens::Screen;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;
use std::time::{Duration, Instant};

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn pipeline_save_screen(user_dir: &Path, project_root: &Path) -> TeamWizardScreen {
    let mut s = TeamWizardScreen::new(Default::default());
    s.apply_health_check(vec![make_health_check("claude", true)]);
    s.set_preset_dirs_for_test(user_dir.to_path_buf(), project_root.to_path_buf());
    s.switch_mode(TeamWizardMode::Compose);
    s.set_compose_primitive_for_test(Primitive::Pipeline);
    for role in [TeamRole::Implementer, TeamRole::Reviewer, TeamRole::Docs] {
        s.compose.bindings.insert(role, "claude".into());
    }
    s.set_compose_step_for_test(ComposeStep::Save);
    s.set_compose_name_for_test("my-team");
    s
}

fn app_with(screen: TeamWizardScreen) -> App {
    let mut app = crate::tui::make_test_app("team-compose-1286");
    app.tui_mode = TuiMode::TeamWizard;
    app.screen_state.team_wizard_screen = Some(screen);
    app
}

fn wizard(app: &mut App) -> &mut TeamWizardScreen {
    app.screen_state
        .team_wizard_screen
        .as_mut()
        .expect("wizard")
}

fn screen_text(app: &mut App) -> String {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn compose_has_four_steps() {
    let labels: Vec<&str> = ComposeStep::ALL.iter().map(|s| s.label()).collect();
    assert_eq!(labels, ["Start from", "Shape", "Roles", "Review & save"]);
}

/// Done when: "Compose has four steps and save needs ⏎ twice".
#[tokio::test]
async fn team_compose_armed_save() {
    assert_eq!(ComposeStep::total(), 4);
    let user = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut app = app_with(pipeline_save_screen(user.path(), project.path()));
    let t0 = Instant::now();
    wizard(&mut app).set_clock_for_test(t0);
    let file = user.path().join("my-team.toml");
    let (shown_path, shown_toml) = wizard(&mut app).compose_preview().expect("preview");
    assert_eq!(shown_path, file);
    let screen = screen_text(&mut app);
    assert!(screen.contains("my-team.toml"), "path shown:\n{screen}");
    assert!(
        screen.contains("primitive = \"pipeline\""),
        "TOML shown:\n{screen}"
    );

    crate::tui::input_handler::handle_key(&mut app, press(KeyCode::Enter)).await;
    assert!(
        matches!(wizard(&mut app).compose_save_state(), ArmedState::Armed(_)),
        "first ⏎ arms"
    );
    assert!(!file.exists(), "one ⏎ never writes");
    assert!(screen_text(&mut app).contains("ARMED"));

    wizard(&mut app).set_clock_for_test(t0 + Duration::from_secs(1));
    crate::tui::input_handler::handle_key(&mut app, press(KeyCode::Enter)).await;
    assert_eq!(*wizard(&mut app).compose_save_state(), ArmedState::Done);
    let written = std::fs::read_to_string(&file).expect("second ⏎ writes");
    assert_eq!(written, shown_toml, "the file is exactly the TOML shown");
    assert!(wizard(&mut app).resolved_teams().contains_key("my-team"));
}

#[tokio::test]
async fn armed_save_disarms_on_esc_and_writes_nothing() {
    let user = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut app = app_with(pipeline_save_screen(user.path(), project.path()));
    wizard(&mut app).set_clock_for_test(Instant::now());
    crate::tui::input_handler::handle_key(&mut app, press(KeyCode::Enter)).await;
    crate::tui::input_handler::handle_key(&mut app, press(KeyCode::Esc)).await;
    assert_eq!(*wizard(&mut app).compose_save_state(), ArmedState::Ready);
    assert_eq!(wizard(&mut app).compose_step(), ComposeStep::Save);
    assert!(!user.path().join("my-team.toml").exists());
}

#[tokio::test]
async fn armed_save_with_invalid_name_never_arms() {
    let user = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut s = pipeline_save_screen(user.path(), project.path());
    s.set_compose_name_for_test("-bad");
    let mut app = app_with(s);
    wizard(&mut app).set_clock_for_test(Instant::now());
    for _ in 0..2 {
        crate::tui::input_handler::handle_key(&mut app, press(KeyCode::Enter)).await;
    }
    assert_eq!(*wizard(&mut app).compose_save_state(), ArmedState::Ready);
    assert!(std::fs::read_dir(user.path()).unwrap().next().is_none());
    assert!(screen_text(&mut app).contains("BLOCKED"));
}

#[tokio::test]
async fn name_field_is_a_text_input_and_takes_global_letters() {
    let user = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut s = pipeline_save_screen(user.path(), project.path());
    s.set_compose_name_for_test("");
    assert_eq!(s.desired_input_mode(), Some(InputMode::Insert));
    let mut app = app_with(s);
    for c in ['q', '1', 'u'] {
        crate::tui::input_handler::handle_key(&mut app, press(KeyCode::Char(c))).await;
    }
    assert_eq!(app.tui_mode, TuiMode::TeamWizard);
    assert_eq!(wizard(&mut app).compose_payload().name, "q1u");
}

#[test]
fn roles_step_is_skipped_only_when_there_is_nothing_to_bind() {
    use ComposeStep::*;
    assert_eq!(next_compose_step(Primitive, false), Some(Save));
    assert_eq!(previous_compose_step(Save, false), Some(Primitive));
    assert_eq!(next_compose_step(Primitive, true), Some(Roles));
    assert_eq!(previous_compose_step(Save, true), Some(Roles));
    assert_eq!(next_compose_step(Save, true), None);
}

#[test]
fn roles_to_bind_lists_dispatched_roles_then_required_ones() {
    assert_eq!(
        roles_to_bind(Primitive::VerdictOnly),
        [TeamRole::Triager, TeamRole::Reviewer]
    );
    assert_eq!(roles_to_bind(Primitive::SinglePass), [TeamRole::Reviewer]);
    assert_eq!(
        roles_to_bind(Primitive::Pipeline),
        [TeamRole::Implementer, TeamRole::Reviewer, TeamRole::Docs]
    );
}

#[tokio::test]
async fn overview_j_and_k_move_the_selected_preset() {
    let mut s = TeamWizardScreen::new(Default::default());
    s.apply_resolved_teams(vec![
        make_test_team("b-user", Primitive::SinglePass, &[], SourceTier::User),
        make_test_team("a-built", Primitive::Pipeline, &[], SourceTier::BuiltIn),
    ]);
    let mut app = app_with(s);
    assert_eq!(wizard(&mut app).selected_preset(), Some("a-built"));
    crate::tui::input_handler::handle_key(&mut app, press(KeyCode::Char('j'))).await;
    assert_eq!(wizard(&mut app).selected_preset(), Some("b-user"));
    crate::tui::input_handler::handle_key(&mut app, press(KeyCode::Char('j'))).await;
    assert_eq!(wizard(&mut app).selected_preset(), Some("b-user"));
    crate::tui::input_handler::handle_key(&mut app, press(KeyCode::Char('k'))).await;
    assert_eq!(wizard(&mut app).selected_preset(), Some("a-built"));
    assert!(screen_text(&mut app).contains("BUILT-IN"));
}
