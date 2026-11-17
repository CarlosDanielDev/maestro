//! Digits `1`-`7` switch tabs from a top-level screen (#1258). They stay
//! content inside a flow (CrumbBar screens) and while a text field has focus.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use super::{KeyAction, handle_key, handle_key_under_size_guard};
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::tab_bar::TABS;
use crate::tui::screens::{IssueBrowserScreen, IssueWizardScreen, LandingScreen};

/// The smallest size that is laid out: keys reach the screens.
const AREA: Rect = Rect::new(0, 0, 80, 24);

async fn press(app: &mut App, c: char) -> KeyAction {
    let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
    match handle_key_under_size_guard(app, key, AREA).await {
        Some(action) => action,
        None => handle_key(app, key).await,
    }
}

fn app_on(mode: TuiMode) -> App {
    let mut app = crate::tui::make_test_app("tab-keys");
    app.screen_state.landing_screen = Some(LandingScreen::new());
    app.tui_mode = mode;
    app
}

#[tokio::test]
async fn digit_keys_switch_tabs() {
    let mut app = app_on(TuiMode::Landing);
    for (digit, _, mode) in TABS.iter().rev() {
        press(&mut app, *digit).await;
        assert_eq!(app.tui_mode, *mode, "digit {digit}");
    }
    press(&mut app, '3').await;
    assert_eq!(app.tui_mode, TuiMode::IssueBrowser);
    assert!(
        app.screen_state.issue_browser_screen.is_some(),
        "a tab key opens the screen the same way its menu entry does"
    );
}

#[tokio::test]
async fn digits_stay_content_inside_a_flow() {
    let mut app = app_on(TuiMode::IssueWizard);
    app.screen_state.issue_wizard_screen = Some(IssueWizardScreen::new());
    press(&mut app, '2').await;
    assert_eq!(app.tui_mode, TuiMode::IssueWizard);
}

#[tokio::test]
async fn digits_stay_content_while_a_text_field_has_focus() {
    let mut app = app_on(TuiMode::IssueBrowser);
    app.screen_state.issue_browser_screen = Some(IssueBrowserScreen::new(vec![]));
    press(&mut app, 'm').await; // milestone filter: Insert mode
    press(&mut app, '2').await;
    assert_eq!(app.tui_mode, TuiMode::IssueBrowser);
}

#[tokio::test]
async fn overview_keeps_digits_for_session_detail() {
    let mut app = app_on(TuiMode::Overview);
    press(&mut app, '2').await;
    assert_eq!(app.tui_mode, TuiMode::Overview, "no session 2, no tab jump");
}

#[tokio::test]
async fn digits_type_into_a_prd_goal_or_roadmap_filter() {
    use crate::tui::screens::prd::{EditTarget, PrdScreen};
    use crate::tui::screens::roadmap::{FilterField, RoadmapScreen};

    let mut app = app_on(TuiMode::Prd);
    app.prd = Some(crate::prd::model::Prd::new());
    let mut prd = PrdScreen::new();
    prd.edit = Some(EditTarget::NewGoal {
        buffer: String::new(),
    });
    app.screen_state.prd_screen = Some(prd);
    press(&mut app, '2').await;
    assert_eq!(app.tui_mode, TuiMode::Prd);

    let mut app = app_on(TuiMode::Roadmap);
    let mut roadmap = RoadmapScreen::new();
    roadmap.editing_filter = Some(FilterField::Label);
    app.screen_state.roadmap_screen = Some(roadmap);
    press(&mut app, '2').await;
    assert_eq!(app.tui_mode, TuiMode::Roadmap);
    let label = &app
        .screen_state
        .roadmap_screen
        .as_ref()
        .unwrap()
        .filters
        .label;
    assert_eq!(label, "2", "the digit is typed into the filter");
}
