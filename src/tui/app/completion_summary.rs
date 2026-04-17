use super::App;
use super::helpers::session_label;
use super::types::{CompletionSessionLine, CompletionSummaryData, GateFailureInfo, TuiCommand};
use crate::session::types::SessionStatus;
use crate::util::truncate_with_ellipsis;

impl App {
    pub fn build_completion_summary(&self) -> CompletionSummaryData {
        use std::collections::HashMap;

        let sessions = self.pool.all_sessions();
        let mut lines = Vec::new();
        let mut total_cost = 0.0;

        let pr_map: HashMap<u64, u64> = self
            .ci_poller
            .pending_pr_checks
            .iter()
            .map(|p| (p.issue_number, p.pr_number))
            .collect();

        let repo = self
            .config
            .as_ref()
            .map(|c| c.project.repo.clone())
            .unwrap_or_default();

        for s in &sessions {
            total_cost += s.cost_usd;

            let pr_num = s
                .issue_number
                .and_then(|iss| pr_map.get(&iss).copied())
                .or_else(|| s.ci_fix_context.as_ref().map(|ctx| ctx.pr_number));

            let pr_link = match pr_num {
                Some(n) if repo.is_empty() => format!("#{}", n),
                Some(n) => format!("https://github.com/{}/pull/{}", repo, n),
                None => String::new(),
            };

            let error_summary = if s.status == SessionStatus::Errored {
                s.activity_log
                    .iter()
                    .rev()
                    .find(|e| e.message.starts_with("Error:") || e.message.starts_with("E:"))
                    .or_else(|| s.activity_log.last())
                    .map(|e| truncate_with_ellipsis(&e.message, 77))
                    .unwrap_or_default()
            } else if s.is_hollow_completion {
                "Hollow completion: no cost, no files, no tool calls".to_string()
            } else {
                String::new()
            };

            let gate_failures = if s.status == SessionStatus::NeedsReview {
                s.gate_results
                    .iter()
                    .filter(|r| !r.passed)
                    .map(|r| GateFailureInfo {
                        gate: r.gate.clone(),
                        message: truncate_with_ellipsis(&r.message, 100),
                    })
                    .collect()
            } else {
                Vec::new()
            };

            lines.push(CompletionSessionLine {
                session_id: s.id,
                label: session_label(s),
                status: s.status,
                cost_usd: s.cost_usd,
                elapsed: s.elapsed_display(),
                pr_link,
                error_summary,
                gate_failures,
                issue_number: s.issue_number,
                model: s.model.clone(),
            });
        }

        CompletionSummaryData {
            session_count: sessions.len(),
            total_cost_usd: total_cost,
            sessions: lines,
            suggestions: Vec::new(),
            selected_suggestion: 0,
        }
    }

    /// Transition from CompletionSummary to Dashboard mode.
    pub fn transition_to_dashboard(&mut self) {
        let all = self.pool.all_sessions();

        // Save session state
        self.state.sessions = all.iter().copied().cloned().collect();
        self.state.update_total_cost();
        self.state.last_updated = Some(chrono::Utc::now());
        let _ = self.store.save(&self.state);

        // Build recent sessions for the home screen
        let recent: Vec<crate::tui::screens::home::SessionSummary> = all
            .iter()
            .rev()
            .take(10)
            .map(|s| crate::tui::screens::home::SessionSummary {
                issue_number: s.issue_number.unwrap_or(0),
                title: s
                    .issue_title
                    .clone()
                    .unwrap_or_else(|| s.last_message.clone()),
                status: s.status.label().to_string(),
                cost_usd: s.cost_usd,
            })
            .collect();

        // Initialize home screen if needed (cmd_run path has no home_screen)
        if self.home_screen.is_none() {
            let project_info = crate::tui::screens::home::ProjectInfo {
                repo: self
                    .config
                    .as_ref()
                    .map(|c| c.project.repo.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                branch: std::process::Command::new("git")
                    .args(["branch", "--show-current"])
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                username: None,
            };
            self.home_screen = Some(crate::tui::screens::HomeScreen::new(
                project_info,
                recent,
                Vec::new(),
            ));
        } else if let Some(ref mut screen) = self.home_screen {
            screen.recent_sessions = recent;
        }

        // Clear completion summary and stale screen state, then switch to dashboard
        self.completion_summary = None;
        self.completion_summary_dismissed = true;
        self.issue_browser_screen = None;
        if let Some(ref mut screen) = self.home_screen {
            screen.start_loading_suggestions();
        }
        self.pending_commands.push(TuiCommand::FetchSuggestionData);
        self.navigate_to_root();
    }
}
