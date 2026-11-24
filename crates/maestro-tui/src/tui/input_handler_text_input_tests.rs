//! #1377: one check (`is_text_input_mode`) answers "is a text field
//! focused?", and every global bare-key handler uses it: the upgrade keys,
//! the `y` restart in the run loop and the digit tab switch.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use super::{handle_key, handle_key_under_size_guard, is_restart_key, is_text_input_mode};
use crate::tui::app::{App, TuiMode};
use crate::tui::help::HelpOverlayState;
use crate::tui::screens::prd::{EditTarget, PrdScreen};
use crate::tui::screens::roadmap::{FilterField, RoadmapScreen};
use crate::tui::screens::{
    InteractionScreen, IssueBrowserScreen, IssueWizardScreen, MilestoneWizardScreen,
    PromptInputScreen,
};
use crate::updater::{ReleaseInfo, UpgradeState};

/// The smallest size that is laid out: keys reach the screens.
const AREA: Rect = Rect::new(0, 0, 80, 24);

/// One key through the run-loop order in `tui/mod.rs`: size guard, then the
/// restart key, then `handle_key`. The restart is mocked: it is not run, the
/// return value says whether the run loop would have called it.
async fn send(app: &mut App, code: KeyCode) -> bool {
    let key = KeyEvent::new(code, KeyModifiers::NONE);
    if handle_key_under_size_guard(app, key, AREA).await.is_some() {
        return false;
    }
    if is_restart_key(app, &key) {
        return true;
    }
    handle_key(app, key).await;
    false
}

/// The download double: a non-GitHub URL fails the installer's trust check
/// before any network call or file write, so a started download is harmless.
fn available() -> UpgradeState {
    UpgradeState::Available(ReleaseInfo {
        tag: "v99.0.0".into(),
        version: "99.0.0".into(),
        download_url: "http://127.0.0.1/untrusted-1377".into(),
    })
}

fn ready_to_restart() -> UpgradeState {
    UpgradeState::ReadyToRestart {
        version: "99.0.0".into(),
        backup_path: "/nonexistent-1377".into(),
    }
}

fn base_app(mode: TuiMode) -> App {
    let mut app = crate::tui::make_test_app("text-input-keys");
    app.tui_mode = mode;
    app
}

fn on_interaction() -> App {
    let mut app = base_app(TuiMode::Interaction);
    app.screen_state.interaction_screen = Some(InteractionScreen::new());
    app
}

fn on_prompt_input() -> App {
    let mut app = base_app(TuiMode::PromptInput);
    app.screen_state.prompt_input_screen = Some(PromptInputScreen::new());
    app
}

fn on_session_switcher() -> App {
    base_app(TuiMode::SessionSwitcher)
}

fn on_issue_browser_filter() -> App {
    let mut app = base_app(TuiMode::IssueBrowser);
    app.screen_state.issue_browser_screen = Some(IssueBrowserScreen::new(vec![]));
    let m = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE);
    let event = crossterm::event::Event::Key(m);
    let _ = crate::tui::screen_dispatch::dispatch_to_active_screen(&mut app, &event);
    assert!(is_text_input_mode(&app), "m opens the milestone filter");
    app
}

/// #1262: the shared launch dialog, instructions focused.
fn on_issue_launch_dialog() -> App {
    let mut app = base_app(TuiMode::IssueBrowser);
    let mut screen = IssueBrowserScreen::new(vec![]);
    screen.launch_dialog = Some(crate::tui::widgets::launch_dialog::LaunchDialog::new(
        vec![(1, "Add login flow".to_string())],
        crate::tui::widgets::launch_dialog::LaunchMode::Autonomous,
    ));
    app.screen_state.issue_browser_screen = Some(screen);
    app
}

fn on_issue_wizard_basic_info() -> App {
    use crate::tui::screens::issue_wizard::IssueWizardStep;
    let mut app = base_app(TuiMode::IssueWizard);
    let mut wizard = IssueWizardScreen::new();
    while wizard.step() != IssueWizardStep::BasicInfo {
        wizard.try_advance();
    }
    app.screen_state.issue_wizard_screen = Some(wizard);
    app
}

fn on_milestone_wizard() -> App {
    let mut app = base_app(TuiMode::MilestoneWizard);
    app.screen_state.milestone_wizard_screen = Some(MilestoneWizardScreen::new());
    app
}

fn on_prd_edit() -> App {
    let mut app = base_app(TuiMode::Prd);
    app.prd = Some(crate::prd::model::Prd::new());
    let mut prd = PrdScreen::new();
    prd.edit = Some(EditTarget::NewGoal {
        buffer: String::new(),
    });
    app.screen_state.prd_screen = Some(prd);
    app
}

