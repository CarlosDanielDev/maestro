use async_trait::async_trait;

use super::prompts::{build_analysis_prompt, parse_json_response, run_claude_print};
use super::types::{AdaptReport, ProjectProfile};

#[async_trait]
pub trait ProjectAnalyzer: Send + Sync {
    async fn analyze(&self, profile: &ProjectProfile) -> anyhow::Result<AdaptReport>;
}

pub struct ClaudeAnalyzer {
    model: String,
}

impl ClaudeAnalyzer {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

#[async_trait]
impl ProjectAnalyzer for ClaudeAnalyzer {
    async fn analyze(&self, profile: &ProjectProfile) -> anyhow::Result<AdaptReport> {
        let profile_json = serde_json::to_string_pretty(profile)?;
        let prompt = build_analysis_prompt(&profile_json);
        let raw = run_claude_print(&self.model, &prompt, &profile.root).await?;
        parse_json_response(&raw)
    }
}

#[cfg(test)]
pub struct MockProjectAnalyzer {
    result: Option<AdaptReport>,
}

#[cfg(test)]
impl MockProjectAnalyzer {
    pub fn with_report(report: AdaptReport) -> Self {
        Self {
            result: Some(report),
        }
    }

    pub fn without_report() -> Self {
        Self { result: None }
    }
}

#[cfg(test)]
#[async_trait]
impl ProjectAnalyzer for MockProjectAnalyzer {
    async fn analyze(&self, _profile: &ProjectProfile) -> anyhow::Result<AdaptReport> {
        self.result
            .clone()
            .ok_or_else(|| anyhow::anyhow!("mock analyzer: no report configured"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::types::*;
    use std::path::PathBuf;

    fn sample_profile() -> ProjectProfile {
        ProjectProfile {
            name: "test".into(),
            root: PathBuf::from("/tmp/test"),
            language: ProjectLanguage::Rust,
            manifests: vec![PathBuf::from("Cargo.toml")],
            config_files: vec![],
            entry_points: vec![PathBuf::from("src/main.rs")],
            source_stats: SourceStats {
                total_files: 5,
                total_lines: 200,
                by_extension: vec![],
            },
            test_infra: TestInfraInfo {
                has_tests: true,
                framework: Some("cargo test".into()),
                test_directories: vec![],
                test_file_count: 2,
            },
            ci: CiInfo {
                provider: None,
                config_files: vec![],
            },
            git: GitInfo {
                is_git_repo: true,
                default_branch: Some("main".into()),
                remote_url: None,
                commit_count: 10,
                recent_contributors: vec![],
            },
            dependencies: DependencySummary {
                direct_count: 3,
                dev_count: 1,
                notable: vec![],
            },
            directory_tree: "src/\n  main.rs".into(),
            has_maestro_config: false,
            has_workflow_docs: false,
        }
    }

    #[test]
    fn analysis_prompt_contains_profile_data() {
        let profile = sample_profile();
        let json = serde_json::to_string_pretty(&profile).unwrap();
        let prompt = build_analysis_prompt(&json);
        assert!(prompt.contains("test"));
        assert!(prompt.contains("Cargo.toml"));
        assert!(prompt.contains("tech_debt_items"));
    }

    #[tokio::test]
    async fn mock_analyzer_returns_configured_report() {
        let report = AdaptReport {
            summary: "Test project".into(),
            modules: vec![],
            tech_debt_items: vec![],
        };
        let analyzer = MockProjectAnalyzer::with_report(report);
        let result = analyzer.analyze(&sample_profile()).await.unwrap();
        assert_eq!(result.summary, "Test project");
    }

    #[tokio::test]
    async fn mock_analyzer_returns_report_with_modules_and_debt() {
        let report = AdaptReport {
            summary: "Complex project".into(),
            modules: vec![
                ModuleDescription {
                    path: "src/auth.rs".into(),
                    purpose: "Authentication".into(),
                    complexity: "high".into(),
                },
                ModuleDescription {
                    path: "src/db.rs".into(),
                    purpose: "Database layer".into(),
                    complexity: "medium".into(),
                },
            ],
            tech_debt_items: vec![TechDebtItem {
                title: "Missing auth tests".into(),
                description: "No tests for auth module".into(),
                location: "src/auth.rs".into(),
                suggested_fix: "Add unit tests".into(),
                category: TechDebtCategory::MissingTests,
                severity: TechDebtSeverity::High,
            }],
        };
        let analyzer = MockProjectAnalyzer::with_report(report);
        let result = analyzer.analyze(&sample_profile()).await.unwrap();
        assert_eq!(result.modules.len(), 2);
        assert_eq!(result.tech_debt_items.len(), 1);
        assert_eq!(result.tech_debt_items[0].severity, TechDebtSeverity::High);
    }

    #[tokio::test]
    async fn mock_analyzer_without_report_returns_error() {
        let analyzer = MockProjectAnalyzer::without_report();
        assert!(analyzer.analyze(&sample_profile()).await.is_err());
    }
}
