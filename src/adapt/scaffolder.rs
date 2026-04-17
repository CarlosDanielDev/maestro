use async_trait::async_trait;
use std::path::Path;

use super::prompts;
use super::types::{
    AdaptPlan, AdaptReport, ProjectProfile, ScaffoldFileStatus, ScaffoldResult, ScaffoldedFile,
};

#[async_trait]
pub trait ProjectScaffolder: Send + Sync {
    async fn scaffold(
        &self,
        profile: &ProjectProfile,
        report: &AdaptReport,
        plan: &AdaptPlan,
    ) -> anyhow::Result<ScaffoldResult>;
}

pub struct ClaudeScaffolder {
    model: String,
}

impl ClaudeScaffolder {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

/// The JSON structure Claude returns for scaffold content.
#[derive(Debug, Clone, serde::Deserialize)]
struct ScaffoldManifest {
    files: Vec<ScaffoldFileEntry>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ScaffoldFileEntry {
    path: String,
    content: String,
}

#[async_trait]
impl ProjectScaffolder for ClaudeScaffolder {
    async fn scaffold(
        &self,
        profile: &ProjectProfile,
        report: &AdaptReport,
        plan: &AdaptPlan,
    ) -> anyhow::Result<ScaffoldResult> {
        let profile_json = serde_json::to_string_pretty(profile)?;
        let report_json = serde_json::to_string_pretty(report)?;
        let plan_json = serde_json::to_string_pretty(plan)?;
        let prompt = prompts::build_scaffold_prompt(&profile_json, &report_json, &plan_json);
        let raw = prompts::run_claude_print(&self.model, &prompt, &profile.root).await?;
        let manifest: ScaffoldManifest = prompts::parse_json_response(&raw)?;

        let claude_dir = profile.root.join(".claude");
        write_scaffold_files(&claude_dir, &manifest.files)
    }
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains("..")
        && !path.contains('\0')
}

fn write_scaffold_files(
    claude_dir: &Path,
    entries: &[ScaffoldFileEntry],
) -> anyhow::Result<ScaffoldResult> {
    let mut files = Vec::with_capacity(entries.len());
    let mut created_count = 0;
    let mut skipped_count = 0;

    for entry in entries {
        let rel_display = format!(".claude/{}", entry.path);

        if !is_safe_relative_path(&entry.path) {
            files.push(ScaffoldedFile {
                path: rel_display,
                status: ScaffoldFileStatus::Failed,
                reason: Some("path escapes .claude/ directory".into()),
            });
            continue;
        }

        let target = claude_dir.join(&entry.path);

        if target.exists() {
            files.push(ScaffoldedFile {
                path: rel_display,
                status: ScaffoldFileStatus::Skipped,
                reason: Some("file already exists".into()),
            });
            skipped_count += 1;
            continue;
        }

        if let Some(parent) = target.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            files.push(ScaffoldedFile {
                path: rel_display,
                status: ScaffoldFileStatus::Failed,
                reason: Some(format!("mkdir failed: {}", e)),
            });
            continue;
        }

        match std::fs::write(&target, &entry.content) {
            Ok(()) => {
                files.push(ScaffoldedFile {
                    path: rel_display,
                    status: ScaffoldFileStatus::Created,
                    reason: None,
                });
                created_count += 1;
            }
            Err(e) => {
                files.push(ScaffoldedFile {
                    path: rel_display,
                    status: ScaffoldFileStatus::Failed,
                    reason: Some(format!("write failed: {}", e)),
                });
            }
        }
    }

    Ok(ScaffoldResult {
        files,
        created_count,
        skipped_count,
    })
}

#[cfg(test)]
pub struct MockProjectScaffolder {
    result: Option<ScaffoldResult>,
}

#[cfg(test)]
impl MockProjectScaffolder {
    pub fn with_result(result: ScaffoldResult) -> Self {
        Self {
            result: Some(result),
        }
    }

