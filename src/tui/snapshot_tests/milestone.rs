use super::*;
use crate::tui::screens::milestone::{MilestoneEntry, MilestoneScreen};
use crate::tui::screens::Screen;
use crate::tui::theme::Theme;
use insta::assert_snapshot;

fn make_milestone(number: u64, title: &str, open: u32, closed: u32) -> MilestoneEntry {
    MilestoneEntry {
        number,
        title: title.to_string(),
        description: String::new(),
        state: "open".to_string(),
        open_issues: open,
        closed_issues: closed,
        issues: vec![],
    }
}

#[test]
fn milestone_screen_with_milestones() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = MilestoneScreen::new(vec![
        make_milestone(1, "v1.0 - Core Features", 3, 7),
        make_milestone(2, "v2.0 - Extensions", 5, 5),
    ]);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn milestone_screen_empty() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = MilestoneScreen::new(vec![]);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn milestone_screen_loading() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = MilestoneScreen::new(vec![]);
    screen.loading = true;

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn milestone_screen_with_issues_in_detail() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut entry = make_milestone(1, "v1.0", 1, 1);
    entry.issues = vec![
        make_gh_issue(10, "Implement feature A"),
        make_gh_issue(11, "Fix bug B"),
    ];
    let mut screen = MilestoneScreen::new(vec![entry]);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}
