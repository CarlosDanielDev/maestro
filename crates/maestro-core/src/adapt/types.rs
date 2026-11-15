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
#[path = "types_tests.rs"]
mod tests;
