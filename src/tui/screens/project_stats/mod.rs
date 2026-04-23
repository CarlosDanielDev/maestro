mod draw;
pub mod types;

pub use types::{
    IssueCounts, MilestoneProgress, ProjectStatsData, RecentActivityRow, SessionMetrics,
};

use crate::provider::github::types::GhMilestone;
use crate::session::types::{Session, SessionStatus};

use super::{Screen, ScreenAction};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};

pub struct ProjectStatsScreen {
    pub loading: bool,
    pub data: ProjectStatsData,
    pub(super) scroll_offset: usize,
}

impl ProjectStatsScreen {
    pub fn new() -> Self {
        Self {
            loading: true,
            data: ProjectStatsData::default(),
            scroll_offset: 0,
        }
    }

    pub fn set_data(&mut self, data: ProjectStatsData) {
        self.data = data;
        self.loading = false;
        self.scroll_offset = 0;
    }

    fn max_scroll(&self) -> usize {
        self.data.recent_activity.len().saturating_sub(1)
    }
}

impl Default for ProjectStatsScreen {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure aggregation helper for the background fetch task. Lives here
/// (not in `tui/mod.rs`) so unit tests can exercise the math without
/// spawning tokio.
pub fn aggregate(
    open_count: u32,
    closed_count: u32,
    ready_count: u32,
    failed_count: u32,
    done_count: u32,
    milestones: Vec<GhMilestone>,
    local_sessions: &[Session],
) -> ProjectStatsData {
    let milestone_progress = milestones
        .into_iter()
        .map(|m| MilestoneProgress {
            title: m.title,
            closed: m.closed_issues,
            total: m.open_issues + m.closed_issues,
        })
        .collect();

    let mut sessions = SessionMetrics::default();
    for s in local_sessions {
        sessions.total_sessions += 1;
        if matches!(s.status, SessionStatus::Completed) {
            sessions.completed_sessions += 1;
        }
        sessions.total_cost_usd += s.cost_usd;
        sessions.total_input_tokens += s.token_usage.input_tokens;
        sessions.total_output_tokens += s.token_usage.output_tokens;
    }

    let mut activity: Vec<RecentActivityRow> = local_sessions
        .iter()
        .rev()
        .take(10)
        .map(|s| RecentActivityRow {
            issue_number: s.issue_number,
            label: s
                .issue_title
                .clone()
                .unwrap_or_else(|| s.last_message.clone()),
            status: s.status.label().to_string(),
            cost_usd: s.cost_usd,
            elapsed: format_elapsed(s),
        })
        .collect();
    activity.truncate(10);

    ProjectStatsData {
        milestones: milestone_progress,
        issues: IssueCounts {
            open: open_count,
            closed: closed_count,
            ready: ready_count,
            failed: failed_count,
            done: done_count,
        },
        sessions,
        recent_activity: activity,
    }
}

fn format_elapsed(s: &Session) -> String {
    let Some(start) = s.started_at else {
        return "—".to_string();
    };
    let end = s.finished_at.unwrap_or_else(chrono::Utc::now);
    let secs = (end - start).num_seconds().max(0);
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

impl KeymapProvider for ProjectStatsScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Project Stats",
            bindings: vec![
                KeyBinding {
                    key: "j/Down",
                    description: "Scroll recent activity down",
                },
                KeyBinding {
                    key: "k/Up",
                    description: "Scroll recent activity up",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Back",
                },
            ],
        }]
    }
}

impl Screen for ProjectStatsScreen {
    fn handle_input(&mut self, event: &Event, mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            if mode == InputMode::Insert {
                return ScreenAction::None;
            }
            match code {
                KeyCode::Char('j') | KeyCode::Down if self.scroll_offset < self.max_scroll() => {
                    self.scroll_offset += 1;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
                KeyCode::Esc => return ScreenAction::Pop,
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        Some(InputMode::Normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;

    fn make_data() -> ProjectStatsData {
        ProjectStatsData {
            milestones: vec![MilestoneProgress {
                title: "v0.15.0".into(),
                closed: 3,
                total: 12,
            }],
            issues: IssueCounts {
                open: 5,
                closed: 10,
                ready: 2,
                failed: 1,
                done: 8,
            },
            sessions: SessionMetrics {
                total_sessions: 4,
                completed_sessions: 3,
                total_cost_usd: 1.23,
                total_input_tokens: 100,
                total_output_tokens: 200,
            },
            recent_activity: vec![
                RecentActivityRow {
                    issue_number: Some(290),
                    label: "Landing".into(),
                    status: "completed".into(),
                    cost_usd: 0.5,
                    elapsed: "1m".into(),
                },
                RecentActivityRow {
                    issue_number: Some(291),
                    label: "Wizard".into(),
                    status: "completed".into(),
                    cost_usd: 0.7,
                    elapsed: "2m".into(),
                },
            ],
        }
    }

    #[test]
    fn new_starts_loading() {
        let s = ProjectStatsScreen::new();
        assert!(s.loading);
    }

    #[test]
    fn set_data_clears_loading_and_resets_scroll() {
        let mut s = ProjectStatsScreen::new();
        s.scroll_offset = 5;
        s.set_data(make_data());
        assert!(!s.loading);
        assert_eq!(s.scroll_offset, 0);
    }

    #[test]
    fn esc_returns_pop() {
        let mut s = ProjectStatsScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn j_scrolls_recent_activity_down() {
        let mut s = ProjectStatsScreen::new();
        s.set_data(make_data());
        s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(s.scroll_offset, 1);
    }

    #[test]
    fn j_does_not_scroll_past_end() {
        let mut s = ProjectStatsScreen::new();
        s.set_data(make_data());
        for _ in 0..10 {
            s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(s.scroll_offset, s.max_scroll());
    }

    #[test]
    fn k_does_not_scroll_below_zero() {
        let mut s = ProjectStatsScreen::new();
        s.set_data(make_data());
        s.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(s.scroll_offset, 0);
    }

    #[test]
    fn milestone_progress_ratio_clamps_at_one() {
        let m = MilestoneProgress {
            title: "x".into(),
            closed: 20,
            total: 10,
        };
        assert_eq!(m.ratio(), 1.0);
        assert_eq!(m.percent(), 100);
    }

    #[test]
    fn milestone_progress_zero_total_returns_zero_ratio() {
        let m = MilestoneProgress {
            title: "x".into(),
            closed: 0,
            total: 0,
        };
        assert_eq!(m.ratio(), 0.0);
    }

    #[test]
    fn milestone_percent_rounds_to_nearest() {
        let m = MilestoneProgress {
            title: "x".into(),
            closed: 1,
            total: 3,
        };
        assert_eq!(m.percent(), 33);
    }

    #[test]
    fn session_metrics_success_rate_handles_empty() {
        let m = SessionMetrics::default();
        assert_eq!(m.success_rate(), 0.0);
    }

    #[test]
    fn session_metrics_success_rate_calculates() {
        let m = SessionMetrics {
            total_sessions: 4,
            completed_sessions: 3,
            ..Default::default()
        };
        assert!((m.success_rate() - 0.75).abs() < 1e-9);
    }
}
