use super::*;
use crate::session::types::SessionStatus;
use crate::tui::panels::PanelView;
use crate::tui::theme::Theme;
use insta::assert_snapshot;

#[test]
fn panel_view_empty_sessions() {
    let mut terminal = test_terminal();
    let panel = PanelView::new();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            panel.draw(f, &[], f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn panel_view_single_running_session() {
    let mut terminal = test_terminal();
    let panel = PanelView::new();
    let theme = Theme::dark();
    let session = make_session(SessionStatus::Running, Some(42));

    terminal
        .draw(|f| {
            panel.draw(f, &[&session], f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn panel_view_multiple_sessions() {
    let mut terminal = test_terminal();
    let panel = PanelView::new();
    let theme = Theme::dark();

    let mut s1 = make_session(SessionStatus::Running, Some(1));
    s1.id = uuid::Uuid::from_u128(1);
    s1.issue_title = Some("Add login flow".to_string());

    let mut s2 = make_session(SessionStatus::Completed, Some(2));
    s2.id = uuid::Uuid::from_u128(2);
    s2.issue_title = Some("Fix database crash".to_string());
    s2.cost_usd = 0.50;

    let mut s3 = make_session(SessionStatus::Errored, Some(3));
    s3.id = uuid::Uuid::from_u128(3);
    s3.issue_title = Some("Add logout endpoint".to_string());
    s3.cost_usd = 0.03;

    terminal
        .draw(|f| {
            panel.draw(f, &[&s1, &s2, &s3], f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn panel_view_selected_session() {
    let mut terminal = test_terminal();
    let mut panel = PanelView::new();
    panel.selected = Some(0);
    let theme = Theme::dark();

    let s1 = make_session(SessionStatus::Running, Some(10));
    let mut s2 = make_session(SessionStatus::Running, Some(11));
    s2.id = uuid::Uuid::from_u128(2);
    s2.issue_title = Some("Another task".to_string());

    terminal
        .draw(|f| {
            panel.draw(f, &[&s1, &s2], f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn panel_view_context_overflow() {
    let mut terminal = test_terminal();
    let panel = PanelView::new();
    let theme = Theme::dark();

    let mut session = make_session(SessionStatus::Running, Some(42));
    session.context_pct = 0.85;

    terminal
        .draw(|f| {
            panel.draw(f, &[&session], f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn panel_view_forked_session() {
    let mut terminal = test_terminal();
    let panel = PanelView::new();
    let theme = Theme::dark();

    let mut session = make_session(SessionStatus::Running, Some(42));
    session.parent_session_id = Some(uuid::Uuid::from_u128(99));
    session.fork_depth = 2;

    terminal
        .draw(|f| {
            panel.draw(f, &[&session], f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}
