//! #1282 key tests for the five-step issue wizard: the placeholder lint and
//! the armed create on Preview, the Blocked-by filter and the per-section
//! AI merge. App-level tests go through `input_handler::handle_key` (so the
//! dispatch hook that queues `TuiCommand::CreateIssue` runs) and draw
//! through `ui::draw` at 120x30 (above the 80x24 SizeGuard).

use super::*;
use crate::tui::app::{App, TuiCommand, TuiMode};
use crate::tui::ds::armed::ArmedState;
use crate::tui::ds::checklist::CheckState;
use crate::tui::input_handler::handle_key;
use crate::tui::screens::test_helpers::key_event;
use ratatui::{Terminal, backend::TestBackend};
use std::time::Duration;

fn ready_payload() -> IssueCreationPayload {
    IssueCreationPayload {
        title: "Add gauge widget".into(),
        overview: "Render progress".into(),
        expected_behavior: "Fills proportionally".into(),
        acceptance_criteria: "- Renders 0..100".into(),
        ..Default::default()
    }
}

fn app_with(wizard: IssueWizardScreen) -> App {
    let mut app = crate::tui::make_test_app("issue-wizard-1282");
    app.tui_mode = TuiMode::IssueWizard;
    app.screen_state.issue_wizard_screen = Some(wizard);
    app
}

fn on_preview(payload: IssueCreationPayload) -> App {
    let mut wizard = IssueWizardScreen::new();
    *wizard.payload_mut() = payload;
    wizard.set_step_for_tests(IssueWizardStep::Preview);
    app_with(wizard)
}

fn wizard(app: &App) -> &IssueWizardScreen {
    app.screen_state.issue_wizard_screen.as_ref().unwrap()
}

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

fn create_queued(app: &App) -> bool {
    app.pending_commands
        .iter()
        .any(|c| matches!(c, TuiCommand::CreateIssue(_)))
}

fn screen_text(app: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    t.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", t.backend())
}

#[tokio::test]
async fn issue_create_blocked_by_placeholder() {
    let mut payload = ready_payload();
    payload.acceptance_criteria = "- Renders 0..100\n- [TODO: overflow rule]".into();
    let mut app = on_preview(payload);

    let out = screen_text(&mut app);
    assert!(out.contains("BLOCKED"), "gate shows blocked:\n{out}");
    assert!(out.contains("1 [TODO: placeholder left"), "{out}");
    assert!(matches!(
        wizard(&app).create_gate().state(),
        ArmedState::Blocked(_)
    ));

    press(&mut app, KeyCode::Enter).await;
    assert!(!create_queued(&app), "a placeholder never reaches GitHub");
    assert!(!wizard(&app).create_in_flight());
    // Enter jumps to the field that holds the placeholder.
    assert_eq!(wizard(&app).step(), IssueWizardStep::DorFields);
    assert_eq!(
        wizard(&app).focused_field(),
        Some(FieldId::AcceptanceCriteria)
    );
    let row = wizard(&app).checklist_rows().pop().unwrap();
    assert_eq!(
        (row.0, row.1, row.2.as_deref()),
        (
            "No [TODO] placeholders",
            CheckState::Missing,
            Some("1 left")
        ),
        "the checklist still shows it"
    );
    let out = screen_text(&mut app);
    assert!(
        out.contains("No [TODO] placeholders 1 left"),
        "checklist:\n{out}"
    );

    // Every field is linted, the title too.
    let mut payload = ready_payload();
    payload.title = "feat: [TODO: name the feature]".into();
    let mut app = on_preview(payload);
    for _ in 0..3 {
        if wizard(&app).step() == IssueWizardStep::Preview {
            press(&mut app, KeyCode::Enter).await;
        }
    }
    assert!(!create_queued(&app));
    assert_eq!(wizard(&app).focused_field(), Some(FieldId::Title));
}

#[tokio::test]
async fn issue_create_armed() {
    let mut app = on_preview(ready_payload());
    assert_eq!(*wizard(&app).create_gate().state(), ArmedState::Ready);

    press(&mut app, KeyCode::Enter).await;
    assert!(!create_queued(&app), "the first Enter only arms");
    assert!(matches!(
        wizard(&app).create_gate().state(),
        ArmedState::Armed(_)
    ));
    let out = screen_text(&mut app);
    assert!(out.contains("ARMED"), "armed bar:\n{out}");
    assert!(out.contains("creates 1 issue on GitHub"), "{out}");

    press(&mut app, KeyCode::Enter).await;
    assert!(create_queued(&app), "the second Enter within 3s creates");
    assert_eq!(wizard(&app).step(), IssueWizardStep::Creating);

    // Esc disarms; a second Enter after the window re-arms instead.
    let t0 = Instant::now();
    let mut s = IssueWizardScreen::new();
    *s.payload_mut() = ready_payload();
    s.set_step_for_tests(IssueWizardStep::Preview);
    s.set_clock_for_test(t0);
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(*s.create_gate().state(), ArmedState::Ready);
    assert_eq!(s.step(), IssueWizardStep::Preview, "Esc disarms, stays");
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    s.set_clock_for_test(t0 + Duration::from_secs(4));
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert!(!s.create_in_flight(), "a late second Enter re-arms");
    s.set_clock_for_test(t0 + Duration::from_secs(6));
    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert!(s.create_in_flight());
}

