use super::*;
use crate::tui::theme::Theme;
use crate::tui::token_dashboard::draw_token_dashboard_with_badges;
use insta::assert_snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn token_dashboard_quota_badges_per_provider_120x40() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let badges = vec![
        "claude 90% ↻2h30m ⇑".to_string(),
        "qwen 40% ↻2h30m → stale".to_string(),
        "minimax no data".to_string(),
    ];

    terminal
        .draw(|f| {
            draw_token_dashboard_with_badges(f, &[], 0.0, &badges, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn token_dashboard_without_badges_keeps_plain_title_80x24() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            draw_token_dashboard_with_badges(f, &[], 0.0, &[], f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}
