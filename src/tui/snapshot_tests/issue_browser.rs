use super::*;
use crate::tui::screens::issue_browser::{FilterMode, IssueBrowserScreen};
use crate::tui::screens::Screen;
use crate::tui::theme::Theme;
use insta::assert_snapshot;

#[test]
fn issue_browser_with_issues() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
        make_gh_issue(3, "Add logout endpoint"),
    ]);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_empty_list() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![]);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_loading_state() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![]);
    screen.loading = true;

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_multi_select() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
        make_gh_issue(3, "Add logout endpoint"),
    ]);
    screen.selected_set.insert(1);
    screen.selected_set.insert(3);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_filter_active() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
        make_gh_issue(3, "Add logout endpoint"),
    ]);
    screen.filter_mode = FilterMode::Label;
    screen.filter_text = "Add".to_string();

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}
