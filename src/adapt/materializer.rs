use std::collections::{HashMap, HashSet};

use anyhow::Context;
use async_trait::async_trait;

use super::types::*;
use crate::github::client::GitHubClient;

const DEFAULT_LABEL_COLOR: &str = "EDEDED";

const STANDARD_LABEL_COLORS: &[(&str, &str)] = &[
    ("enhancement", "A2EEEF"),
    ("bug", "D73A4A"),
    ("documentation", "0075CA"),
    ("testing", "BFD4F2"),
    ("tech-debt", "D4C5F9"),
    ("chore", "EDEDED"),
    ("type:feature", "1D76DB"),
    ("type:bug", "D93F0B"),
    ("type:docs", "0075CA"),
    ("type:chore", "EDEDED"),
    ("priority:P0", "B60205"),
    ("priority:P1", "D93F0B"),
    ("priority:P2", "FBCA04"),
    ("maestro:ready", "0E8A16"),
    ("maestro:in-progress", "F9D0C4"),
    ("maestro:done", "0E8A16"),
    ("maestro:failed", "D93F0B"),
];

fn color_for_label(name: &str) -> &str {
    STANDARD_LABEL_COLORS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
        .unwrap_or(DEFAULT_LABEL_COLOR)
}

#[async_trait]
pub trait PlanMaterializer: Send + Sync {
    async fn materialize(
        &self,
        plan: &AdaptPlan,
        report: &AdaptReport,
        dry_run: bool,
    ) -> anyhow::Result<MaterializeResult>;
}

pub struct GhMaterializer<G: GitHubClient> {
    github: G,
}

impl<G: GitHubClient> GhMaterializer<G> {
    pub fn new(github: G) -> Self {
        Self { github }
    }