fn on_roadmap_filter(field: FilterField) -> App {
    let mut app = base_app(TuiMode::Roadmap);
    let mut roadmap = RoadmapScreen::new();
    roadmap.editing_filter = Some(field);
    app.screen_state.roadmap_screen = Some(roadmap);
    app
}

fn on_help_search() -> App {
    let mut app = base_app(TuiMode::Overview);
    let mut help = HelpOverlayState::new();
    help.search_active = true;
    app.help_state = Some(help);
    app
}

/// #1261: the kill dialog's typed issue number.
fn on_kill_confirm() -> App {
    base_app(TuiMode::ConfirmKill(uuid::Uuid::nil()))
}

/// #1261: the bypass warning's typed `CONFIRM`.
fn on_bypass_warning() -> App {
    let mut app = base_app(TuiMode::BypassWarning);
    app.screen_state.bypass_warning_screen =
        Some(crate::tui::screens::bypass_warning::BypassWarningState::new());
    app
}

/// #1255: the ⌃K palette's query field, opened over Overview.
/// #1286: the Compose Review & save name (canonical `Input`, fed through
/// `widgets::input::feed`).
fn on_team_compose_name() -> App {
    use crate::tui::screens::TeamWizardScreen;
    use crate::tui::screens::team_wizard::{ComposeStep, TeamWizardMode};
    let mut app = base_app(TuiMode::TeamWizard);
    let mut screen = TeamWizardScreen::new(Default::default());
    screen.switch_mode(TeamWizardMode::Compose);
    screen.set_compose_step_for_test(ComposeStep::Save);
    app.screen_state.team_wizard_screen = Some(screen);
    app
}

fn on_palette() -> App {
    let mut app = base_app(TuiMode::Overview);
    super::open_palette(&mut app);
    app
}

#[test]
fn text_input_mode_true_for_prd_edit_field() {
    assert!(is_text_input_mode(&on_prd_edit()));
}

#[test]
fn text_input_mode_false_for_prd_without_edit() {
    let mut app = on_prd_edit();
    app.screen_state.prd_screen.as_mut().unwrap().edit = None;
    assert!(!is_text_input_mode(&app));
}

#[test]
fn text_input_mode_true_for_roadmap_label_filter() {
    assert!(is_text_input_mode(&on_roadmap_filter(FilterField::Label)));
}

#[test]
fn text_input_mode_true_for_roadmap_assignee_filter() {
    assert!(is_text_input_mode(&on_roadmap_filter(
        FilterField::Assignee
    )));
}

#[test]
fn text_input_mode_true_for_help_overlay_search() {
    assert!(is_text_input_mode(&on_help_search()));
}

#[test]
fn text_input_mode_false_for_help_overlay_without_search() {
    let mut app = on_help_search();
    app.help_state.as_mut().unwrap().search_active = false;
    assert!(!is_text_input_mode(&app));
}

#[test]
fn stale_edit_state_on_another_screen_is_not_text_input() {
    let mut app = on_prd_edit();
    let roadmap = on_roadmap_filter(FilterField::Label);
    app.screen_state.roadmap_screen = roadmap.screen_state.roadmap_screen;
    app.tui_mode = TuiMode::Overview;
    assert!(!is_text_input_mode(&app));
}

#[tokio::test]
async fn ready_to_restart_y_ignored_in_text_input() {
    let mut app = on_interaction();
    app.upgrade_state = ready_to_restart();
    for c in "yes".chars() {
        assert!(!send(&mut app, KeyCode::Char(c)).await, "no restart on {c}");
    }
    let text = app
        .screen_state
        .interaction_screen
        .as_ref()
        .unwrap()
        .editor_text();
    assert_eq!(text, "yes", "the y is typed into the composer");
    assert!(matches!(
        app.upgrade_state,
        UpgradeState::ReadyToRestart { .. }
    ));
}

#[tokio::test]
async fn ready_to_restart_bare_y_outside_text_input_restarts() {
    let mut app = base_app(TuiMode::Overview);
    app.upgrade_state = ready_to_restart();
    assert!(send(&mut app, KeyCode::Char('y')).await);
}

#[test]
fn ready_to_restart_y_chord_is_not_restart() {
    let mut app = base_app(TuiMode::Overview);
    app.upgrade_state = ready_to_restart();
    for m in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        assert!(!is_restart_key(&app, &KeyEvent::new(KeyCode::Char('y'), m)));
    }
}

#[test]
fn y_without_pending_restart_is_not_restart() {
    let app = base_app(TuiMode::Overview);
    let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(!is_restart_key(&app, &y));
}

/// What a global bare-key handler would change if it fired.
fn global_state(app: &App) -> (std::mem::Discriminant<UpgradeState>, TuiMode, bool, bool) {
    (
        std::mem::discriminant(&app.upgrade_state),
        app.tui_mode,
        app.help_state.is_some(),
        app.running,
    )
}

