use super::App;
use crate::tui::activity_log::LogLevel;

impl App {
    pub(super) fn dispatch_review(&mut self, pr_number: u64, branch: &str, issue_number: u64) {
        let Some(config) = &self.config else { return };
        let review_cfg = &config.review;
        if !review_cfg.enabled {
            return;
        }

        if !review_cfg.reviewers.is_empty() {
            let reviewers: Vec<crate::review::council::ReviewerConfig> = review_cfg
                .reviewers
                .iter()
                .map(|r| crate::review::council::ReviewerConfig {
                    name: r.name.clone(),
                    command: r.command.clone(),
                    required: r.required,
                })
                .collect();
            match crate::review::council::ReviewCouncil::convene(pr_number, branch, &reviewers) {
                Ok(council_result) => {
                    let status_label = match &council_result.status {
                        crate::review::council::ReviewStatus::Approved { .. } => "Council approved",
                        crate::review::council::ReviewStatus::Rejected { .. } => "Council rejected",
                        crate::review::council::ReviewStatus::Partial { .. } => "Council partial",
                    };
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("PR #{}: {}", pr_number, status_label),
                        LogLevel::Info,
                    );
                    let comment =
                        crate::review::council::ReviewCouncil::format_comment(&council_result);
                    let _ = crate::review::dispatch::ReviewDispatcher::post_comment(
                        pr_number, &comment,
                    );
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Council review failed: {}", e),
                        LogLevel::Error,
                    );
                }
            }
        } else {
            let review_config = crate::review::ReviewConfig {
                enabled: review_cfg.enabled,
                command: review_cfg.command.clone(),
            };
            let dispatcher = crate::review::ReviewDispatcher::new(review_config);
            match dispatcher.dispatch(pr_number, branch) {
                Ok(result) => {
                    let status = if result.success {
                        "Review passed"
                    } else {
                        "Review failed"
                    };
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("PR #{}: {}", pr_number, status),
                        LogLevel::Info,
                    );
                    let comment_body = format!(
                        "**Maestro Review**\n\nStatus: {}\n\n```\n{}\n```",
                        status, result.output
                    );
                    let _ = crate::review::dispatch::ReviewDispatcher::post_comment(
                        pr_number,
                        &comment_body,
                    );
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Review dispatch failed: {}", e),
                        LogLevel::Error,
                    );
                }
            }
        }
    }
}
