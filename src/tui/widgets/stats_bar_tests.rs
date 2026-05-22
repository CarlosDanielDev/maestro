use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn render_to_string(data: StatsBarData, width: u16, height: u16) -> String {
    let theme = Theme::default();
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(StatsBar::new(data, &theme), f.area());
        })
        .unwrap();
    format!("{:?}", terminal.backend())
}

fn make_loaded_data() -> StatsBarData {
    StatsBarData {
        loaded: true,
        repo: "owner/repo".to_string(),
        branch: "main".to_string(),
        username: Some("carlos".to_string()),
        issues_open: 12,
        issues_closed: 45,
        milestone_title: Some("v1.0".to_string()),
        milestone_closed: 7,
        milestone_total: 10,
        sessions_active: 2,
        sessions_total: 8,
        minimax_forced_count: None,
    }
}

#[test]
fn renders_repo_info() {
    let out = render_to_string(make_loaded_data(), 150, 3);
    assert!(out.contains("owner/repo"), "repo must appear");
    assert!(out.contains("main"), "branch must appear");
    assert!(out.contains("carlos"), "username must appear");
}

#[test]
fn renders_issue_counts() {
    let out = render_to_string(make_loaded_data(), 150, 3);
    assert!(out.contains("12"), "open issue count must appear");
    assert!(out.contains("45"), "closed issue count must appear");
}

#[test]
fn renders_milestone_progress() {
    let out = render_to_string(make_loaded_data(), 150, 3);
    assert!(out.contains("v1.0"), "milestone title must appear");
    assert!(out.contains("70%"), "milestone percentage must appear");
}

#[test]
fn renders_session_counts() {
    let out = render_to_string(make_loaded_data(), 150, 3);
    assert!(out.contains("2"), "active sessions must appear");
    assert!(out.contains("8"), "total sessions must appear");
}

// --- #845 MiniMax forced-quota footer ---

#[test]
fn renders_minimax_forced_count_when_positive() {
    let mut data = make_loaded_data();
    data.minimax_forced_count = Some(3);
    let out = render_to_string(data, 200, 3);
    assert!(
        out.contains("QUOTA"),
        "forced-quota label must appear; got: {out}"
    );
    assert!(out.contains('3'), "forced count must appear");
}

#[test]
fn hides_minimax_forced_count_when_zero() {
    let mut data = make_loaded_data();
    data.minimax_forced_count = Some(0);
    let out = render_to_string(data, 200, 3);
    assert!(
        !out.contains("QUOTA"),
        "forced-quota label must NOT appear for zero count; got: {out}"
    );
}

#[test]
fn hides_minimax_forced_count_when_none() {
    let data = make_loaded_data();
    let out = render_to_string(data, 200, 3);
    assert!(
        !out.contains("QUOTA"),
        "forced-quota label must NOT appear when None; got: {out}"
    );
}

#[test]
fn renders_loading_when_not_loaded() {
    let mut data = make_loaded_data();
    data.loaded = false;
    let out = render_to_string(data, 150, 3);
    assert!(out.contains("Loading"), "must show loading indicator");
}

#[test]
fn handles_no_milestone() {
    let mut data = make_loaded_data();
    data.milestone_title = None;
    let out = render_to_string(data, 150, 3);
    assert!(out.contains("owner/repo"), "repo must still appear");
}

#[test]
fn renders_without_panic_at_minimum_size() {
    let _ = render_to_string(make_loaded_data(), 1, 1);
}

// --- Issue #410: marquee scroll when stats bar overflows ---

fn render_with_marquee_to_string(
    data: StatsBarData,
    width: u16,
    height: u16,
    marquee: &mut MarqueeState,
    frames: u32,
) -> String {
    let theme = Theme::default();
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut last = String::new();
    for _ in 0..frames {
        terminal
            .draw(|f| {
                StatsBar::new(data.clone(), &theme).render_with_marquee(
                    f.area(),
                    f.buffer_mut(),
                    marquee,
                );
            })
            .unwrap();
        last = format!("{:?}", terminal.backend());
    }
    last
}

fn long_data() -> StatsBarData {
    StatsBarData {
        loaded: true,
        repo: "CarlosDanielDev/maestro".to_string(),
        branch: "feat/rust-development-guardrails".to_string(),
        username: Some("carlosdanieldev".to_string()),
        issues_open: 42,
        issues_closed: 137,
        milestone_title: Some("v0.14.0 TurboQuant".to_string()),
        milestone_closed: 4,
        milestone_total: 9,
        sessions_active: 3,
        sessions_total: 12,
        minimax_forced_count: None,
    }
}

#[test]
fn marquee_stays_at_pause_start_when_content_fits_wide_viewport() {
    let mut state = MarqueeState::new();
    let _ = render_with_marquee_to_string(make_loaded_data(), 200, 3, &mut state, 50);
    assert_eq!(
        state.phase,
        crate::tui::marquee::MarqueePhase::PauseStart,
        "wide viewport must never leave PauseStart"
    );
    assert_eq!(state.offset, 0);
}

#[test]
fn marquee_advances_when_content_overflows_narrow_viewport() {
    let mut state = MarqueeState::new();
    // Very narrow viewport guarantees overflow. Advance past pause_start ticks.
    let cfg = MarqueeConfig::default();
    let frames = cfg.pause_start_ticks as u32 + 20;
    let _ = render_with_marquee_to_string(long_data(), 40, 3, &mut state, frames);
    assert_ne!(
        state.offset, 0,
        "marquee must have advanced off zero after pause_start + scroll frames"
    );
}

#[test]
fn marquee_resets_when_long_data_fits_after_shrink() {
    let mut state = MarqueeState::new();
    // Advance past pause_start into scrolling so offset > 0
    let cfg = MarqueeConfig::default();
    let frames = cfg.pause_start_ticks as u32 + 20;
    let _ = render_with_marquee_to_string(long_data(), 40, 3, &mut state, frames);
    assert_ne!(state.offset, 0);

    // Now render with a wide viewport so the stats line fits again — the
    // renderer must reset the marquee back to PauseStart.
    let _ = render_with_marquee_to_string(long_data(), 300, 3, &mut state, 1);
    assert_eq!(state.offset, 0);
    assert_eq!(state.phase, crate::tui::marquee::MarqueePhase::PauseStart);
}

#[test]
fn render_with_marquee_does_not_panic_at_minimum_widths() {
    let mut state = MarqueeState::new();
    for width in [1u16, 10, 40, 80, 120] {
        let _ = render_with_marquee_to_string(long_data(), width, 3, &mut state, 5);
    }
}
