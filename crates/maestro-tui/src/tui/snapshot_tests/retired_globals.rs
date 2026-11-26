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
use crate::tui::screens::dashboard::usage::View;
use crate::tui::screens::milestone::{MilestoneEntry, MilestoneScreen};

const CONFIG_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[tui]\nmodal_on_complete = true\n",
);

/// Screens a retired key used to act on, plus Milestones. The view only
/// matters on the Dashboard.
const SCREENS: [(TuiMode, View); 4] = [
    (TuiMode::Dashboard, View::Ops),
    (TuiMode::IssueBrowser, View::Ops),
    (TuiMode::Dashboard, View::Tokens),
    (TuiMode::MilestoneView, View::Ops),
];

async fn press(app: &mut App, c: char) {
    handle_key(app, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)).await;
}

/// Every retired key has something to act on: a pending review, and the
/// selected session #42 with a PR waiting for a retry. Drawn at 80x24, so
/// the SizeGuard is not shown.
fn armed_app(mode: TuiMode) -> App {
    armed_app_on(mode, View::Ops)
}

/// [`armed_app`] with the Dashboard on `view`.
fn armed_app_on(mode: TuiMode, view: View) -> App {
    let mut app = log_app(mode);
    app.screen_state.dashboard.view = view;
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
    for (mode, on) in SCREENS {
        for c in ['Q', 'M', 'C', 'P'] {
            let mut app = armed_app_on(mode, on);
            let view = app.screen_state.dashboard.view;
            press(&mut app, c).await;
            let at = format!("{c} on {mode:?} {on:?}");
            // #1280: no screen takes `P`; ⏎ ⏎ on the PR row retries (below).
            // #1289: TurboQuant is a panel of Dashboard › Tokens (`t`), so
            // the Dashboard's own `Q` launcher is gone too.
            let p_owner = false;
            assert_eq!(app.screen_state.dashboard.view, view, "{at} switched view");
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
    for (mode, on) in SCREENS {
        for c in '!'..='~' {
            let mut app = armed_app_on(mode, on);
            app.config_path = Some(path.clone());
            press(&mut app, c).await;
            let now = std::fs::read_to_string(&path).expect("config readable");
            assert_eq!(
                now, CONFIG_TOML,
                "{c} on {mode:?} {on:?} wrote the config file"
            );
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
    // Each retired Shift global, its palette entry and its new key, shown
    // only on the screen where that key works: `t` (Tokens) on Dashboard,
    // "" where the palette is the way in. #1280 retired `P` with the
    // Overview grid.
    use PaletteCommand::{CompleteModal, PrPipeline, ReviewConcerns, Turboquant};
    let want = [
        (TuiMode::IssueBrowser, Turboquant, ""),
        (TuiMode::Dashboard, Turboquant, "t"),
        (TuiMode::IssueBrowser, CompleteModal, ""),
        (TuiMode::IssueBrowser, ReviewConcerns, ""),
        (TuiMode::IssueBrowser, PrPipeline, ""),
        (TuiMode::Dashboard, PrPipeline, ""),
    ];
    for (mode, cmd, key) in want {
        let list = palette::entries(registry(), mode, |_| None);
        let e = list.iter().find(|e| e.run == Run::Command(cmd));
        let e = e.unwrap_or_else(|| panic!("{cmd:?} has no palette entry"));
        assert_eq!(e.key, key, "{cmd:?} on {mode:?}");
    }
    let list = palette::entries(registry(), TuiMode::Dashboard, |_| None);
    let entry = |cmd| {
        list.iter()
            .find(|e| e.run == Run::Command(cmd))
            .unwrap_or_else(|| panic!("{cmd:?} has no palette entry"))
    };

    // Each entry runs its action.
    let label = |cmd| entry(cmd).label;
    let mut app = armed_app(TuiMode::Dashboard);
    run_from_palette(&mut app, label(PaletteCommand::Turboquant)).await;
    assert_eq!(app.tui_mode, TuiMode::Dashboard);
    assert_eq!(app.screen_state.dashboard.view, View::Tokens);

    let mut app = armed_app(TuiMode::Dashboard);
    run_from_palette(&mut app, label(PaletteCommand::CompleteModal)).await;
    assert_eq!(
        app.tui_mode,
        TuiMode::Settings,
        "the toggle is a Settings row"
    );
    assert_eq!(modal_on(&app), Some(true), "opening Settings saves nothing");

    let mut app = armed_app(TuiMode::Dashboard);
    run_from_palette(&mut app, label(PaletteCommand::ReviewConcerns)).await;
    assert_eq!(app.tui_mode, TuiMode::PrReview);

    // #1280: `retry PR` opens the Dashboard on the pending PR row and writes
    // nothing; the row's armed ⏎ ⏎ is the retry. It works from any screen.
    for mode in [TuiMode::Dashboard, TuiMode::IssueBrowser] {
        let mut app = armed_app(mode);
        run_from_palette(&mut app, label(PaletteCommand::PrPipeline)).await;
        assert_eq!(app.tui_mode, TuiMode::Dashboard, "from {mode:?}");
        assert_eq!(
            app.screen_state.dashboard.list.selected,
            Some(crate::tui::screens::dashboard::rows::WorkKey::PendingPr(42)),
            "from {mode:?}"
        );
        assert!(!retry_queued(&app), "the palette writes nothing");
    }

    // Review concerns is off with no review pending.
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
    assert_eq!(reason(PaletteCommand::PrPipeline), None);
    assert_eq!(
        reason(PaletteCommand::ReviewConcerns),
        Some("no review pending")
    );
}

/// #1280: Shift+P is inert on the Dashboard; the
/// PR row's armed ⏎ retries exactly the PR it shows, at 80x24 and 80x27.
#[tokio::test]
async fn pr_row_enter_retries_that_pr_only() {
    for (w, h) in [(80, 24), (80, 27)] {
        let mut app = armed_app(TuiMode::Dashboard);
        app.pending_prs.clear();
        for issue in [43, 44] {
            app.pending_prs.push(awaiting_pending_pr(issue));
        }
        let _ = draw(&mut app, w, h);
        press(&mut app, 'P').await;
        assert!(
            app.pending_prs
                .iter()
                .all(|p| p.status != PendingPrStatus::RetryScheduled),
            "{w}x{h}: P is retired"
        );
        // Rows: session #42, then the PR pipeline #43, #44.
        for _ in 0..2 {
            handle_key(&mut app, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)).await;
        }
        for _ in 0..2 {
            handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
        }
        for pr in &app.pending_prs {
            let queued = pr.status == PendingPrStatus::RetryScheduled;
            assert_eq!(
                queued,
                pr.issue_number == 44,
                "{w}x{h}: #{}",
                pr.issue_number
            );
        }
    }
}

/// #1280: every recovery hint names the Dashboard PR row, not the
/// retired Shift+P.
#[test]
fn recovery_hints_name_the_dashboard_row() {
    let theme = crate::tui::theme::Theme::dark();
    let warning: String = crate::tui::gh_auth_warning::line(&theme)
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    for hint in [crate::tui::app::AUTH_RECOVERY_HINT, warning.as_str()] {
        assert!(
            hint.contains("Dashboard") && !hint.contains("Shift+P"),
            "{hint}"
        );
    }
}
