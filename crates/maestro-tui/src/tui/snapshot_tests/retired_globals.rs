//! #1265: the Shift globals `Q` `M` `C` `P` are retired. Each action now
//! lives on its own screen or in the ⌃K palette, nothing outside Settings
//! writes the config file, and Milestones `r` reloads (run-all is `R`).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::log_drawer::{draw, log_app};
use super::make_gh_issue;
use crate::provider::github::types::{PendingPrStatus, awaiting_pending_pr};
use crate::review::types::{PrNumber, ReviewReport};
use crate::session::types::Session;
use crate::tui::app::{App, TuiCommand, TuiMode};
use crate::tui::ds::palette::{self, Run};
use crate::tui::input_handler::handle_key;
use crate::tui::navigation::registry::{PaletteCommand, Scope, registry};
use crate::tui::screens::milestone::{MilestoneEntry, MilestoneScreen};

const CONFIG_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[tui]\nmodal_on_complete = true\n",
);

/// Screens a retired key used to act on, plus Milestones.
const SCREENS: [TuiMode; 5] = [
    TuiMode::Overview,
    TuiMode::Dashboard,
    TuiMode::IssueBrowser,
    TuiMode::TokenDashboard,
    TuiMode::MilestoneView,
];

async fn press(app: &mut App, c: char) {
    handle_key(app, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)).await;
}

/// Every retired key has something to act on: a pending review, and the
/// selected session #42 with a PR waiting for a retry. Drawn at 80x24, so
/// the SizeGuard is not shown.
fn armed_app(mode: TuiMode) -> App {
    let mut app = log_app(mode);
    app.config = Some(toml::from_str(CONFIG_TOML).expect("config parses"));
    app.pending_review_report = Some(ReviewReport::new(PrNumber(7), "bot"));
    let session = Session::new(
        "task".into(),
        "claude-opus-4-5".into(),
        "orchestrator".into(),
        Some(42),
        None,
    );
    app.pool.enqueue(session);
    app.panel_view.selected = Some(0);
    app.pending_prs.push(awaiting_pending_pr(42));
    let issues = vec![make_gh_issue(10, "a"), make_gh_issue(11, "b")];
    app.screen_state.milestone_screen = Some(MilestoneScreen::new(vec![MilestoneEntry {
        number: 1,
        title: "v1".into(),
        description: String::new(),
        state: "open".into(),
        open_issues: 2,
        closed_issues: 0,
        issues,
    }]));
    let _ = draw(&mut app, 80, 24);
    app
}

fn modal_on(app: &App) -> Option<bool> {
    app.config.as_ref().map(|c| c.tui.modal_on_complete)
}

fn retry_queued(app: &App) -> bool {
    app.pending_prs[0].status == PendingPrStatus::RetryScheduled
}

#[tokio::test]
async fn shift_globals_retired() {
    for mode in SCREENS {
        for c in ['Q', 'M', 'C', 'P'] {
            let mut app = armed_app(mode);
            press(&mut app, c).await;
            let at = format!("{c} on {mode:?}");
            // The owning screens: Dashboard's own `Q` quick action
            // ("TurboQuant Savings", shadowed by the global until now) and
            // Overview's `P`, whose selection it retries.
            let q_owner = c == 'Q' && mode == TuiMode::Dashboard;
            let p_owner = c == 'P' && mode == TuiMode::Overview;
            let tq = app.tui_mode == TuiMode::TurboquantDashboard;
            assert_eq!(tq, q_owner, "{at}");
            assert_ne!(app.tui_mode, TuiMode::PrReview, "{at}");
            assert_eq!(modal_on(&app), Some(true), "{at} toggled the modal");
            assert_eq!(retry_queued(&app), p_owner, "{at}");
        }
    }
    // No Shift global is left in the Global scope.
    for c in ['Q', 'M', 'C', 'P'] {
        assert!(
            !registry()
                .iter()
                .any(|b| b.scope == Scope::Global && b.chord.code == KeyCode::Char(c)),
            "{c} is still a Global binding"
        );
    }
}

#[tokio::test]
async fn only_settings_writes_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("maestro.toml");
    std::fs::write(&path, CONFIG_TOML).expect("seed config");
    for mode in SCREENS {
        for c in '!'..='~' {
            let mut app = armed_app(mode);
            app.config_path = Some(path.clone());
            press(&mut app, c).await;
            let now = std::fs::read_to_string(&path).expect("config readable");
            assert_eq!(now, CONFIG_TOML, "{c} on {mode:?} wrote the config file");
        }
    }
}

#[tokio::test]
async fn milestones_r_reloads() {
    let mut app = armed_app(TuiMode::MilestoneView);
    app.pending_commands.clear();
    press(&mut app, 'r').await;
    assert_eq!(app.tui_mode, TuiMode::MilestoneView, "r opens nothing");
    assert!(
        app.pending_commands
            .iter()
            .any(|c| matches!(c, TuiCommand::FetchMilestones)),
        "r reloads the milestones"
    );
    assert!(
        !app.pending_commands
            .iter()
            .any(|c| matches!(c, TuiCommand::LaunchSessions(_))),
        "r never spawns"
    );
    let screen = app.screen_state.milestone_screen.as_ref().unwrap();
    assert!(screen.loading, "the reload shows as loading");
    // Run-all moved to `R` and keeps its confirm: the queue review screen.
    press(&mut app, 'R').await;
    assert_eq!(app.tui_mode, TuiMode::QueueConfirmation);
    assert!(
        !app.pending_commands
            .iter()
            .any(|c| matches!(c, TuiCommand::LaunchSessions(_))),
        "R asks before it spawns"
    );
}