/// A text-input surface: its name and how to open it.
type Surface = (&'static str, fn() -> App);

/// Every text-input surface the walk covers.
fn text_surfaces() -> [Surface; 15] {
    [
        ("interaction composer", on_interaction),
        ("prompt input", on_prompt_input),
        ("session switcher", on_session_switcher),
        ("issue browser filter", on_issue_browser_filter),
        ("issue launch dialog", on_issue_launch_dialog),
        ("issue wizard basic info", on_issue_wizard_basic_info),
        ("milestone wizard", on_milestone_wizard),
        ("prd edit field", on_prd_edit),
        ("roadmap label filter", || {
            on_roadmap_filter(FilterField::Label)
        }),
        ("roadmap assignee filter", || {
            on_roadmap_filter(FilterField::Assignee)
        }),
        ("help overlay search", on_help_search),
        ("kill confirm", on_kill_confirm),
        ("bypass warning", on_bypass_warning),
        ("command palette", on_palette),
        ("team compose name", on_team_compose_name),
    ]
}

#[tokio::test]
async fn global_bare_keys_never_fire_on_any_text_input_surface() {
    let keys = ['u', 'n', 'y', '1', '2', '3', '4', '5', '6', '7'];
    for (name, make) in text_surfaces() {
        for state in [available(), ready_to_restart()] {
            for c in keys {
                let mut app = make();
                app.upgrade_state = state.clone();
                assert!(is_text_input_mode(&app), "{name} is a text field");
                let before = global_state(&app);
                let restarted = send(&mut app, KeyCode::Char(c)).await;
                assert!(!restarted, "{name}: `{c}` restarted maestro");
                assert_eq!(global_state(&app), before, "{name}: `{c}` fired a global");
            }
        }
    }
}

/// The full frame at the laid-out size: cells, styles and the cursor.
fn frame(app: &mut App) -> (ratatui::buffer::Buffer, ratatui::layout::Position) {
    let backend = ratatui::backend::TestBackend::new(AREA.width, AREA.height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    let cursor = terminal.get_cursor_position().unwrap();
    (terminal.backend().buffer().clone(), cursor)
}

/// #1413: ratatui-textarea reads crossterm's `BackTab` as Shift+Tab and
/// inserts a tab. On every text surface Shift+Tab must not type: the frame
/// stays the same, except on the two surfaces where it moves focus back.
#[tokio::test]
async fn shift_tab_never_types_on_any_text_input_surface() {
    let shift_tab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
    for (name, make) in text_surfaces() {
        let mut app = make();
        for c in "ab".chars() {
            send(&mut app, KeyCode::Char(c)).await;
        }
        let mut ring =
            (app.screen_state.prompt_input_screen.as_ref()).map(|s| s.focus_ring.clone());
        let before = frame(&mut app);
        assert!(
            handle_key_under_size_guard(&mut app, shift_tab, AREA)
                .await
                .is_none()
        );
        handle_key(&mut app, shift_tab).await;
        match name {
            "prompt input" => {
                let screen = app.screen_state.prompt_input_screen.as_ref().unwrap();
                let back = ring.as_mut().unwrap().previous();
                assert_eq!(screen.focus_ring.current(), back, "{name}: focus back");
                assert_eq!(screen.editor_text(), "ab", "{name}: no tab typed");
            }
            "issue launch dialog" => {
                use crate::tui::widgets::launch_dialog::LaunchFocus;
                let screen = app.screen_state.issue_browser_screen.as_ref().unwrap();
                let dialog = screen.launch_dialog.as_ref().unwrap();
                assert_eq!(dialog.focus, LaunchFocus::Mode, "{name}: focus back");
                assert_eq!(dialog.editor_text(), "ab", "{name}: no tab typed");
            }
            "issue wizard basic info" => {
                use crate::tui::screens::issue_wizard::FieldId;
                let wizard = app.screen_state.issue_wizard_screen.as_ref().unwrap();
                assert_eq!(wizard.focused_field(), Some(FieldId::Overview), "{name}");
            }
            _ => assert!(
                frame(&mut app) == before,
                "{name}: Shift+Tab changed the frame"
            ),
        }
        assert!(is_text_input_mode(&app), "{name}: still a text field");
    }
}

/// #1255: inside a text field ⌃K stays the field's; the palette opens only
/// outside one.
#[tokio::test]
async fn ctrl_k_never_opens_palette_on_any_text_input_surface() {
    let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
    for (name, make) in text_surfaces() {
        if name == "command palette" {
            continue;
        }
        let mut app = make();
        handle_key(&mut app, ctrl_k).await;
        assert!(app.palette.is_none(), "{name}: ⌃K opened the palette");
    }
}
