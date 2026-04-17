//! Integration tests for the adapt pipeline (scanner → analyzer → planner → materializer).
//!
//! Uses mock implementations for analyzer and planner to test the pipeline
//! stages in isolation without calling Claude.

use crate::adapt::analyzer::{MockProjectAnalyzer, ProjectAnalyzer};
use crate::adapt::planner::{AdaptPlanner, MockAdaptPlanner};
use crate::adapt::types::*;
use std::path::PathBuf;

fn sample_profile() -> ProjectProfile {
    ProjectProfile {
        name: "test-project".into(),
        root: PathBuf::from("/tmp/test"),
        language: ProjectLanguage::Rust,
        manifests: vec![PathBuf::from("Cargo.toml")],
        config_files: vec![],
        entry_points: vec![PathBuf::from("src/main.rs")],
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
            provider: Some("github_actions".into()),
            config_files: vec![],
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
    }
}

fn sample_report() -> AdaptReport {
    AdaptReport {
        summary: "A Rust CLI with async runtime".into(),
        modules: vec![
            ModuleDescription {
                path: "src/main.rs".into(),
                purpose: "Entry point".into(),
                complexity: "low".into(),
            },
            ModuleDescription {
                path: "src/config.rs".into(),
                purpose: "Configuration".into(),
                complexity: "medium".into(),
            },
        ],
        tech_debt_items: vec![TechDebtItem {
            title: "Missing unit tests".into(),
            description: "No tests for config parsing".into(),
            location: "src/config.rs".into(),
            suggested_fix: "Add unit tests".into(),
            category: TechDebtCategory::MissingTests,
            severity: TechDebtSeverity::Medium,
        }],
    }
}

fn sample_plan() -> AdaptPlan {
    AdaptPlan {
        milestones: vec![
            PlannedMilestone {
                title: "M0: Foundation".into(),
                description: "Initial setup and scaffolding".into(),
                issues: vec![PlannedIssue {
                    title: "feat: project scaffolding".into(),
                    body: "## Overview\n\nSet up the project.".into(),
                    labels: vec!["enhancement".into()],
                    blocked_by_titles: vec![],
                }],
            },
            PlannedMilestone {
                title: "M1: Testing".into(),
                description: "Add test coverage".into(),
                issues: vec![PlannedIssue {
                    title: "test: add config tests".into(),
                    body: "## Overview\n\nAdd config tests.".into(),
                    labels: vec!["testing".into()],
                    blocked_by_titles: vec!["feat: project scaffolding".into()],
                }],
            },
        ],
        maestro_toml_patch: Some("[project]\nrepo = \"owner/repo\"".into()),
        workflow_guide: None,
    }
}

/// End-to-end: mock analyzer produces report → mock planner produces plan from that report.
#[tokio::test]
async fn pipeline_analyze_then_plan_with_mocks() {
    let profile = sample_profile();

    // Phase 2: Analyze
    let analyzer = MockProjectAnalyzer::with_report(sample_report());
    let report = analyzer.analyze(&profile).await.unwrap();
    assert_eq!(report.modules.len(), 2);
    assert_eq!(report.tech_debt_items.len(), 1);

    // Phase 3: Plan
    let planner = MockAdaptPlanner::with_plan(sample_plan());
    let plan = planner.plan(&profile, &report, None).await.unwrap();
    assert_eq!(plan.milestones.len(), 2);
    assert_eq!(plan.milestones[0].issues.len(), 1);
    assert_eq!(plan.milestones[1].issues[0].blocked_by_titles.len(), 1);
    assert!(plan.maestro_toml_patch.is_some());
}

/// Pipeline handles empty analysis gracefully.
#[tokio::test]
async fn pipeline_empty_analysis_produces_valid_plan() {
    let profile = sample_profile();

    let analyzer = MockProjectAnalyzer::with_report(AdaptReport {
        summary: "Empty project".into(),
        modules: vec![],
        tech_debt_items: vec![],
    });
    let report = analyzer.analyze(&profile).await.unwrap();
    assert!(report.modules.is_empty());

    let planner = MockAdaptPlanner::with_plan(AdaptPlan {
        milestones: vec![],
        maestro_toml_patch: None,
        workflow_guide: None,
    });
    let plan = planner.plan(&profile, &report, None).await.unwrap();
    assert!(plan.milestones.is_empty());
    assert!(plan.maestro_toml_patch.is_none());
}

/// Analyzer failure propagates correctly.
#[tokio::test]
async fn pipeline_analyzer_failure_propagates() {
    let profile = sample_profile();
    let analyzer = MockProjectAnalyzer::without_report();
    let err = analyzer.analyze(&profile).await;
    assert!(err.is_err());
}

/// Planner failure propagates correctly.
#[tokio::test]
async fn pipeline_planner_failure_propagates() {
    let profile = sample_profile();
    let report = sample_report();
    let planner = MockAdaptPlanner::without_plan();
    let err = planner.plan(&profile, &report, None).await;
    assert!(err.is_err());
}