fn issue(number: u64, title: &str, labels: &[&str]) -> crate::provider::types::Issue {
    crate::provider::types::Issue {
        number,
        title: title.into(),
        body: String::new(),
        labels: labels.iter().map(|l| l.to_string()).collect(),
        state: "open".into(),
        html_url: String::new(),
        milestone: None,
        assignees: vec![],
    }
}

#[tokio::test]
async fn blocked_by_filter_keeps_selection() {
    let mut w = IssueWizardScreen::new();
    *w.payload_mut() = ready_payload();
    w.set_step_for_tests(IssueWizardStep::Dependencies);
    w.begin_dependency_fetch();
    w.apply_dep_issues(vec![
        issue(12, "Fix login redirect", &["bug"]),
        issue(975, "Dependency picker", &["tui"]),
        issue(40, "Docs pass", &["docs"]),
    ]);
    let mut app = app_with(w);

    press(&mut app, KeyCode::Char(' ')).await; // select #12
    press(&mut app, KeyCode::Char('/')).await;
    assert!(wizard(&app).dep_filter_editing());
    for c in "975".chars() {
        press(&mut app, KeyCode::Char(c)).await;
    }
    assert_eq!(wizard(&app).dep_filter_text(), "975");
    assert_eq!(wizard(&app).dep_visible(), vec![1]);

    let out = screen_text(&mut app);
    assert!(out.contains("Selected 1"), "chip count:\n{out}");
    assert!(out.contains(" #12 "), "#12 stays as a chip:\n{out}");
    assert!(!out.contains("Fix login redirect"), "filtered out:\n{out}");
    assert!(out.contains("Dependency picker"), "{out}");

    // Labels match too; Space toggles the filtered row, not row 0.
    press(&mut app, KeyCode::Enter).await; // keep the filter, leave the field
    press(&mut app, KeyCode::Char(' ')).await;
    assert!(wizard(&app).dep_is_checked(975));
    assert!(wizard(&app).dep_is_checked(12));

    // Esc in the filter clears the text, never the selection.
    press(&mut app, KeyCode::Char('/')).await;
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(wizard(&app).dep_filter_text(), "");
    assert_eq!(wizard(&app).dep_checked_numbers(), vec![12, 975]);
    assert_eq!(wizard(&app).step(), IssueWizardStep::Dependencies);

    press(&mut app, KeyCode::Char('/')).await;
    for c in "docs".chars() {
        press(&mut app, KeyCode::Char(c)).await;
    }
    assert_eq!(wizard(&app).dep_visible(), vec![2], "label match");
    press(&mut app, KeyCode::Enter).await;
    press(&mut app, KeyCode::Enter).await; // advance
    assert_eq!(wizard(&app).payload().blocked_by, vec![12, 975]);
    assert_eq!(wizard(&app).step(), IssueWizardStep::AiReview);
}

#[tokio::test]
async fn ai_review_per_section_accept() {
    let mut w = IssueWizardScreen::new();
    *w.payload_mut() = ready_payload();
    w.payload_mut().blocked_by = vec![10];
    w.set_step_for_tests(IssueWizardStep::AiReview);
    w.begin_ai_review();
    w.apply_ai_review(Ok("critique".into()));
    w.begin_improve();
    let mut candidate = ready_payload();
    candidate.title = "AI title".into();
    candidate.overview = "AI overview".into();
    candidate.acceptance_criteria = "- [TODO: AI criteria]".into();
    candidate.blocked_by = vec![10];
    w.apply_improve_result(Ok(candidate));
    let mut app = app_with(w);

    // Sections: Title (focused), Overview, Acceptance Criteria.
    assert_eq!(wizard(&app).changed_sections(), vec![0, 1, 5]);
    press(&mut app, KeyCode::Char('j')).await;
    press(&mut app, KeyCode::Char('a')).await; // accept Overview
    press(&mut app, KeyCode::Char('j')).await;
    press(&mut app, KeyCode::Char('d')).await; // keep my Acceptance Criteria
    let out = screen_text(&mut app);
    assert!(out.contains("accepted"), "{out}");
    assert!(out.contains("kept mine"), "{out}");
    assert!(out.contains("! 1 [TODO:"), "placeholder flagged:\n{out}");

    press(&mut app, KeyCode::Enter).await; // merge accepted only
    let p = wizard(&app).payload();
    assert_eq!(p.overview, "AI overview", "accepted section merged");
    assert_eq!(p.title, "Add gauge widget", "undecided keeps mine");
    assert_eq!(p.acceptance_criteria, "- Renders 0..100", "kept mine");
    assert_eq!(p.blocked_by, vec![10], "trusted seat untouched");
    assert!(wizard(&app).improve_candidate().is_none());
    assert_eq!(wizard(&app).step(), IssueWizardStep::AiReview);
}

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
