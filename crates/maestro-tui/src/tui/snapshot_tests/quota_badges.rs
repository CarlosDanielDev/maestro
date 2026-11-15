use super::*;
use crate::agent_provider::quota::badge;
use crate::agent_provider::quota::types::{ProviderWindows, QuotaWindow};
use crate::tui::theme::Theme;
use crate::tui::token_dashboard::draw_token_dashboard_with_badges;
use chrono::{Duration, TimeZone, Utc};
use insta::assert_snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn token_dashboard_quota_badges_per_provider_120x40() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    // Badges come from the real badge function, so a rendering change shows up here.
    let now = Utc.with_ymd_and_hms(2026, 10, 17, 12, 0, 0).unwrap();
    let real_window = ProviderWindows {
        five_hour: Some(QuotaWindow {
            utilization_pct: 90.0,
            resets_at: now + Duration::minutes(150),
        }),
        seven_day: None,
        seven_day_sonnet: None,
        stale: false,
    };
    let stale_window = ProviderWindows {
        stale: true,
        five_hour: Some(QuotaWindow {
            utilization_pct: 40.0,
            resets_at: now + Duration::minutes(150),
        }),
        ..real_window.clone()
    };
    let badges = vec![
        badge::badge("minimax", None, now),
        badge::badge("qwen", Some(&stale_window), now),
        badge::badge("claude", Some(&real_window), now),
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
