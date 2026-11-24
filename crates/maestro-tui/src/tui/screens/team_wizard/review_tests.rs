//! #1286 judge round: seeded health is never shown as real, the overwrite
//! warning checks the file the write targets, the footer keys are declared
//! in the registry, and the preset dirs are never real by default.

use super::TeamWizardScreen;
use super::test_helpers::make_test_team;
use super::types::{ComposeStep, ComposeTier, SEEDED_HEALTH_MESSAGE, TeamWizardMode};
use crate::agent_provider::types::{AgentHealthCheck, AgentProviderId};
use crate::orchestration::team::SourceTier;
use crate::orchestration::types::{Primitive, TeamRole};
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::checklist::CheckState;
use crate::tui::navigation::registry::{Scope, registry};
use crate::tui::screens::Screen;
use crate::tui::widgets::preset_card::readiness;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;
use std::time::Instant;

/// The cache exactly as `screen_dispatch::populate_team_wizard_data`
/// seeds it today: every agent `available`, no real check.
fn seeded(agents: &[&str]) -> Vec<AgentHealthCheck> {
    agents
        .iter()
        .map(|id| AgentHealthCheck {
            provider_id: AgentProviderId::new(*id),
            available: true,
            version: None,
            message: SEEDED_HEALTH_MESSAGE.into(),
        })
        .collect()
}

fn text(app: &mut App, w: u16, h: u16) -> String {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
    t.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    let buf = t.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn app_with(screen: TeamWizardScreen) -> App {
    let mut app = crate::tui::make_test_app("team-review-1286");
    app.tui_mode = TuiMode::TeamWizard;
    app.screen_state.team_wizard_screen = Some(screen);
    app
}

fn bound_pipeline(name: &str, tier: SourceTier) -> crate::orchestration::team::ResolvedTeam {
    make_test_team(
        name,
        Primitive::Pipeline,
        &[
            (TeamRole::Implementer, "claude"),
            (TeamRole::Reviewer, "claude"),
            (TeamRole::Docs, "claude"),
        ],
        tier,
    )
}

#[test]
fn the_seeded_message_is_the_one_screen_dispatch_writes() {
    let src = include_str!("../../screen_dispatch.rs");
    assert!(
        src.contains(&format!("{SEEDED_HEALTH_MESSAGE:?}")),
        "screen_dispatch seeds health with another message; update SEEDED_HEALTH_MESSAGE"
    );
}

/// B1: seeded health is not a check, so nothing reads `● ok` / `✓ ready`.
#[test]
fn seeded_health_never_shows_ok_or_ready() {
    let mut s = TeamWizardScreen::new(Default::default());
    s.apply_resolved_teams(vec![bound_pipeline("default-coder", SourceTier::BuiltIn)]);
    s.apply_health_check(seeded(&["claude"]));
    let team = &s.resolved_teams()["default-coder"];
    assert_eq!(
        readiness(team, s.preflight_cache()),
        (CheckState::Pending, "health not checked".into())
    );
    let screen = text(&mut app_with(s), 168, 42);
    assert!(!screen.contains("ready"), "{screen}");
    assert!(!screen.contains("● ok"), "{screen}");
    assert!(screen.contains("? not checked"), "{screen}");
}

fn save_screen(user: &Path, project: &Path, name: &str) -> TeamWizardScreen {
    let mut s = TeamWizardScreen::new(Default::default());
    s.set_preset_dirs_for_test(user.to_path_buf(), project.to_path_buf());
    s.switch_mode(TeamWizardMode::Compose);
    s.set_compose_primitive_for_test(Primitive::SinglePass);
    s.set_compose_binding_for_test(TeamRole::Reviewer, "claude");
    s.set_compose_step_for_test(ComposeStep::Save);
    s.set_compose_name_for_test(name);
    s
}

async fn enter(app: &mut App) {
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    crate::tui::input_handler::handle_key(app, key).await;
}

/// B2 (a): preset loading failed (empty map) but the file is on disk.
#[tokio::test]
async fn existing_target_file_is_named_and_needs_the_armed_confirm() {
    let user = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let file = user.path().join("x.toml");
    std::fs::write(&file, "old").unwrap();
    let mut app = app_with(save_screen(user.path(), project.path(), "x"));
    let t0 = Instant::now();
    let w = app.screen_state.team_wizard_screen.as_mut().unwrap();
    w.set_clock_for_test(t0);
    assert!(w.compose_target_exists());
    let screen = text(&mut app, 120, 34);
    assert!(screen.contains("! replaces"), "{screen}");
    assert!(screen.contains("x.toml"), "{screen}");
    enter(&mut app).await;
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "old",
        "one ⏎ keeps it"
    );
    assert!(text(&mut app, 120, 34).contains("replaces x.toml"));
    enter(&mut app).await;
    assert_ne!(std::fs::read_to_string(&file).unwrap(), "old");
}

