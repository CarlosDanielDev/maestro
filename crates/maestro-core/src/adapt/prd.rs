use async_trait::async_trait;

use super::types::{AdaptReport, ProjectProfile};

#[async_trait]
pub trait PrdGenerator: Send + Sync {
    async fn generate(
        &self,
        profile: &ProjectProfile,
        report: &AdaptReport,
    ) -> anyhow::Result<String>;

    /// Enrich an existing PRD with the latest analysis instead of
    /// regenerating from scratch. Default implementation delegates to
    /// `generate`, which is a safe fallback for mock implementations.
    async fn enrich(
        &self,
        profile: &ProjectProfile,
        report: &AdaptReport,
        _existing: &str,
    ) -> anyhow::Result<String> {
        self.generate(profile, report).await
    }
}

pub struct ClaudePrdGenerator {
    model: String,
}

impl ClaudePrdGenerator {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

#[async_trait]
impl PrdGenerator for ClaudePrdGenerator {
    async fn generate(
        &self,
        profile: &ProjectProfile,
        report: &AdaptReport,
    ) -> anyhow::Result<String> {
        let profile_json = serde_json::to_string_pretty(profile)?;
        let report_json = serde_json::to_string_pretty(report)?;
        let prompt = super::prompts::build_prd_prompt(&profile_json, &report_json);
        super::prompts::run_claude_print(&self.model, &prompt, &profile.root).await
    }

    async fn enrich(
        &self,
        profile: &ProjectProfile,
        report: &AdaptReport,
        existing: &str,
    ) -> anyhow::Result<String> {
        let profile_json = serde_json::to_string_pretty(profile)?;
        let report_json = serde_json::to_string_pretty(report)?;
        let prompt = super::prompts::build_prd_enrich_prompt(&profile_json, &report_json, existing);
        super::prompts::run_claude_print(&self.model, &prompt, &profile.root).await
    }
}

#[cfg(test)]
pub struct MockPrdGenerator {
    result: Option<String>,
}

#[cfg(test)]
impl MockPrdGenerator {
    pub fn with_content(content: String) -> Self {
        Self {
            result: Some(content),
        }
    }

    pub fn without_content() -> Self {
        Self { result: None }
    }
}

#[cfg(test)]
#[async_trait]
impl PrdGenerator for MockPrdGenerator {
    async fn generate(
        &self,
        _profile: &ProjectProfile,
        _report: &AdaptReport,
    ) -> anyhow::Result<String> {
        self.result
            .clone()
            .ok_or_else(|| anyhow::anyhow!("mock prd generator: no content configured"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::types::*;
    use std::path::PathBuf;

    fn sample_profile() -> ProjectProfile {
        ProjectProfile {
            name: "test-project".into(),
            root: PathBuf::from("/tmp"),
            language: ProjectLanguage::Rust,
            manifests: vec![],
            config_files: vec![],
            entry_points: vec![],
            source_stats: SourceStats {
                total_files: 10,
                total_lines: 500,
                by_extension: vec![],
            },
            test_infra: TestInfraInfo {
                has_tests: true,
                framework: Some("cargo test".into()),
                test_directories: vec![],
                test_file_count: 3,
            },
            ci: CiInfo {
                provider: None,
                config_files: vec![],
            },
            git: GitInfo {
                is_git_repo: true,
                default_branch: Some("main".into()),
                remote_url: None,
                commit_count: 42,
                recent_contributors: vec![],
            },
            dependencies: DependencySummary::default(),
            directory_tree: String::new(),
            has_maestro_config: false,
            has_workflow_docs: false,
        }
    }

    fn sample_report() -> AdaptReport {
        AdaptReport {
            summary: "A Rust CLI tool".into(),
            modules: vec![ModuleDescription {
                path: "src/main.rs".into(),
                purpose: "Entry point".into(),
                complexity: "low".into(),
            }],
            tech_debt_items: vec![TechDebtItem {
                title: "Missing tests".into(),
                description: "No tests for auth module".into(),
                location: "src/auth.rs".into(),
                suggested_fix: "Add unit tests".into(),
                category: TechDebtCategory::MissingTests,
                severity: TechDebtSeverity::High,
            }],
        }
    }

    #[tokio::test]
    async fn mock_prd_generator_returns_configured_content() {
        let generator = MockPrdGenerator::with_content("# PRD: Test".into());
        let result = generator
            .generate(&sample_profile(), &sample_report())
            .await
            .unwrap();
        assert_eq!(result, "# PRD: Test");
    }

    #[tokio::test]
    async fn mock_prd_generator_without_content_returns_error() {
        let generator = MockPrdGenerator::without_content();
        let result = generator
            .generate(&sample_profile(), &sample_report())
            .await;
        assert!(result.is_err());
    }
}
