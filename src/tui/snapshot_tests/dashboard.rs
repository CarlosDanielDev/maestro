use super::*;
use crate::tui::screens::home::{HomeScreen, ProjectInfo, Suggestion};
use crate::tui::screens::Screen;
use crate::tui::theme::Theme;
use insta::assert_snapshot;

fn make_project_info() -> ProjectInfo {
    ProjectInfo {
        repo: "owner/repo".to_string(),
        branch: "main".to_string(),
        username: Some("carlos".to_string()),
    }
}

#[test]
fn home_screen_baseline() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn home_screen_with_warnings() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = HomeScreen::new(
        make_project_info(),
        vec![],
        vec!["Budget 90% consumed".to_string()],
    );

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn home_screen_with_suggestions() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
    screen.set_suggestions(Suggestion::build_suggestions(3, 0, &[], 0));

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn home_screen_selected_action() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
    screen.selected_action = 1;

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}