    pub fn without_result() -> Self {
        Self { result: None }
    }
}

#[cfg(test)]
#[async_trait]
impl ProjectScaffolder for MockProjectScaffolder {
    async fn scaffold(
        &self,
        _profile: &ProjectProfile,
        _report: &AdaptReport,
        _plan: &AdaptPlan,
    ) -> anyhow::Result<ScaffoldResult> {
        self.result
            .clone()
            .ok_or_else(|| anyhow::anyhow!("mock scaffolder: no result configured"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::types::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn sample_profile_rust() -> ProjectProfile {
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
            summary: "A test project".into(),
            modules: vec![],
            tech_debt_items: vec![],
        }
    }

    fn sample_plan() -> AdaptPlan {
        AdaptPlan {
            milestones: vec![],
            maestro_toml_patch: None,
            workflow_guide: None,
        }
    }

    fn entry(path: &str, content: &str) -> ScaffoldFileEntry {
        ScaffoldFileEntry {
            path: path.into(),
            content: content.into(),
        }
    }

    // ── Type serialization ────────────────────────────────────────────

    #[test]
    fn scaffold_file_status_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&ScaffoldFileStatus::Created).unwrap(),
            r#""created""#
        );
        assert_eq!(
            serde_json::to_string(&ScaffoldFileStatus::Skipped).unwrap(),
            r#""skipped""#
        );
        assert_eq!(
            serde_json::to_string(&ScaffoldFileStatus::Failed).unwrap(),
            r#""failed""#
        );
    }

    #[test]
    fn scaffold_file_status_deserializes_from_snake_case() {
        let status: ScaffoldFileStatus = serde_json::from_str(r#""skipped""#).unwrap();
        assert_eq!(status, ScaffoldFileStatus::Skipped);
    }

    #[test]
    fn scaffolded_file_round_trips_through_json() {
        let file = ScaffoldedFile {
            path: "CLAUDE.md".into(),
            status: ScaffoldFileStatus::Created,
            reason: None,
        };
        let json = serde_json::to_string(&file).unwrap();
        let rt: ScaffoldedFile = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.path, "CLAUDE.md");
        assert_eq!(rt.status, ScaffoldFileStatus::Created);
        assert!(rt.reason.is_none());
    }

