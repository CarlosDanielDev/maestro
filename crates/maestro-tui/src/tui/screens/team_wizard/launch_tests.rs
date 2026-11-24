//! #1290: the Plan's waves come from `work::run_plan`, only those waves are
//! dispatched, and the cost line states the exact worst case against the
//! budget left (the launch dialog's `LaunchBudget`).

use super::test_helpers::{make_issue_meta, make_test_team};
use super::types::{LaunchInputKind, LaunchPhase, LaunchStep, TeamWizardMode};
use super::{Screen, ScreenAction, TeamWizardScreen};
use crate::orchestration::dag::{IssueMeta, IssueState};
use crate::orchestration::team::SourceTier;
use crate::orchestration::types::{Primitive, TeamInput, TeamRole};
use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::key_event;
use crate::tui::theme::Theme;
use crate::tui::widgets::launch_dialog::LaunchBudget;
use crate::work::run_plan::{Hold, run_plan};
use crossterm::event::KeyCode;
use ratatui::{Terminal, backend::TestBackend};
use std::collections::{HashMap, HashSet};

fn budget(per_session_usd: f64, total_usd: f64, spent: f64) -> LaunchBudget {
    LaunchBudget {
        per_session_usd,
        total_usd,
        spent,
        ..Default::default()
    }
}

/// Launch on Plan & launch for the issue set `picked`.
fn planned(picked: &[u64], metas: &HashMap<u64, IssueMeta>, b: LaunchBudget) -> TeamWizardScreen {
    let mut s = TeamWizardScreen::new(Default::default());
    s.apply_resolved_teams(vec![make_test_team(
        "default-coder",
        Primitive::Pipeline,
        &[
            (TeamRole::Implementer, "claude"),
            (TeamRole::Reviewer, "claude"),
            (TeamRole::Docs, "claude"),
        ],
        SourceTier::BuiltIn,
    )]);
    s.set_known_agents(vec!["claude".into()]);
    s.set_launch_budget(b);
    s.switch_mode(TeamWizardMode::Launch);
    s.set_launch_team_for_test("default-coder");
    s.set_launch_input_for_test(LaunchInputKind::IssueSet, None);
    s.set_launch_manual_issues_for_test(picked.to_vec());
    s.launch.primary_milestone = Some(1);
    s.apply_issue_metas(metas.clone());
    s.set_launch_step_for_test(LaunchStep::Plan);
    s.build_plan_preview_for_test();
    s
}

fn drawn(s: &mut TeamWizardScreen) -> String {
    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    t.draw(|f| s.draw(f, f.area(), &Theme::dark())).unwrap();
    let buf = t.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 10 ← 11, 12; 11 ← 13; 14 is blocked by #99 of another milestone.
fn dag() -> HashMap<u64, IssueMeta> {
    let rows: [(u64, Option<u64>, &[u64]); 6] = [
        (10, Some(1), &[]),
        (11, Some(1), &[10]),
        (12, Some(1), &[10]),
        (13, Some(1), &[11]),
        (14, Some(1), &[99]),
        (99, Some(2), &[]),
    ];
    rows.iter()
        .map(|(n, m, b)| (*n, make_issue_meta(*n, IssueState::Open, *m, b)))
        .collect()
}

#[test]
fn team_launch_uses_run_plan() {
    let metas = dag();
    let mut s = planned(&[11, 12, 13, 14], &metas, budget(5.0, 0.0, 0.0));
    let plan = s.launch_payload().plan.clone().expect("plan built");

    // The Scheduler adds the open same-milestone blocker #10; run_plan
    // plans the expanded set with the same inputs, and nothing else does.
    assert_eq!(plan.auto_added, vec![10]);
    let open: HashSet<u64> = [10, 11, 12, 13, 14].into();
    let want = run_plan(&open, &metas, &HashSet::new(), &HashMap::new(), 5.0).unwrap();
    assert_eq!(plan.levels, want.waves);
    assert_eq!(plan.deferred, want.deferred);
    assert_eq!(plan.worst_case_usd, want.worst_case_usd);
    assert_eq!(plan.levels, vec![vec![10], vec![11, 12], vec![13]]);
    assert_eq!(
        plan.deferred,
        vec![(
            14,
            vec![Hold::BlockedOutside {
                by: 99,
                milestone: Some(2)
            }]
        )]
    );
    assert_eq!(plan.final_count, 4);

    // Drawn as waves of lanes, the held issue under NOT INCLUDED.
    let screen = drawn(&mut s);
    for want in ["WAVE 1", "WAVE 2", "after wave 1 finishes", "WAVE 3"] {
        assert!(screen.contains(want), "{want} missing:\n{screen}");
    }
    assert!(screen.contains("NOT INCLUDED · 1"), "{screen}");
    assert!(screen.contains("#14"), "{screen}");
    assert!(screen.contains("blocked by open #99"), "{screen}");

    // Plan ⏎ opens the dialog; dialog ⏎ dispatches exactly the waves.
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_phase(), LaunchPhase::Dialog);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(
        action,
        ScreenAction::LaunchTeam {
            team_name: "default-coder".into(),
            input: TeamInput::IssueSet {
                primary_milestone: Some(1),
                issues: vec![10, 11, 12, 13],
            },
            max_parallel: 3,
        }
    );
}