    /// Ensure all labels referenced in the plan (and tech-debt catalog) exist on
    /// the target repo. Creates missing labels with standard or default colors.
    async fn ensure_labels(
        &self,
        plan: &AdaptPlan,
        report: &AdaptReport,
    ) -> anyhow::Result<()> {
        let mut needed: HashSet<String> = plan
            .milestones
            .iter()
            .flat_map(|m| &m.issues)
            .flat_map(|i| &i.labels)
            .cloned()
            .collect();

        if !report.tech_debt_items.is_empty() {
            needed.insert("tech-debt".to_string());
            needed.insert("enhancement".to_string());
        }

        if needed.is_empty() {
            return Ok(());
        }

        let existing = self.github.list_labels().await?;
        let existing_names: HashSet<&str> = existing.iter().map(|l| l.as_str()).collect();

        for label in &needed {
            if !existing_names.contains(label.as_str()) {
                let color = color_for_label(label);
                self.github
                    .create_label(label, color)
                    .await
                    .with_context(|| format!("Failed to create label '{}'", label))?;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<G: GitHubClient> PlanMaterializer for GhMaterializer<G> {
    async fn materialize(
        &self,
        plan: &AdaptPlan,
        report: &AdaptReport,
        dry_run: bool,
    ) -> anyhow::Result<MaterializeResult> {
        if dry_run {
            return Ok(build_dry_run_result(plan));
        }

        // Ensure all labels exist before creating issues (#348)
        self.ensure_labels(plan, report).await?;

        let mut milestones_created = Vec::new();
        let mut issues_created = Vec::new();
        let mut title_to_number: HashMap<String, u64> = HashMap::new();

        for milestone in &plan.milestones {
            let ms_number = self
                .github
                .create_milestone(&milestone.title, &milestone.description)
                .await?;

            milestones_created.push(CreatedMilestone {
                number: ms_number,
                title: milestone.title.clone(),
            });

            for issue in &milestone.issues {
                let body =
                    resolve_blocked_by(&issue.body, &issue.blocked_by_titles, &title_to_number);

                let issue_number = self
                    .github
                    .create_issue(&issue.title, &body, &issue.labels, Some(ms_number))
                    .await?;

                title_to_number.insert(issue.title.clone(), issue_number);
                issues_created.push(CreatedIssue {
                    number: issue_number,
                    title: issue.title.clone(),
                    milestone_number: Some(ms_number),
                });
            }
        }

        // Generate tech debt catalog issue (#95)
        let tech_debt_issue = if !report.tech_debt_items.is_empty() {
            let body = build_tech_debt_catalog_body(&report.tech_debt_items);
            let first_ms = milestones_created.first().map(|m| m.number);
            let number = self
                .github
                .create_issue(
                    "chore: Tech debt catalog",
                    &body,
                    &["tech-debt".to_string(), "enhancement".to_string()],
                    first_ms,
                )
                .await?;
            Some(CreatedIssue {
                number,
                title: "chore: Tech debt catalog".into(),
                milestone_number: first_ms,
            })
        } else {
            None
        };

        Ok(MaterializeResult {
            milestones_created,
            issues_created,
            tech_debt_issue,
            dry_run: false,
        })
    }
}

fn build_dry_run_result(plan: &AdaptPlan) -> MaterializeResult {
    let mut milestones = Vec::new();
    let mut issues = Vec::new();
    let mut counter = 0u64;

    for milestone in &plan.milestones {
        counter += 1;
        let ms_num = counter;
        milestones.push(CreatedMilestone {
            number: ms_num,
            title: milestone.title.clone(),
        });
        for issue in &milestone.issues {
            counter += 1;
            issues.push(CreatedIssue {
                number: counter,
                title: issue.title.clone(),
                milestone_number: Some(ms_num),
            });
        }
    }

    MaterializeResult {
        milestones_created: milestones,
        issues_created: issues,
        tech_debt_issue: None,
        dry_run: true,
    }
}

fn resolve_blocked_by(
    body: &str,
    blocked_by_titles: &[String],
    title_to_number: &HashMap<String, u64>,
) -> String {
    if blocked_by_titles.is_empty() {
        return body.to_string();
    }

    let references: Vec<String> = blocked_by_titles
        .iter()
        .map(|title| {
            if let Some(num) = title_to_number.get(title) {
                format!("- #{} {}", num, title)
            } else {
                format!("- {}", title)
            }
        })
        .collect();

    let blocked_section = format!("\n\n## Blocked By\n\n{}", references.join("\n"));

    // If body already has a Blocked By section, replace it
    if let Some(idx) = body.find("## Blocked By") {
        let before = &body[..idx];
        // Find next section or end
        let after = &body[idx..];
        let end = after[14..] // skip "## Blocked By\n"
            .find("## ")
            .map(|i| idx + 14 + i)
            .unwrap_or(body.len());
        format!("{}{}{}", before.trim_end(), blocked_section, &body[end..])
    } else {
        format!("{}{}", body, blocked_section)
    }
}

pub fn build_tech_debt_catalog_body(items: &[TechDebtItem]) -> String {
    let mut sections: Vec<(TechDebtSeverity, Vec<&TechDebtItem>)> = vec![
        (TechDebtSeverity::Critical, vec![]),
        (TechDebtSeverity::High, vec![]),
        (TechDebtSeverity::Medium, vec![]),
        (TechDebtSeverity::Low, vec![]),
    ];

    for item in items {
        for (sev, items_vec) in &mut sections {
            if *sev == item.severity {
                items_vec.push(item);
                break;
            }
        }
    }

    let mut body = String::from(
        "## Tech Debt Catalog\n\nAutomated catalog generated by `maestro adapt`. Items are ordered by severity.\n",
    );

    for (severity, items_vec) in &sections {
        if items_vec.is_empty() {
            continue;
        }

        let label = match severity {
            TechDebtSeverity::Critical => "Critical",
            TechDebtSeverity::High => "High",
            TechDebtSeverity::Medium => "Medium",
            TechDebtSeverity::Low => "Low",
        };

        body.push_str(&format!("\n### {}\n", label));
        for item in items_vec {
            let cat = format!("{:?}", item.category);
            body.push_str(&format!(
                "- [ ] **[{}]** `{}` — {} **Fix:** {}\n",
                cat, item.location, item.description, item.suggested_fix
            ));
        }
    }

    body.push_str(&format!(
        "\n---\n*Generated by `maestro adapt` on {}*\n",
        chrono::Utc::now().format("%Y-%m-%d")
    ));

    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::client::mock::MockGitHubClient;

    fn sample_plan() -> AdaptPlan {
        AdaptPlan {
            milestones: vec![PlannedMilestone {
                title: "M0: Foundation".into(),
                description: "Setup".into(),
                issues: vec![
                    PlannedIssue {
                        title: "feat: setup".into(),
                        body: "Setup the project".into(),
                        labels: vec!["enhancement".into()],
                        blocked_by_titles: vec![],
                    },
                    PlannedIssue {
                        title: "test: add tests".into(),
                        body: "Add tests".into(),
                        labels: vec!["testing".into()],
                        blocked_by_titles: vec!["feat: setup".into()],
                    },
                ],
            }],
            maestro_toml_patch: None,
        }
    }

    fn sample_report() -> AdaptReport {
        AdaptReport {
            summary: "test".into(),
            modules: vec![],
            tech_debt_items: vec![],
        }
    }

    fn sample_report_with_debt() -> AdaptReport {
        AdaptReport {
            summary: "test".into(),
            modules: vec![],
            tech_debt_items: vec![
                TechDebtItem {
                    title: "Missing tests".into(),
                    description: "No tests for auth".into(),
                    location: "src/auth.rs".into(),
                    suggested_fix: "Add unit tests".into(),
                    category: TechDebtCategory::MissingTests,
                    severity: TechDebtSeverity::High,
                },
                TechDebtItem {
                    title: "Dead code".into(),
                    description: "Unused handler".into(),
                    location: "src/legacy.rs".into(),
                    suggested_fix: "Delete module".into(),
                    category: TechDebtCategory::DeadCode,
                    severity: TechDebtSeverity::Low,
                },
                TechDebtItem {
                    title: "Hardcoded secret".into(),
                    description: "API key in source".into(),
                    location: "src/config.rs:42".into(),
                    suggested_fix: "Use env var".into(),
                    category: TechDebtCategory::SecurityConcern,
                    severity: TechDebtSeverity::Critical,
                },
            ],
        }
    }

    #[tokio::test]
    async fn materialize_creates_milestones_before_issues() {
        let client = MockGitHubClient::new();
        let materializer = GhMaterializer::new(client.clone());
        let plan = sample_plan();
        let report = sample_report();

        let result = materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        assert_eq!(result.milestones_created.len(), 1);
        assert_eq!(result.issues_created.len(), 2);

        // Milestone was created with number 1
        assert_eq!(result.milestones_created[0].number, 1);
        // Issues were assigned to milestone 1
        assert_eq!(result.issues_created[0].milestone_number, Some(1));
        assert_eq!(result.issues_created[1].milestone_number, Some(1));
    }

    #[tokio::test]
    async fn materialize_resolves_blocked_by_titles() {
        let client = MockGitHubClient::new();
        let materializer = GhMaterializer::new(client.clone());
        let plan = sample_plan();
        let report = sample_report();

        materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        let calls = client.create_issue_calls();
        assert_eq!(calls.len(), 2);
        // Second issue should have blocked_by reference to first issue (#1)
        assert!(
            calls[1].body.contains("#1"),
            "Second issue body should contain reference to first issue: {}",
            calls[1].body
        );
    }

    #[tokio::test]
    async fn materialize_dry_run_does_not_call_client() {
        let client = MockGitHubClient::new();
        let materializer = GhMaterializer::new(client.clone());
        let plan = sample_plan();
        let report = sample_report();

        let result = materializer
            .materialize(&plan, &report, true)
            .await
            .unwrap();

        assert!(result.dry_run);
        assert!(client.create_milestone_calls().is_empty());
        assert!(client.create_issue_calls().is_empty());
    }

    #[tokio::test]
    async fn materialize_creates_tech_debt_issue_when_items_exist() {
        let client = MockGitHubClient::new();
        let materializer = GhMaterializer::new(client.clone());
        let plan = sample_plan();
        let report = sample_report_with_debt();

        let result = materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        assert!(result.tech_debt_issue.is_some());
        let td = result.tech_debt_issue.unwrap();
        assert_eq!(td.title, "chore: Tech debt catalog");
    }

    #[tokio::test]
    async fn materialize_skips_tech_debt_issue_when_no_items() {
        let client = MockGitHubClient::new();
        let materializer = GhMaterializer::new(client.clone());
        let plan = sample_plan();
        let report = sample_report();

        let result = materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        assert!(result.tech_debt_issue.is_none());
    }

    #[test]
    fn tech_debt_catalog_body_groups_by_severity() {
        let report = sample_report_with_debt();
        let body = build_tech_debt_catalog_body(&report.tech_debt_items);

        assert!(body.contains("### Critical"));
        assert!(body.contains("### High"));
        assert!(body.contains("### Low"));
        // Medium should be omitted (no items)
        assert!(!body.contains("### Medium"));
    }

    #[test]
    fn tech_debt_catalog_body_critical_before_low() {
        let report = sample_report_with_debt();
        let body = build_tech_debt_catalog_body(&report.tech_debt_items);

        let critical_pos = body.find("### Critical").unwrap();
        let low_pos = body.find("### Low").unwrap();
        assert!(
            critical_pos < low_pos,
            "Critical must appear before Low in the catalog"
        );
    }

    #[test]
    fn tech_debt_catalog_body_has_checkboxes() {
        let report = sample_report_with_debt();
        let body = build_tech_debt_catalog_body(&report.tech_debt_items);
        assert!(body.contains("- [ ]"));
    }

    #[test]
    fn tech_debt_catalog_body_empty_items_produces_no_sections() {
        let body = build_tech_debt_catalog_body(&[]);
        assert!(!body.contains("### Critical"));
        assert!(!body.contains("### High"));
    }

    #[test]
    fn resolve_blocked_by_appends_section() {
        let body = "Some issue body";
        let titles = vec!["feat: setup".into()];
        let mut map = HashMap::new();
        map.insert("feat: setup".to_string(), 5u64);

        let result = resolve_blocked_by(body, &titles, &map);
        assert!(result.contains("## Blocked By"));
        assert!(result.contains("#5"));
    }

    #[test]
    fn resolve_blocked_by_empty_titles_returns_original() {
        let body = "Some body";
        let result = resolve_blocked_by(body, &[], &HashMap::new());
        assert_eq!(result, "Some body");
    }

    // ── ensure_labels tests (Issue #348) ─────────────────────────────────

    fn plan_with_labels(labels: Vec<String>) -> AdaptPlan {
        AdaptPlan {
            milestones: vec![PlannedMilestone {
                title: "M0: Foundation".into(),
                description: "Setup".into(),
                issues: vec![PlannedIssue {
                    title: "feat: setup".into(),
                    body: "Setup the project".into(),
                    labels,
                    blocked_by_titles: vec![],
                }],
            }],
            maestro_toml_patch: None,
        }
    }

    #[tokio::test]
    async fn ensure_labels_creates_missing_labels_with_correct_colors() {
        let client = MockGitHubClient::new();
        client.set_labels(vec![]);

        let materializer = GhMaterializer::new(client.clone());
        let plan = plan_with_labels(vec!["enhancement".into(), "bug".into()]);
        let report = sample_report();

        materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        let calls = client.create_label_calls();
        let call_map: std::collections::HashMap<String, String> =
            calls.into_iter().collect();

        assert!(
            call_map.contains_key("enhancement"),
            "expected create_label call for 'enhancement'"
        );
        assert!(
            call_map.contains_key("bug"),
            "expected create_label call for 'bug'"
        );
        assert_eq!(call_map["enhancement"], "A2EEEF");
        assert_eq!(call_map["bug"], "D73A4A");
    }

    #[tokio::test]
    async fn ensure_labels_skips_labels_that_already_exist() {
        let client = MockGitHubClient::new();
        client.set_labels(vec!["enhancement".into(), "bug".into()]);

        let materializer = GhMaterializer::new(client.clone());
        let plan = plan_with_labels(vec!["enhancement".into(), "bug".into()]);
        let report = sample_report();

        materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        assert!(
            client.create_label_calls().is_empty(),
            "no create_label calls expected when labels already exist"
        );
    }

    #[tokio::test]
    async fn ensure_labels_skips_when_no_labels_needed() {
        let client = MockGitHubClient::new();

        let materializer = GhMaterializer::new(client.clone());
        let plan = plan_with_labels(vec![]);
        let report = sample_report();

        materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        assert_eq!(
            client.list_labels_call_count(),
            0,
            "list_labels must not be called when no labels are required"
        );
    }

    #[tokio::test]
    async fn ensure_labels_includes_tech_debt_labels_when_report_has_debt() {
        let client = MockGitHubClient::new();
        client.set_labels(vec![]);

        let materializer = GhMaterializer::new(client.clone());
        let plan = plan_with_labels(vec!["type:feature".into()]);
        let report = sample_report_with_debt();

        materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        let calls = client.create_label_calls();
        let created_names: Vec<String> = calls.iter().map(|(n, _)| n.clone()).collect();

        assert!(
            created_names.contains(&"tech-debt".to_string()),
            "expected 'tech-debt' to be created; got: {:?}",
            created_names
        );
        assert!(
            created_names.contains(&"enhancement".to_string()),
            "expected 'enhancement' to be created; got: {:?}",
            created_names
        );
    }

    #[tokio::test]
    async fn materialize_succeeds_on_fresh_repo_with_empty_labels() {
        let client = MockGitHubClient::new();
        client.set_labels(vec![]);

        let materializer = GhMaterializer::new(client.clone());
        let plan = sample_plan();
        let report = sample_report();

        let result = materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        assert_eq!(result.issues_created.len(), 2, "all issues must be created");
        assert!(
            !client.create_label_calls().is_empty(),
            "labels should have been created on a fresh repo"
        );
    }

    #[tokio::test]
    async fn materialize_is_idempotent_when_labels_already_exist() {
        let client = MockGitHubClient::new();
        client.set_labels(vec!["enhancement".into(), "testing".into()]);

        let materializer = GhMaterializer::new(client.clone());
        let plan = sample_plan();
        let report = sample_report();

        let result = materializer
            .materialize(&plan, &report, false)
            .await
            .unwrap();

        assert_eq!(result.issues_created.len(), 2);
        assert!(
            client.create_label_calls().is_empty(),
            "no labels should be created when all already exist"
        );
    }

    #[test]
    fn color_for_label_returns_standard_color_for_known_labels() {
        for (label, expected_color) in STANDARD_LABEL_COLORS {
            let got = color_for_label(label);
            assert_eq!(
                got, *expected_color,
                "label '{}': expected '{}' got '{}'",
                label, expected_color, got
            );
        }
    }

    #[test]
    fn color_for_label_returns_default_color_for_unknown_label() {
        let got = color_for_label("some-custom-unknown-label");
        assert_eq!(got, DEFAULT_LABEL_COLOR);
    }

    #[tokio::test]
    async fn materialize_dry_run_skips_ensure_labels() {
        let client = MockGitHubClient::new();
        client.set_labels(vec![]);

        let materializer = GhMaterializer::new(client.clone());
        let plan = sample_plan();
        let report = sample_report();

        let result = materializer
            .materialize(&plan, &report, true)
            .await
            .unwrap();

        assert!(result.dry_run);
        assert_eq!(client.list_labels_call_count(), 0);
        assert!(client.create_label_calls().is_empty());
    }

    #[tokio::test]
    async fn ensure_labels_propagates_list_labels_error() {
        let client = MockGitHubClient::new();
        client.set_list_labels_error("gh: HTTP 403 Forbidden");

        let materializer = GhMaterializer::new(client.clone());
        let plan = plan_with_labels(vec!["enhancement".into()]);
        let report = sample_report();

        let result = materializer.materialize(&plan, &report, false).await;

        assert!(result.is_err(), "materialize must return Err when list_labels fails");
        assert!(
            client.create_issue_calls().is_empty(),
            "no issues should be created when ensure_labels fails"
        );
    }

    #[tokio::test]
    async fn ensure_labels_propagates_create_label_error() {
        let client = MockGitHubClient::new();
        client.set_labels(vec![]);
        client.set_create_label_error("gh: HTTP 422 Unprocessable Entity");

        let materializer = GhMaterializer::new(client.clone());
        let plan = plan_with_labels(vec!["enhancement".into()]);
        let report = sample_report();

        let result = materializer.materialize(&plan, &report, false).await;

        assert!(result.is_err(), "materialize must return Err when create_label fails");
        assert!(
            client.create_issue_calls().is_empty(),
            "no issues should be created when ensure_labels fails"
        );
    }
}
