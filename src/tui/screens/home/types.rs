use crate::tui::app::TuiMode;

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub repo: String,
    pub branch: String,
    pub username: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub issue_number: u64,
    pub title: String,
    pub status: String,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionKind {
    ReadyIssues {
        count: usize,
    },
    MilestoneProgress {
        title: String,
        closed: u32,
        total: u32,
    },
    IdleSessions,
    FailedIssues {
        count: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub kind: SuggestionKind,
    pub message: String,
    pub action: TuiMode,
}

impl Suggestion {
    pub fn build_suggestions(
        ready_issue_count: usize,
        failed_issue_count: usize,
        milestones: &[(String, u32, u32)],
        active_session_count: usize,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        if ready_issue_count > 0 {
            suggestions.push(Suggestion {
                kind: SuggestionKind::ReadyIssues {
                    count: ready_issue_count,
                },
                message: format!(
                    "{} issue{} labeled maestro:ready — press [i] to browse",
                    ready_issue_count,
                    if ready_issue_count == 1 { "" } else { "s" }
                ),
                action: TuiMode::IssueBrowser,
            });
        }

        for (title, closed, total) in milestones {
            if *total > 0 {
                let pct = (*closed as f64 / *total as f64 * 100.0).clamp(0.0, 100.0) as u32;
                suggestions.push(Suggestion {
                    kind: SuggestionKind::MilestoneProgress {
                        title: title.clone(),
                        closed: *closed,
                        total: *total,
                    },
                    message: format!(
                        "Milestone {} is {}% complete ({}/{} closed)",
                        title, pct, closed, total
                    ),
                    action: TuiMode::MilestoneView,
                });
            }
        }

        if failed_issue_count > 0 {
            suggestions.push(Suggestion {
                kind: SuggestionKind::FailedIssues {
                    count: failed_issue_count,
                },
                message: format!(
                    "{} issue{} labeled maestro:failed — press [i] to review",
                    failed_issue_count,
                    if failed_issue_count == 1 { "" } else { "s" }
                ),
                action: TuiMode::IssueBrowser,
            });
        }

        if active_session_count == 0 {
            suggestions.push(Suggestion {
                kind: SuggestionKind::IdleSessions,
                message: "No sessions running — press [r] to start".to_string(),
                action: TuiMode::Overview,
            });
        }

        suggestions
    }
}