/// B2 (b): user and project both hold `x`; saving to user names the user
/// file, not the project preset that wins the merged list.
#[test]
fn clash_warning_names_the_file_that_is_written() {
    let user = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::write(user.path().join("x.toml"), "old").unwrap();
    let mut s = save_screen(user.path(), project.path(), "x");
    s.apply_resolved_teams(vec![bound_pipeline("x", SourceTier::Project)]);
    assert_eq!(s.compose_payload().tier, ComposeTier::User);
    let screen = text(&mut app_with(s), 200, 34);
    assert!(!screen.contains("PROJECT preset"), "{screen}");
    let user_path = user.path().join("x.toml").to_string_lossy().into_owned();
    assert!(
        screen.contains(&format!("! replaces {user_path}")),
        "{screen}"
    );
}

#[test]
fn a_new_name_has_no_replace_warning() {
    let user = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let s = save_screen(user.path(), project.path(), "fresh");
    assert!(!s.compose_target_exists());
    assert!(!text(&mut app_with(s), 120, 34).contains("replaces"));
}

#[test]
fn preset_dirs_are_unset_by_default_in_tests() {
    let mut s = TeamWizardScreen::new(Default::default());
    s.switch_mode(TeamWizardMode::Compose);
    s.set_compose_primitive_for_test(Primitive::SinglePass);
    s.set_compose_name_for_test("x");
    assert!(s.compose_preview().is_err(), "no real config dir in tests");
}

/// Footer keys are declared once: in the TeamWizard registry block or as
/// Global keys. `l` has one label covering both its uses.
#[test]
fn footer_keys_are_declared_in_the_registry() {
    let declared: Vec<_> = registry()
        .iter()
        .filter(|b| {
            b.scope == Scope::Global || Scope::Screen(TuiMode::TeamWizard).encloses(b.scope)
        })
        .map(|b| b.chord.label())
        .collect();
    let mut s = TeamWizardScreen::new(Default::default());
    let mut keys = s.footer_hints();
    s.switch_mode(TeamWizardMode::Compose);
    for step in ComposeStep::ALL {
        s.set_compose_step_for_test(*step);
        keys.extend(s.footer_hints());
    }
    for (key, _) in keys {
        for token in key.split([' ', '/']).filter(|t| !t.is_empty()) {
            let token = match token {
                "↑↓" => "↑",
                "←→" => "←",
                t => t,
            };
            assert!(
                declared.contains(&token),
                "footer key {token:?} is not in the registry"
            );
        }
    }
    let l: Vec<_> = registry()
        .iter()
        .filter(|b| Scope::Screen(TuiMode::TeamWizard).encloses(b.scope))
        .filter(|b| b.chord.label() == "l")
        .collect();
    assert_eq!(l.len(), 1);
    assert!(l[0].label.contains("Roles"), "{}", l[0].label);
}

/// Nit: the estimate prices the roles the shape runs, and `+ $?` only for
/// a priced role whose agent has no rate.
#[test]
fn estimate_prices_dispatched_roles_and_flags_only_their_unpriced_agents() {
    use crate::tui::widgets::preset_card::prompt_estimate;
    let l2_only = make_test_team("e", Primitive::SinglePass, &[], SourceTier::User);
    let reviewer = make_test_team(
        "r",
        Primitive::SinglePass,
        &[(TeamRole::Reviewer, "claude")],
        SourceTier::User,
    );
    assert!(prompt_estimate(&reviewer).0 > prompt_estimate(&l2_only).0);
    let unused = make_test_team(
        "u",
        Primitive::Pipeline,
        &[
            (TeamRole::Implementer, "claude"),
            (TeamRole::Reviewer, "claude"),
            (TeamRole::Docs, "claude"),
            (TeamRole::Triager, "mystery-llm"),
        ],
        SourceTier::User,
    );
    assert!(!prompt_estimate(&unused).1, "unused triager is not priced");
    let unpriced = make_test_team(
        "p",
        Primitive::SinglePass,
        &[(TeamRole::Reviewer, "mystery-llm")],
        SourceTier::User,
    );
    assert!(prompt_estimate(&unpriced).1);
}
