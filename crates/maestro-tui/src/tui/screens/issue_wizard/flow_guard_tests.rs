//! #1282 judge-round tests (#1450): the gate validates, Esc asks before
//! dropping a draft, the AI diff scrolls, one placeholder count, the Blocked
//! By round trip, and `r` retrying a failed review with one AI call.

use super::flow_tests::*;
use super::*;
use crate::tui::app::{App, TuiCommand, TuiMode};
use crate::tui::ds::armed::ArmedState;
use crate::tui::screens::test_helpers::key_event;
use ratatui::{Terminal, backend::TestBackend};
use std::time::Duration;

#[tokio::test]
async fn issue_create_blocked_by_an_empty_required_field() {
    // An accepted AI section emptied Overview after Details had passed.
    let mut payload = ready_payload();
    payload.overview = String::new();
    let mut app = on_preview(payload);
    assert_eq!(
        *wizard(&app).create_gate().state(),
        ArmedState::Blocked("Overview required · ⏎ goes to it".into())
    );
    let out = screen_text(&mut app);
    assert!(
        out.contains("Overview required · ⏎ goes to it"),
        "gate names it:\n{out}"
    );
    for _ in 0..2 {
        if wizard(&app).step() == IssueWizardStep::Preview {
            press(&mut app, KeyCode::Enter).await;
        }
    }
    assert!(
        !create_queued(&app),
        "an invalid draft never reaches GitHub"
    );
    assert_eq!(wizard(&app).step(), IssueWizardStep::BasicInfo);
    assert_eq!(wizard(&app).focused_field(), Some(FieldId::Overview));
    assert!(wizard(&app).field_error(FieldId::Overview).is_some());
}

#[tokio::test]
async fn esc_on_basics_with_a_draft_asks_before_closing() {
    let mut app = app_with(IssueWizardScreen::new());
    for c in "Add".chars() {
        press(&mut app, KeyCode::Char(c)).await;
    }
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(
        app.tui_mode,
        TuiMode::IssueWizard,
        "one Esc keeps the draft"
    );
    assert!(wizard(&app).leave_armed());
    let out = screen_text(&mut app);
    assert!(
        out.contains("Esc again closes and drops this draft"),
        "{out}"
    );

    // Any other key keeps the draft and disarms.
    press(&mut app, KeyCode::Char('!')).await;
    assert!(!wizard(&app).leave_armed());
    assert_eq!(wizard(&app).field_text(FieldId::Title), "Add!");

    // Screen level: the second Esc must come inside the window.
    let t0 = Instant::now();
    let mut s = IssueWizardScreen::new();
    s.paste_text_into_focused("draft");
    s.set_clock_for_test(t0);
    let esc = || key_event(KeyCode::Esc);
    assert_eq!(
        s.handle_input(&esc(), InputMode::Insert),
        ScreenAction::None
    );
    s.set_clock_for_test(t0 + LEAVE_WINDOW + Duration::from_millis(1));
    assert_eq!(
        s.handle_input(&esc(), InputMode::Insert),
        ScreenAction::None,
        "a late second Esc asks again"
    );
    assert_eq!(s.handle_input(&esc(), InputMode::Insert), ScreenAction::Pop);

    // An empty draft closes at once.
    let mut empty = IssueWizardScreen::new();
    assert_eq!(
        empty.handle_input(&esc(), InputMode::Insert),
        ScreenAction::Pop
    );
}

#[test]
fn ai_diff_scrolls_inside_the_focused_section() {
    let mut w = IssueWizardScreen::new();
    *w.payload_mut() = ready_payload();
    w.set_step_for_tests(IssueWizardStep::AiReview);
    w.begin_ai_review();
    w.apply_ai_review(Ok("critique".into()));
    w.begin_improve();
    let mut candidate = ready_payload();
    candidate.acceptance_criteria = (1..=40).map(|i| format!("- line {i}\n")).collect();
    w.apply_improve_result(Ok(candidate));
    assert_eq!(w.section_focus(), 5);
    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    let theme = crate::tui::theme::Theme::dark();
    t.draw(|f| w.draw(f, f.area(), &theme)).unwrap();
    let out = format!("{}", t.backend());
    assert!(
        out.contains("more rows · ↑↓ scroll"),
        "a cut says so:\n{out}"
    );
    assert!(!out.contains("+ - line 40"), "{out}");
    for _ in 0..100 {
        w.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    }
    let (_, height) = w.diff_view.get();
    let end = w.section_diff_rows() - usize::from(height);
    assert_eq!(
        usize::from(w.diff_scroll()),
        end,
        "stops with the end on screen"
    );
    t.draw(|f| w.draw(f, f.area(), &theme)).unwrap();
    let out = format!("{}", t.backend());
    assert!(out.contains("+ - line 40"), "last line reachable:\n{out}");
    assert!(out.contains("↑ scroll up"), "{out}");
    w.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(usize::from(w.diff_scroll()), end - 1);
    // j/k move between sections (here only one changed) and reset the scroll.
    w.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
    assert_eq!(w.diff_scroll(), 0);
}