    #[test]
    fn scaffolded_file_with_reason_round_trips() {
        let file = ScaffoldedFile {
            path: "CLAUDE.md".into(),
            status: ScaffoldFileStatus::Skipped,
            reason: Some("file already exists".into()),
        };
        let json = serde_json::to_string(&file).unwrap();
        let rt: ScaffoldedFile = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.reason, Some("file already exists".into()));
    }

    #[test]
    fn scaffold_result_round_trips_through_json() {
        let result = ScaffoldResult {
            files: vec![
                ScaffoldedFile {
                    path: "CLAUDE.md".into(),
                    status: ScaffoldFileStatus::Created,
                    reason: None,
                },
                ScaffoldedFile {
                    path: "old.md".into(),
                    status: ScaffoldFileStatus::Skipped,
                    reason: Some("exists".into()),
                },
            ],
            created_count: 1,
            skipped_count: 1,
        };
        let json = serde_json::to_string(&result).unwrap();
        let rt: ScaffoldResult = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.files.len(), 2);
        assert_eq!(rt.created_count, 1);
        assert_eq!(rt.skipped_count, 1);
    }

    #[test]
    fn scaffold_result_empty_is_valid() {
        let result = ScaffoldResult {
            files: vec![],
            created_count: 0,
            skipped_count: 0,
        };
        let json = serde_json::to_string(&result).unwrap();
        let rt: ScaffoldResult = serde_json::from_str(&json).unwrap();
        assert!(rt.files.is_empty());
        assert_eq!(rt.created_count, 0);
    }

    // ── write_scaffold_files ──────────────────────────────────────────

    #[test]
    fn write_scaffold_files_creates_files_in_claude_dir() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        let entries = vec![entry("CLAUDE.md", "# Hello")];

        let result = write_scaffold_files(&claude_dir, &entries).unwrap();
        assert_eq!(result.created_count, 1);
        assert_eq!(result.skipped_count, 0);
        assert_eq!(result.files[0].status, ScaffoldFileStatus::Created);
        assert_eq!(
            std::fs::read_to_string(claude_dir.join("CLAUDE.md")).unwrap(),
            "# Hello"
        );
    }

    #[test]
    fn write_scaffold_files_creates_nested_subdirectories() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        let entries = vec![entry("skills/rust/SKILL.md", "rust skill")];

        write_scaffold_files(&claude_dir, &entries).unwrap();
        assert!(claude_dir.join("skills/rust/SKILL.md").exists());
    }

    #[test]
    fn write_scaffold_files_skips_existing_file() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(claude_dir.join("CLAUDE.md"), "original").unwrap();

        let entries = vec![entry("CLAUDE.md", "overwrite attempt")];
        let result = write_scaffold_files(&claude_dir, &entries).unwrap();

        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.created_count, 0);
        assert_eq!(result.files[0].status, ScaffoldFileStatus::Skipped);
        assert_eq!(
            std::fs::read_to_string(claude_dir.join("CLAUDE.md")).unwrap(),
            "original"
        );
    }

    #[test]
    fn write_scaffold_files_mixed_create_and_skip() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(claude_dir.join("CLAUDE.md"), "existing").unwrap();

        let entries = vec![entry("CLAUDE.md", "new"), entry("agents/qa.md", "qa agent")];
        let result = write_scaffold_files(&claude_dir, &entries).unwrap();

        assert_eq!(result.created_count, 1);
        assert_eq!(result.skipped_count, 1);
        assert_eq!(
            std::fs::read_to_string(claude_dir.join("agents/qa.md")).unwrap(),
            "qa agent"
        );
        assert_eq!(
            std::fs::read_to_string(claude_dir.join("CLAUDE.md")).unwrap(),
            "existing"
        );
    }

    #[test]
    fn write_scaffold_files_empty_entries_returns_zero_counts() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");

        let result = write_scaffold_files(&claude_dir, &[]).unwrap();
        assert_eq!(result.created_count, 0);
        assert_eq!(result.skipped_count, 0);
        assert!(result.files.is_empty());
    }

    #[test]
    fn write_scaffold_files_result_order_matches_input() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        let entries = vec![entry("a.md", "a"), entry("b.md", "b"), entry("c.md", "c")];

        let result = write_scaffold_files(&claude_dir, &entries).unwrap();
        assert_eq!(result.files[0].path, ".claude/a.md");
        assert_eq!(result.files[1].path, ".claude/b.md");
        assert_eq!(result.files[2].path, ".claude/c.md");
    }

    // ── Path traversal protection ───────────────────────────────────────

    #[test]
    fn write_scaffold_files_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        let entries = vec![entry("../../etc/evil", "pwned")];

        let result = write_scaffold_files(&claude_dir, &entries).unwrap();
        assert_eq!(result.files[0].status, ScaffoldFileStatus::Failed);
        assert!(result.files[0].reason.as_ref().unwrap().contains("escapes"));
        assert!(!dir.path().join("etc/evil").exists());
    }

    #[test]
    fn write_scaffold_files_rejects_absolute_path() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        let entries = vec![entry("/tmp/evil", "pwned")];

        let result = write_scaffold_files(&claude_dir, &entries).unwrap();
        assert_eq!(result.files[0].status, ScaffoldFileStatus::Failed);
    }

    #[test]
    fn write_scaffold_files_rejects_null_bytes() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        let entries = vec![entry("evil\0.md", "pwned")];

        let result = write_scaffold_files(&claude_dir, &entries).unwrap();
        assert_eq!(result.files[0].status, ScaffoldFileStatus::Failed);
    }

    // ── MockProjectScaffolder ─────────────────────────────────────────

    #[tokio::test]
    async fn mock_scaffolder_returns_configured_result() {
        let expected = ScaffoldResult {
            files: vec![ScaffoldedFile {
                path: "CLAUDE.md".into(),
                status: ScaffoldFileStatus::Created,
                reason: None,
            }],
            created_count: 1,
            skipped_count: 0,
        };
        let mock = MockProjectScaffolder::with_result(expected);
        let result = mock
            .scaffold(&sample_profile_rust(), &sample_report(), &sample_plan())
            .await
            .unwrap();
        assert_eq!(result.created_count, 1);
        assert_eq!(result.files[0].status, ScaffoldFileStatus::Created);
    }

    #[tokio::test]
    async fn mock_scaffolder_without_result_returns_error() {
        let mock = MockProjectScaffolder::without_result();
        let result = mock
            .scaffold(&sample_profile_rust(), &sample_report(), &sample_plan())
            .await;
        assert!(result.is_err());
    }
}