/// Every picked issue held back: nothing to launch, and Plan says why.
#[test]
fn team_launch_with_nothing_to_run_never_opens_the_dialog() {
    let metas = dag();
    let mut s = planned(&[14], &metas, budget(5.0, 0.0, 0.0));
    assert_eq!(s.launch_payload().plan.as_ref().unwrap().final_count, 0);
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(s.launch_phase(), LaunchPhase::Editing);
    assert_eq!(
        s.launch.error,
        Some("Nothing will run · every issue is held back")
    );
}

#[test]
fn team_launch_worst_case() {
    let metas: HashMap<u64, IssueMeta> = [10, 11, 12]
        .iter()
        .map(|n| (*n, make_issue_meta(*n, IssueState::Open, Some(1), &[])))
        .collect();

    // 3 sessions × $5.00 cap = $15.00 exactly; $50.00 − $12.00 = $38.00 left.
    let mut s = planned(&[10, 11, 12], &metas, budget(5.0, 50.0, 12.0));
    assert_eq!(
        s.launch_payload().plan.as_ref().unwrap().worst_case_usd,
        15.0
    );
    let screen = drawn(&mut s);
    assert!(screen.contains("$15.00 (3 × $5.00 cap)"), "{screen}");
    assert!(screen.contains("$38.00 left now"), "{screen}");
    assert!(screen.contains("estimate not available"), "{screen}");
    assert!(!screen.contains("rough estimate"), "{screen}");
    assert!(!screen.contains("over the budget left"), "{screen}");

    // The dialog states the same line.
    s.set_launch_phase_for_test(LaunchPhase::Dialog);
    let dialog = drawn(&mut s);
    assert!(dialog.contains("$15.00 (3 × $5.00 cap)"), "{dialog}");
    assert!(dialog.contains("$38.00 left now"), "{dialog}");

    // Over the budget left ($8.00 < $15.00): the plan says the cap can stop it.
    let mut s = planned(&[10, 11, 12], &metas, budget(5.0, 20.0, 12.0));
    let screen = drawn(&mut s);
    assert!(screen.contains("$8.00 left now"), "{screen}");
    assert!(screen.contains("over the budget left"), "{screen}");

    // No total cap: the bound, and no "left".
    let mut s = planned(&[10, 11, 12], &metas, budget(5.0, 0.0, 12.0));
    let screen = drawn(&mut s);
    assert!(
        screen.contains("$15.00 (3 × $5.00 cap)  ·  no total cap"),
        "{screen}"
    );

    // No per-session cap: no bound exists, so none is invented.
    let mut s = planned(&[10, 11, 12], &metas, budget(0.0, 50.0, 12.0));
    let screen = drawn(&mut s);
    assert!(screen.contains("worst case unknown"), "{screen}");
    assert!(!screen.contains("$0.00 ("), "{screen}");
}

/// Through `ui::draw` and `handle_key` at 120x30: the Plan's cost line reads
/// the app's `[budget]` and spend, the launch dialog's source
/// (`budget_prespawn::launch_budget`), fed before every draw.
#[tokio::test]
async fn team_launch_cost_reads_the_launch_dialog_budget() {
    let toml = concat!(
        "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
        "[sessions]\n",
        "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
        "[github]\n",
        "[notifications]\nslack_webhook_url = \"\"\n",
    );
    let mut app = crate::tui::make_test_app("team-launch-budget");
    app.config = Some(toml::from_str(toml).expect("config parses"));
    app.total_cost = 12.0;
    app.tui_mode = crate::tui::app::TuiMode::TeamWizard;
    let metas: HashMap<u64, IssueMeta> = [(42, make_issue_meta(42, IssueState::Open, None, &[]))]
        .into_iter()
        .collect();
    let mut s = planned(&[42], &metas, LaunchBudget::default());
    s.set_launch_step_for_test(LaunchStep::Input);
    app.screen_state.team_wizard_screen = Some(s);

    let frame = |app: &mut crate::tui::app::App| {
        let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
        t.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    frame(&mut app);
    // Input › Issue › find: type 42, Enter picks it and builds the plan.
    let key = |c| crossterm::event::KeyEvent::new(c, crossterm::event::KeyModifiers::NONE);
    crate::tui::input_handler::handle_key(&mut app, key(KeyCode::Enter)).await;
    for c in ['4', '2'] {
        crate::tui::input_handler::handle_key(&mut app, key(KeyCode::Char(c))).await;
    }
    crate::tui::input_handler::handle_key(&mut app, key(KeyCode::Enter)).await;
    let screen = frame(&mut app);
    assert!(screen.contains("$5.00 (1 × $5.00 cap)"), "{screen}");
    assert!(screen.contains("$38.00 left now"), "{screen}");
}
