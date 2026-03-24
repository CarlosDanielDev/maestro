use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::thread;

use super::dispatch::{ReviewDispatcher, ReviewResult};

/// Configuration for a single reviewer in the council.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerConfig {
    /// Name of the reviewer (e.g., "claude", "codex", "gemini").
    pub name: String,
    /// Command template. Variables: {pr_number}, {branch}.
    pub command: String,
    /// Whether this reviewer's approval is required for consensus.
    #[serde(default)]
    pub required: bool,
}

/// Overall status of a council review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewStatus {
    /// All required reviewers approved.
    Approved { approvals: Vec<String> },
    /// One or more required reviewers rejected.
    Rejected { reasons: Vec<String> },
    /// Mixed results — some approved, some rejected (no required rejects).
    Partial {
        approved: Vec<String>,
        rejected: Vec<String>,
    },
}

/// Aggregated result from the reviewer council.
#[derive(Debug, Clone)]
pub struct CouncilResult {
    pub status: ReviewStatus,
    pub results: Vec<ReviewResult>,
}

/// Multi-reviewer council that dispatches reviews in parallel and determines consensus.
pub struct ReviewCouncil;

impl ReviewCouncil {
    /// Convene the council: run all reviewers in parallel and aggregate results.
    pub fn convene(
        pr_number: u64,
        branch: &str,
        reviewers: &[ReviewerConfig],
    ) -> Result<CouncilResult> {
        if reviewers.is_empty() {
            return Ok(CouncilResult {
                status: ReviewStatus::Approved {
                    approvals: Vec::new(),
                },
                results: Vec::new(),
            });
        }

        // Spawn each reviewer in a thread for parallel execution
        let handles: Vec<_> = reviewers
            .iter()
            .map(|reviewer| {
                let cmd = reviewer.command.clone();
                let name = reviewer.name.clone();
                let required = reviewer.required;
                let pr = pr_number;
                let br = branch.to_string();

                thread::spawn(move || {
                    let result = ReviewDispatcher::run_review_command(&cmd, pr, &br, Some(&name));
                    (name, required, result)
                })
            })
            .collect();

        let mut results = Vec::new();
        let mut approvals = Vec::new();
        let mut rejections = Vec::new();
        let mut required_rejected = false;

        for handle in handles {
            let (name, required, result) = handle
                .join()
                .map_err(|_| anyhow::anyhow!("Reviewer thread panicked"))?;

            match result {
                Ok(review_result) => {
                    if review_result.success {
                        approvals.push(name.clone());
                    } else {
                        rejections.push(name.clone());
                        if required {
                            required_rejected = true;
                        }
                    }
                    results.push(review_result);
                }
                Err(e) => {
                    rejections.push(name.clone());
                    if required {
                        required_rejected = true;
                    }
                    results.push(ReviewResult {
                        success: false,
                        output: format!("Error: {}", e),
                        reviewer_name: Some(name),
                    });
                }
            }
        }

        let status = if required_rejected {
            ReviewStatus::Rejected {
                reasons: rejections,
            }
        } else if rejections.is_empty() {
            ReviewStatus::Approved { approvals }
        } else {
            ReviewStatus::Partial {
                approved: approvals,
                rejected: rejections,
            }
        };

        Ok(CouncilResult { status, results })
    }

    /// Format the council results as a PR comment body.
    pub fn format_comment(result: &CouncilResult) -> String {
        let status_label = match &result.status {
            ReviewStatus::Approved { .. } => "APPROVED",
            ReviewStatus::Rejected { .. } => "REJECTED",
            ReviewStatus::Partial { .. } => "PARTIAL",
        };

        let mut body = format!("**Maestro Review Council** — {}\n\n", status_label);

        for review in &result.results {
            let name = review.reviewer_name.as_deref().unwrap_or("unknown");
            let icon = if review.success { "pass" } else { "fail" };
            body.push_str(&format!(
                "### {} — {}\n```\n{}\n```\n\n",
                name,
                icon,
                review.output.chars().take(500).collect::<String>()
            ));
        }

        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convene_empty_reviewers_returns_approved() {
        let result = ReviewCouncil::convene(1, "main", &[]).unwrap();
        assert_eq!(
            result.status,
            ReviewStatus::Approved {
                approvals: Vec::new()
            }
        );
        assert!(result.results.is_empty());
    }

    #[test]
    fn convene_single_passing_reviewer() {
        let reviewers = vec![ReviewerConfig {
            name: "test".into(),
            command: "true".into(),
            required: true,
        }];
        let result = ReviewCouncil::convene(1, "main", &reviewers).unwrap();
        assert!(matches!(result.status, ReviewStatus::Approved { .. }));
    }

    #[test]
    fn convene_single_failing_required_reviewer() {
        let reviewers = vec![ReviewerConfig {
            name: "strict".into(),
            command: "false".into(),
            required: true,
        }];
        let result = ReviewCouncil::convene(1, "main", &reviewers).unwrap();
        assert!(matches!(result.status, ReviewStatus::Rejected { .. }));
    }

    #[test]
    fn convene_failing_optional_reviewer_partial() {
        let reviewers = vec![
            ReviewerConfig {
                name: "pass".into(),
                command: "true".into(),
                required: true,
            },
            ReviewerConfig {
                name: "fail".into(),
                command: "false".into(),
                required: false,
            },
        ];
        let result = ReviewCouncil::convene(1, "main", &reviewers).unwrap();
        assert!(matches!(result.status, ReviewStatus::Partial { .. }));
    }

    #[test]
    fn convene_all_passing_reviewers() {
        let reviewers = vec![
            ReviewerConfig {
                name: "a".into(),
                command: "true".into(),
                required: true,
            },
            ReviewerConfig {
                name: "b".into(),
                command: "true".into(),
                required: false,
            },
        ];
        let result = ReviewCouncil::convene(1, "main", &reviewers).unwrap();
        assert!(matches!(result.status, ReviewStatus::Approved { .. }));
    }

    #[test]
    fn format_comment_contains_status() {
        let result = CouncilResult {
            status: ReviewStatus::Approved {
                approvals: vec!["test".into()],
            },
            results: vec![ReviewResult {
                success: true,
                output: "all good".into(),
                reviewer_name: Some("test".into()),
            }],
        };
        let comment = ReviewCouncil::format_comment(&result);
        assert!(comment.contains("APPROVED"));
        assert!(comment.contains("test"));
        assert!(comment.contains("all good"));
    }
}
