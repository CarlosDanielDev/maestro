use super::*;
use insta::assert_snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::Paragraph;

fn item<'a>(label: &'a str, state: CheckState, note: Option<&'a str>) -> CheckItem<'a> {
    CheckItem { label, state, note }
}

/// One row per state, as the issue wizard would show it mid-draft.
fn four_states() -> Vec<CheckItem<'static>> {
    vec![
        item("Title", CheckState::Ok, None),
        item("Acceptance", CheckState::Missing, Some("required")),
        item("Files to modify", CheckState::Warn, Some("no paths")),
        item("Blocked by", CheckState::Pending, Some("step 4")),
    ]
}

/// Five rows, three ready: the `Draft 3/5 ready` case.
fn draft_3_of_5() -> Vec<CheckItem<'static>> {
    vec![
        item("Title", CheckState::Ok, None),
        item("Overview", CheckState::Ok, None),
        item("Acceptance", CheckState::Missing, Some("required")),
        item("Expected", CheckState::Ok, None),
        item("No [TODO] placeholders", CheckState::Warn, Some("2 left")),
    ]
}

fn draw_panel(theme: &Theme, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|f| checklist_panel(f, f.area(), "Draft", &four_states(), false, theme))
        .unwrap();
    terminal
}

fn draw_strip(theme: &Theme, w: u16, compact: bool) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(w, 1)).unwrap();
    let line = ready_strip("Draft", &draft_3_of_5(), compact, w, theme);
    terminal
        .draw(|f| f.render_widget(Paragraph::new(line), f.area()))
        .unwrap();
    terminal
}

fn strip_text(line: &Line) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

#[test]
fn ready_count_counts_only_ok_and_every_row_in_total() {
    assert_eq!(ready_count(&draft_3_of_5()), (3, 5));
    assert_eq!(ready_count(&four_states()), (1, 4));
    assert_eq!(ready_count(&[]), (0, 0));
}

#[test]
fn checklist_panel_states() {
    let theme = Theme::dark();
    let terminal = draw_panel(&theme, 40, 6);
    let buf = terminal.backend().buffer();
    // Pending rows are never red: find the `○` cell and check its colour.
    let pending = buf
        .content()
        .iter()
        .find(|c| c.symbol() == "○")
        .expect("pending glyph drawn");
    assert_ne!(pending.fg, theme.accent_error, "pending must not be red");
    assert_eq!(pending.fg, theme.text_muted);
    assert_snapshot!(terminal.backend());
}

#[test]
fn ready_strip_compact() {
    let theme = Theme::dark();
    let line = ready_strip("Draft", &draft_3_of_5(), true, 60, &theme);
    let text = strip_text(&line);
    assert!(text.starts_with(" Draft 3/5 ready"), "got {text:?}");
    assert!(!text.contains("required"), "compact hides notes: {text:?}");
    assert_snapshot!(draw_strip(&theme, 60, true).backend());
}

#[test]
fn ready_strip_regular_shows_notes() {
    let theme = Theme::dark();
    let text = strip_text(&ready_strip("Draft", &draft_3_of_5(), false, 120, &theme));
    assert_eq!(
        text,
        " Draft 3/5 ready   ✗ Acceptance required   ! No [TODO] placeholders 2 left"
    );
}

#[test]
fn ready_strip_cuts_with_ellipsis_when_too_narrow() {
    let theme = Theme::dark();
    let text = strip_text(&ready_strip("Draft", &draft_3_of_5(), true, 36, &theme));
    assert_eq!(text, " Draft 3/5 ready   ✗ Acceptance   …");
    assert!(text.chars().count() <= 36, "fits the width: {text:?}");
}

#[test]
fn ready_strip_all_done_has_no_problem_rows() {
    let theme = Theme::dark();
    let done = [item("Title", CheckState::Ok, None)];
    let line = ready_strip("Draft", &done, false, 40, &theme);
    assert_eq!(strip_text(&line), " Draft 1/1 ready");
    assert_eq!(line.spans[1].style.fg, Some(theme.accent_success));
}

#[test]
fn checklist_no_color() {
    let theme = Theme::no_color();
    let mut terminal = Terminal::new(TestBackend::new(44, 7)).unwrap();
    let strip = ready_strip("Draft", &draft_3_of_5(), true, 44, &theme);
    terminal
        .draw(|f| {
            let [panel, row] =
                Layout::vertical([Constraint::Length(6), Constraint::Length(1)]).areas(f.area());
            checklist_panel(f, panel, "Draft", &four_states(), true, &theme);
            f.render_widget(Paragraph::new(strip), row);
        })
        .unwrap();
    let out = format!("{:?}", terminal.backend().buffer());
    for colour in ["Green", "Yellow", "Red", "Cyan", "Rgb", "Indexed"] {
        assert!(!out.contains(colour), "NoColor drew {colour}: {out}");
    }
    assert_snapshot!(out);
}
