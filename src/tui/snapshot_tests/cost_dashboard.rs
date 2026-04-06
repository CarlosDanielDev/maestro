use super::*;
use crate::session::types::SessionStatus;
use crate::tui::cost_dashboard::draw_cost_dashboard;
use crate::tui::theme::Theme;
use insta::assert_snapshot;

#[test]
fn cost_dashboard_no_budget() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let s1 = make_session(SessionStatus::Completed, Some(1));
    let mut s2 = make_session(SessionStatus::Running, Some(2));
    s2.id = uuid::Uuid::from_u128(2);
    s2.issue_title = Some("Fix crash".to_string());
    s2.cost_usd = 1.11;

    terminal
        .draw(|f| {
            draw_cost_dashboard(f, &[&s1, &s2], 1.23, None, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn cost_dashboard_with_budget_under_threshold() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let session = make_session(SessionStatus::Completed, Some(1));

    terminal
        .draw(|f| {
            draw_cost_dashboard(f, &[&session], 1.0, Some(10.0), f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn cost_dashboard_budget_over_90_percent() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut session = make_session(SessionStatus::Running, Some(1));
    session.cost_usd = 9.50;

    terminal
        .draw(|f| {
            draw_cost_dashboard(f, &[&session], 9.5, Some(10.0), f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn cost_dashboard_empty_sessions() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            draw_cost_dashboard(f, &[], 0.0, None, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn cost_dashboard_multiple_sessions_sorted_by_cost() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();

    let mut s1 = make_session(SessionStatus::Completed, Some(1));
    s1.id = uuid::Uuid::from_u128(1);
    s1.issue_title = Some("Cheap task".to_string());
    s1.cost_usd = 0.10;

    let mut s2 = make_session(SessionStatus::Completed, Some(2));
    s2.id = uuid::Uuid::from_u128(2);
    s2.issue_title = Some("Expensive task".to_string());
    s2.cost_usd = 0.50;

    let mut s3 = make_session(SessionStatus::Running, Some(3));
    s3.id = uuid::Uuid::from_u128(3);
    s3.issue_title = Some("Medium task".to_string());
    s3.cost_usd = 0.30;

    terminal
        .draw(|f| {
            draw_cost_dashboard(f, &[&s1, &s2, &s3], 0.90, Some(5.0), f.area(), &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    let pos_expensive = output.find("Expensive task").expect("Should contain Expensive task");
    let pos_medium = output.find("Medium task").expect("Should contain Medium task");
    let pos_cheap = output.find("Cheap task").expect("Should contain Cheap task");
    assert!(
        pos_expensive < pos_medium && pos_medium < pos_cheap,
        "Sessions should be sorted by cost descending"
    );
    assert_snapshot!(terminal.backend());
}
