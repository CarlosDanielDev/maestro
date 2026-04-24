use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProfile {
    pub name: String,
    pub root: PathBuf,
    pub language: ProjectLanguage,
    pub manifests: Vec<PathBuf>,
    pub config_files: Vec<PathBuf>,
    pub entry_points: Vec<PathBuf>,
    pub source_stats: SourceStats,
    pub test_infra: TestInfraInfo,
    pub ci: CiInfo,
    pub git: GitInfo,
    pub dependencies: DependencySummary,
    pub directory_tree: String,
    pub has_maestro_config: bool,
    /// Whether existing workflow documentation (WORKFLOW.md, CONTRIBUTING.md) was detected.
    #[serde(default)]
    pub has_workflow_docs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLanguage {
    Rust,
    TypeScript,
    Python,
    Go,
    Java,
    Ruby,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceStats {
    pub total_files: u32,
    pub total_lines: u64,
    pub by_extension: Vec<ExtensionStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionStats {
    pub extension: String,
    pub files: u32,
    pub lines: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestInfraInfo {
    pub has_tests: bool,
    pub framework: Option<String>,
    pub test_directories: Vec<PathBuf>,
    pub test_file_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiInfo {
    pub provider: Option<String>,
    pub config_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub is_git_repo: bool,
    pub default_branch: Option<String>,
    pub remote_url: Option<String>,
    pub commit_count: u64,
    pub recent_contributors: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencySummary {
    pub direct_count: u32,
    pub dev_count: u32,
    pub notable: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptReport {
    pub summary: String,
    pub modules: Vec<ModuleDescription>,
    pub tech_debt_items: Vec<TechDebtItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDescription {
    pub path: String,
    pub purpose: String,
    pub complexity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechDebtItem {
    pub title: String,
    pub description: String,
    pub location: String,
    pub suggested_fix: String,
    pub category: TechDebtCategory,
    pub severity: TechDebtSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TechDebtCategory {
    DeadCode,
    MissingTests,
    InconsistentPatterns,
    PoorAbstractions,
    SecurityConcern,
    Documentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TechDebtSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptPlan {
    pub milestones: Vec<PlannedMilestone>,
    pub maestro_toml_patch: Option<String>,
    /// Generated workflow guide content (markdown). `None` if existing docs were detected.
    #[serde(default)]
    pub workflow_guide: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedMilestone {
    pub title: String,
    pub description: String,
    pub issues: Vec<PlannedIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedIssue {
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub blocked_by_titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializeResult {
    pub milestones_created: Vec<CreatedMilestone>,
    pub issues_created: Vec<CreatedIssue>,
    #[serde(default)]
    pub issues_skipped: Vec<SkippedIssue>,
    pub tech_debt_issue: Option<CreatedIssue>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedMilestone {
    pub number: u64,
    pub title: String,
    /// `true` when `create_milestone` matched a pre-existing milestone
    /// instead of POSTing a new one.
    #[serde(default)]
    pub reused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedIssue {
    pub number: u64,
    pub title: String,
    pub milestone_number: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    DuplicateTitle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedIssue {
    /// Number of the existing issue that matched.
    pub number: u64,
    pub title: String,
    pub reason: SkipReason,
}

/// Status of a single scaffolded file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaffoldFileStatus {
    Created,
    Skipped,
    Failed,
}

/// A single file generated (or skipped) by the scaffold phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaffoldedFile {
    pub path: String,
    pub status: ScaffoldFileStatus,
    pub reason: Option<String>,
}

/// Result of the scaffold phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaffoldResult {
    pub files: Vec<ScaffoldedFile>,
    pub created_count: usize,
    pub skipped_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- ProjectLanguage serialization --

    #[test]
    fn project_language_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&ProjectLanguage::Rust).unwrap(),
            r#""rust""#
        );
        assert_eq!(
            serde_json::to_string(&ProjectLanguage::TypeScript).unwrap(),
            r#""type_script""#
        );
        assert_eq!(
            serde_json::to_string(&ProjectLanguage::Unknown).unwrap(),
            r#""unknown""#
        );
    }

    #[test]
    fn project_language_deserializes_from_snake_case() {
        let lang: ProjectLanguage = serde_json::from_str(r#""python""#).unwrap();
        assert_eq!(lang, ProjectLanguage::Python);
    }

    // -- TechDebtCategory serialization --

    #[test]
    fn tech_debt_category_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&TechDebtCategory::DeadCode).unwrap(),
            r#""dead_code""#
        );
        assert_eq!(
            serde_json::to_string(&TechDebtCategory::MissingTests).unwrap(),
            r#""missing_tests""#
        );
        assert_eq!(
            serde_json::to_string(&TechDebtCategory::SecurityConcern).unwrap(),
            r#""security_concern""#
        );
    }

    // -- TechDebtSeverity serialization and ordering --

    #[test]
    fn tech_debt_severity_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&TechDebtSeverity::Critical).unwrap(),
            r#""critical""#
        );
        assert_eq!(
            serde_json::to_string(&TechDebtSeverity::Low).unwrap(),
            r#""low""#
        );
    }

    #[test]
    fn tech_debt_severity_ordering() {
        assert!(TechDebtSeverity::Critical > TechDebtSeverity::High);
        assert!(TechDebtSeverity::High > TechDebtSeverity::Medium);
        assert!(TechDebtSeverity::Medium > TechDebtSeverity::Low);
    }

    // -- ProjectProfile round-trip --

    #[test]
    fn project_profile_round_trips_through_json() {
        let profile = ProjectProfile {
            name: "test-project".into(),
            root: PathBuf::from("/tmp/test"),
            language: ProjectLanguage::Rust,
            manifests: vec![PathBuf::from("Cargo.toml")],
            config_files: vec![],
            entry_points: vec![PathBuf::from("src/main.rs")],
            source_stats: SourceStats {
                total_files: 10,
                total_lines: 500,
                by_extension: vec![ExtensionStats {
                    extension: "rs".into(),
                    files: 10,
                    lines: 500,
                }],
            },
            test_infra: TestInfraInfo {
                has_tests: true,
                framework: Some("cargo test".into()),
                test_directories: vec![],
                test_file_count: 3,
            },
            ci: CiInfo {
                provider: Some("github_actions".into()),
                config_files: vec![PathBuf::from(".github/workflows/ci.yml")],
            },
            git: GitInfo {
                is_git_repo: true,
                default_branch: Some("main".into()),
                remote_url: Some("https://github.com/owner/repo".into()),
                commit_count: 42,
                recent_contributors: vec!["alice".into()],
            },
            dependencies: DependencySummary {
                direct_count: 5,
                dev_count: 2,
                notable: vec!["tokio".into(), "serde".into()],
            },
            directory_tree: "src/\n  main.rs".into(),
            has_maestro_config: false,
            has_workflow_docs: false,
        };

        let json = serde_json::to_string(&profile).unwrap();
        let rt: ProjectProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.name, "test-project");
        assert_eq!(rt.language, ProjectLanguage::Rust);
        assert_eq!(rt.source_stats.total_files, 10);
    }

    // -- AdaptReport round-trip --

    #[test]
    fn adapt_report_round_trips_through_json() {
        let report = AdaptReport {
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
        };

        let json = serde_json::to_string(&report).unwrap();
        let rt: AdaptReport = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.summary, "A Rust CLI tool");
        assert_eq!(rt.modules.len(), 1);
        assert_eq!(rt.tech_debt_items.len(), 1);
        assert_eq!(rt.tech_debt_items[0].severity, TechDebtSeverity::High);
    }

    // -- AdaptPlan round-trip --

    #[test]
    fn adapt_plan_round_trips_through_json() {
        let plan = AdaptPlan {
            milestones: vec![PlannedMilestone {
                title: "M0: Foundation".into(),
                description: "Initial setup".into(),
                issues: vec![PlannedIssue {
                    title: "feat: setup project".into(),
                    body: "## Overview\nSetup".into(),
                    labels: vec!["enhancement".into()],
                    blocked_by_titles: vec![],
                }],
            }],
            maestro_toml_patch: Some("[project]\nrepo = \"owner/repo\"".into()),
            workflow_guide: None,
        };

        let json = serde_json::to_string(&plan).unwrap();
        let rt: AdaptPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.milestones.len(), 1);
        assert_eq!(rt.milestones[0].issues.len(), 1);
        assert!(rt.maestro_toml_patch.is_some());
    }

    // -- MaterializeResult round-trip --

    #[test]
    fn materialize_result_round_trips_through_json() {
        let result = MaterializeResult {
            milestones_created: vec![CreatedMilestone {
                number: 1,
                title: "M0".into(),
                reused: false,
            }],
            issues_created: vec![CreatedIssue {
                number: 10,
                title: "feat: thing".into(),
                milestone_number: Some(1),
            }],
            issues_skipped: vec![],
            tech_debt_issue: None,
            dry_run: false,
        };

        let json = serde_json::to_string(&result).unwrap();
        let rt: MaterializeResult = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.milestones_created.len(), 1);
        assert_eq!(rt.issues_created.len(), 1);
        assert!(!rt.dry_run);
    }

    #[test]
    fn skipped_issue_round_trips_through_json() {
        let skipped = SkippedIssue {
            number: 42,
            title: "feat: already exists".into(),
            reason: SkipReason::DuplicateTitle,
        };
        let json = serde_json::to_string(&skipped).unwrap();
        let rt: SkippedIssue = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.number, 42);
        assert_eq!(rt.reason, SkipReason::DuplicateTitle);
    }
}
