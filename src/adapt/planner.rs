use async_trait::async_trait;

use super::prompts::{build_planning_prompt, parse_json_response, run_claude_print};
use super::types::{AdaptPlan, AdaptReport, ProjectProfile};

#[async_trait]
pub trait AdaptPlanner: Send + Sync {
    async fn plan(
        &self,
        profile: &ProjectProfile,
        report: &AdaptReport,
    ) -> anyhow::Result<AdaptPlan>;
}

pub struct ClaudePlanner {
    model: String,
}

impl ClaudePlanner {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

#[async_trait]
impl AdaptPlanner for ClaudePlanner {
    async fn plan(
        &self,
        profile: &ProjectProfile,
        report: &AdaptReport,
    ) -> anyhow::Result<AdaptPlan> {
        let profile_json = serde_json::to_string_pretty(profile)?;
        let report_json = serde_json::to_string_pretty(report)?;
        let prompt = build_planning_prompt(&profile_json, &report_json);
        let raw = run_claude_print(&self.model, &prompt, &profile.root).await?;
        parse_json_response(&raw)
    }
}

#[cfg(test)]
pub struct MockAdaptPlanner {
    result: Option<AdaptPlan>,
}

#[cfg(test)]
impl MockAdaptPlanner {
    pub fn with_plan(plan: AdaptPlan) -> Self {
        Self { result: Some(plan) }
    }
}

#[cfg(test)]
#[async_trait]
impl AdaptPlanner for MockAdaptPlanner {
    async fn plan(
        &self,
        _profile: &ProjectProfile,
        _report: &AdaptReport,
    ) -> anyhow::Result<AdaptPlan> {
        self.result
            .clone()
            .ok_or_else(|| anyhow::anyhow!("mock planner: no plan configured"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::types::*;
    use std::path::PathBuf;

    fn sample_plan() -> AdaptPlan {
        AdaptPlan {
            milestones: vec![PlannedMilestone {
                title: "M0: Foundation".into(),
                description: "Initial setup".into(),
                issues: vec![
                    PlannedIssue {
                        title: "feat: setup project".into(),
                        body: "## Overview\n\nSetup the project.".into(),
                        labels: vec!["enhancement".into()],
                        blocked_by_titles: vec![],
                    },
                    PlannedIssue {
                        title: "test: add unit tests".into(),
                        body: "## Overview\n\nAdd tests.".into(),
                        labels: vec!["testing".into()],
                        blocked_by_titles: vec!["feat: setup project".into()],
                    },
                ],
            }],
            maestro_toml_patch: Some("[project]\nrepo = \"owner/repo\"".into()),
        }
    }

    #[tokio::test]
    async fn mock_planner_returns_configured_plan() {
        let plan = sample_plan();
        let planner = MockAdaptPlanner::with_plan(plan.clone());

        let profile = ProjectProfile {
            name: "test".into(),
            root: PathBuf::from("/tmp"),
            language: ProjectLanguage::Rust,
            manifests: vec![],
            config_files: vec![],
            entry_points: vec![],
            source_stats: SourceStats {
                total_files: 0,
                total_lines: 0,
                by_extension: vec![],
            },
            test_infra: TestInfraInfo {
                has_tests: false,
                framework: None,
                test_directories: vec![],
                test_file_count: 0,
            },
            ci: CiInfo {
                provider: None,
                config_files: vec![],
            },
            git: GitInfo {
                is_git_repo: false,
                default_branch: None,
                remote_url: None,
                commit_count: 0,
                recent_contributors: vec![],
            },
            dependencies: DependencySummary {
                direct_count: 0,
                dev_count: 0,
                notable: vec![],
            },
            directory_tree: String::new(),
            has_maestro_config: false,
        };

        let report = AdaptReport {
            summary: "test".into(),
            modules: vec![],
            tech_debt_items: vec![],
        };

        let result = planner.plan(&profile, &report).await.unwrap();
        assert_eq!(result.milestones.len(), 1);
        assert_eq!(result.milestones[0].issues.len(), 2);
        assert_eq!(
            result.milestones[0].issues[1].blocked_by_titles,
            vec!["feat: setup project"]
        );
    }

    #[test]
    fn plan_json_round_trip_with_dependencies() {
        let plan = sample_plan();
        let json = serde_json::to_string(&plan).unwrap();
        let rt: AdaptPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.milestones[0].issues[1].blocked_by_titles.len(), 1);
    }
}
