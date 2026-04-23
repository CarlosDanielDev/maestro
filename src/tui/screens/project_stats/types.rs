/// One row in the milestone progress section. Closed/total drive the gauge.
#[derive(Debug, Clone, Default)]
pub struct MilestoneProgress {
    pub title: String,
    pub closed: u32,
    pub total: u32,
}

impl MilestoneProgress {
    /// Progress fraction in `0.0..=1.0`. Empty milestones report `0.0`.
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.closed as f64 / self.total as f64).min(1.0)
        }
    }

    pub fn percent(&self) -> u16 {
        (self.ratio() * 100.0).round() as u16
    }
}

/// Aggregated session metrics. Built from the local `MaestroState` so the
/// stats screen can render without needing GitHub.
#[derive(Debug, Clone, Default)]
pub struct SessionMetrics {
    pub total_sessions: usize,
    pub completed_sessions: usize,
    pub total_cost_usd: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

impl SessionMetrics {
    /// Success rate as a fraction. Empty sessions report `0.0`.
    pub fn success_rate(&self) -> f64 {
        if self.total_sessions == 0 {
            0.0
        } else {
            self.completed_sessions as f64 / self.total_sessions as f64
        }
    }
}

/// One row in the recent activity list.
#[derive(Debug, Clone)]
pub struct RecentActivityRow {
    pub issue_number: Option<u64>,
    pub label: String,
    pub status: String,
    pub cost_usd: f64,
    pub elapsed: String,
}

/// Counts surfaced in the issue statistics table.
#[derive(Debug, Clone, Default)]
pub struct IssueCounts {
    pub open: u32,
    pub closed: u32,
    pub ready: u32,
    pub failed: u32,
    pub done: u32,
}

/// All data shown by the Project Stats screen. Assembled by a background
/// task and shipped via `TuiDataEvent::ProjectStats`.
#[derive(Debug, Clone, Default)]
pub struct ProjectStatsData {
    pub milestones: Vec<MilestoneProgress>,
    pub issues: IssueCounts,
    pub sessions: SessionMetrics,
    pub recent_activity: Vec<RecentActivityRow>,
}