#[test]
fn blocked_by_round_trips_through_the_body() {
    let mut w = IssueWizardScreen::new();
    *w.payload_mut() = ready_payload();
    w.set_step_for_tests(IssueWizardStep::Dependencies);
    w.begin_dependency_fetch();
    w.apply_dep_issues(vec![
        issue(12, "Fix login redirect", &["bug"]),
        issue(975, "Dependency picker", &["tui"]),
    ]);
    w.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    w.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
    w.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    let picked = w.dep_checked_numbers();
    w.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    let body = w.render_body_markdown();
    assert_eq!(crate::orchestration::dag::parse_blocked_by(&body), picked);
    assert_eq!(picked, vec![12, 975]);
}

#[test]
fn gate_and_checklist_count_placeholders_the_same_way() {
    let mut s = IssueWizardScreen::new();
    *s.payload_mut() = ready_payload();
    s.payload_mut().test_hints = "[TODO: x] and [TODO: y]".into();
    s.set_step_for_tests(IssueWizardStep::Preview);
    assert_eq!(
        *s.create_gate().state(),
        ArmedState::Blocked("2 [TODO: placeholders left · ⏎ jumps to the first".into())
    );
    let row = s.checklist_rows().pop().unwrap();
    assert_eq!(row.2.as_deref(), Some("2 left"));
}

fn review_launches(app: &App) -> usize {
    app.pending_commands
        .iter()
        .filter(|c| matches!(c, TuiCommand::LaunchAiReview(_)))
        .count()
}

#[tokio::test]
async fn r_after_a_review_error_queues_exactly_one_review() {
    let mut w = IssueWizardScreen::new();
    *w.payload_mut() = ready_payload();
    w.set_step_for_tests(IssueWizardStep::AiReview);
    w.begin_ai_review();
    w.apply_ai_review(Err("claude: rate limited".into()));
    let mut app = app_with(w);
    let out = screen_text(&mut app);
    assert!(out.contains("r retries"), "{out}");

    press(&mut app, KeyCode::Char('r')).await;
    assert_eq!(review_launches(&app), 1, "r sends one AI review");
    assert!(wizard(&app).review_loading());
    assert!(wizard(&app).review_error().is_none());
    let out = screen_text(&mut app);
    assert!(out.contains("AI is reviewing your issue"), "{out}");
    assert!(
        !out.contains("Enter next"),
        "Enter does nothing while loading:\n{out}"
    );
    assert!(
        !out.contains("improve"),
        "i does nothing while loading:\n{out}"
    );

    press(&mut app, KeyCode::Char('r')).await;
    assert_eq!(review_launches(&app), 1, "r while loading sends nothing");

    // The result lands; the review is usable again.
    app.screen_state
        .issue_wizard_screen
        .as_mut()
        .unwrap()
        .apply_ai_review(Ok("critique".into()));
    assert!(wizard(&app).can_begin_improve());
}

/// #1454: the wizard at AI review with a ready draft, reached through
/// `handle_key` (⏎ on an empty Blocked-by list), so the step-entry hook runs.
async fn entered_ai_review() -> App {
    let mut w = IssueWizardScreen::new();
    *w.payload_mut() = ready_payload();
    w.set_step_for_tests(IssueWizardStep::Dependencies);
    w.begin_dependency_fetch();
    w.apply_dep_issues(Vec::new());
    let mut app = app_with(w);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(wizard(&app).step(), IssueWizardStep::AiReview);
    app
}

#[tokio::test]
async fn entering_ai_review_spends_nothing() {
    let mut app = entered_ai_review().await;
    assert_eq!(review_launches(&app), 0, "entering the step sends no call");
    assert!(!wizard(&app).review_loading());
    let out = screen_text(&mut app);
    assert!(out.contains("r runs the review"), "names the key:\n{out}");
    assert!(out.contains("paid AI call"), "names the cost:\n{out}");
    assert!(
        !out.contains("i improve"),
        "i does nothing before a critique, so it is not shown:\n{out}"
    );

    // `e` revise and `s` skip still work, and still send nothing.
    press(&mut app, KeyCode::Char('e')).await;
    assert_eq!(wizard(&app).step(), IssueWizardStep::BasicInfo);
    let mut app = entered_ai_review().await;
    press(&mut app, KeyCode::Char('s')).await;
    assert_eq!(wizard(&app).step(), IssueWizardStep::Preview);
    assert_eq!(review_launches(&app), 0, "skip sends no call");
}

#[tokio::test]
async fn review_key_queues_one_review() {
    let mut app = entered_ai_review().await;
    assert_eq!(review_launches(&app), 0, "nothing before the key");
    press(&mut app, KeyCode::Char('r')).await;
    assert_eq!(review_launches(&app), 1, "r sends one AI review");
    assert!(wizard(&app).review_loading());
    let out = screen_text(&mut app);
    assert!(out.contains("AI is reviewing your issue"), "{out}");

    press(&mut app, KeyCode::Char('r')).await;
    assert_eq!(review_launches(&app), 1, "r while loading sends nothing");

    app.screen_state
        .issue_wizard_screen
        .as_mut()
        .unwrap()
        .apply_ai_review(Ok("critique".into()));
    press(&mut app, KeyCode::Char('r')).await;
    assert_eq!(review_launches(&app), 1, "r with a critique sends nothing");
    assert_eq!(wizard(&app).review_text(), Some("critique"));
}