/// Opens ⌃K on `app`, types `label` and presses ⏎.
async fn run_from_palette(app: &mut App, label: &str) {
    let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
    handle_key(app, ctrl_k).await;
    for c in label.chars() {
        press(app, c).await;
    }
    handle_key(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
}

#[tokio::test]
async fn retired_actions_in_palette() {
    // Each retired Shift global, its palette entry and its new key: `Q` on
    // Dashboard, `P` on Overview, "" where the palette is the way in.
    let want = [
        (PaletteCommand::TurboquantDashboard, "Q"),
        (PaletteCommand::CompleteModal, ""),
        (PaletteCommand::ReviewConcerns, ""),
        (PaletteCommand::RetryPr, "P"),
    ];
    let list = palette::entries(registry(), TuiMode::Overview, |_| None);
    let entry = |cmd| {
        list.iter()
            .find(|e| e.run == Run::Command(cmd))
            .unwrap_or_else(|| panic!("{cmd:?} has no palette entry"))
    };
    for (cmd, key) in want {
        assert_eq!(entry(cmd).key, key, "{cmd:?}");
    }

    // Each entry runs its action.
    let label = |cmd| entry(cmd).label;
    let mut app = armed_app(TuiMode::Overview);
    run_from_palette(&mut app, label(PaletteCommand::TurboquantDashboard)).await;
    assert_eq!(app.tui_mode, TuiMode::TurboquantDashboard);

    let mut app = armed_app(TuiMode::Overview);
    run_from_palette(&mut app, label(PaletteCommand::CompleteModal)).await;
    assert_eq!(
        app.tui_mode,
        TuiMode::Settings,
        "the toggle is a Settings row"
    );
    assert_eq!(modal_on(&app), Some(true), "opening Settings saves nothing");

    let mut app = armed_app(TuiMode::Overview);
    run_from_palette(&mut app, label(PaletteCommand::ReviewConcerns)).await;
    assert_eq!(app.tui_mode, TuiMode::PrReview);

    let mut app = armed_app(TuiMode::Overview);
    run_from_palette(&mut app, label(PaletteCommand::RetryPr)).await;
    assert!(retry_queued(&app), "retry PR from the palette on Overview");

    // Retry PR acts on the Overview selection, so it is off elsewhere, and
    // review concerns is off with no review pending.
    let mut app = armed_app(TuiMode::IssueBrowser);
    app.pending_review_report = None;
    let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
    handle_key(&mut app, ctrl_k).await;
    let open = app.palette.as_ref().expect("palette open");
    let reason = |cmd| {
        open.matches()
            .into_iter()
            .find(|(e, _)| e.run == Run::Command(cmd))
            .and_then(|(e, _)| e.disabled)
    };
    assert_eq!(reason(PaletteCommand::RetryPr), Some("on Overview only"));
    assert_eq!(
        reason(PaletteCommand::ReviewConcerns),
        Some("no review pending")
    );
    let mut app = armed_app(TuiMode::IssueBrowser);
    run_from_palette(&mut app, label(PaletteCommand::RetryPr)).await;
    assert!(!retry_queued(&app), "retry PR off the Overview");
}

/// #1265 with #1384: Shift+P retries the PR of the session the grid
/// highlights, after arrow keys, at 80x24 and 80x27.
#[tokio::test]
async fn shift_p_retries_the_highlighted_session() {
    for (w, h) in [(80, 24), (80, 27)] {
        let mut app = armed_app(TuiMode::Overview);
        for issue in [43, 44] {
            let s = Session::new(
                "task".into(),
                "claude-opus-4-5".into(),
                "orchestrator".into(),
                Some(issue),
                None,
            );
            app.pool.enqueue(s);
            app.pending_prs.push(awaiting_pending_pr(issue));
        }
        let _ = draw(&mut app, w, h);
        for code in [KeyCode::Down, KeyCode::Right] {
            handle_key(&mut app, KeyEvent::new(code, KeyModifiers::NONE)).await;
            let _ = draw(&mut app, w, h);
        }
        let count = app.pool.total_count();
        let layout = app.panel_view.grid_layout(count);
        let lit = app.panel_view.grid_state.selected_index(&layout);
        let want = app
            .pool
            .session_at_index(lit)
            .and_then(|s| s.issue_number)
            .expect("a highlighted session");
        press(&mut app, 'P').await;
        for pr in &app.pending_prs {
            let queued = pr.status == PendingPrStatus::RetryScheduled;
            assert_eq!(
                queued,
                pr.issue_number == want,
                "{w}x{h}: #{}",
                pr.issue_number
            );
        }
    }
}
