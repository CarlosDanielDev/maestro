//! Tests for `src/adapt/types.rs` — extracted to keep the impl file under
//! the 400-LOC budget. Loaded back via `#[cfg(test)] #[path = "types_tests.rs"] mod tests;`.

#![cfg(test)]

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
