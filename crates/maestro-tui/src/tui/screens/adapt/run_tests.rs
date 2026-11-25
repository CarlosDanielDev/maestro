//! #1281: Stop after, the Review pause before writes, the project-relative
//! cache and Esc reaching the cancel token. Every test runs in a tempdir so
//! no cache lands in the repo.

use super::*;
use crate::adapt::types::*;
use crate::tui::screens::test_helpers::key_event;
use std::path::Path;

fn screen_in(dir: &Path) -> AdaptScreen {
    let mut s = AdaptScreen::new();
    s.results = AdaptResults::default();
    s.loaded_from_cache = false;
    s.config.path = dir.display().to_string();
    s
}

fn profile(root: &Path) -> ProjectProfile {
    ProjectProfile {
        name: "t".into(),
        root: root.to_path_buf(),
        language: ProjectLanguage::Rust,
        manifests: vec![],
        config_files: vec![],
        entry_points: vec![],
        source_stats: SourceStats {
            total_files: 1,
            total_lines: 1,
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
            is_git_repo: true,
            default_branch: Some("main".into()),
            remote_url: None,
            commit_count: 1,
            recent_contributors: vec![],
        },
        dependencies: DependencySummary::default(),
        directory_tree: String::new(),
        has_maestro_config: false,
        has_workflow_docs: false,
    }
}

fn report() -> AdaptReport {
    AdaptReport {
        summary: "s".into(),
        modules: vec![],
        tech_debt_items: vec![],
    }
}

fn plan() -> AdaptPlan {
    AdaptPlan {
        milestones: vec![PlannedMilestone {
            title: "M0: Foundation".into(),
            description: "d".into(),
            issues: vec![PlannedIssue {
                title: "feat: setup".into(),
                body: "b".into(),
                labels: vec!["enhancement".into()],
                blocked_by_titles: vec![],
            }],
        }],
        maestro_toml_patch: None,
        workflow_guide: None,
    }
}

/// Every file under `dir`, relative, sorted.
fn files_under(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, at: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(at).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn adapt_review_pause_before_write() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    StopAfter::Full.apply(&mut s.config);
    assert!(!s.config.dry_run, "setup: dry run is off");
    s.results.profile = Some(profile(dir.path()));
    s.results.report = Some(report());

    let cmd = s.complete_plan(plan());

    assert_eq!(s.step, AdaptStep::Review, "plan completion stops at Review");
    assert!(cmd.is_none(), "no scaffold or materialize is queued");
    assert_eq!(
        files_under(dir.path()),
        [".maestro/adapt-cache.json"],
        "only the resume cache is written before Review"
    );

    let first = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(first, ScreenAction::None, "the first ⏎ only arms");
    assert_eq!(s.step, AdaptStep::Review);

    let second = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert!(
        matches!(second, ScreenAction::StartAdaptPipeline(_)),
        "the second ⏎ inside the window starts the write"
    );
    assert_eq!(s.step, AdaptStep::Scaffolding);
}

#[test]
fn review_other_key_disarms() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    StopAfter::Full.apply(&mut s.config);
    s.results.profile = Some(profile(dir.path()));
    s.results.report = Some(report());
    s.complete_plan(plan());

    s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
    let after = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);

    assert_eq!(after, ScreenAction::None, "a stray key disarms: ⏎ re-arms");
    assert_eq!(s.step, AdaptStep::Review);
}

#[test]
fn plan_dry_run_completes_without_review() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    s.results.profile = Some(profile(dir.path()));
    s.results.report = Some(report());

    let cmd = s.complete_plan(plan());

    assert_eq!(s.step, AdaptStep::Complete, "the default stops after Plan");
    assert!(cmd.is_none());
}

#[test]
fn adapt_cache_path_relative() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());

    s.complete_scan(profile(dir.path()));

    let cache = dir.path().join(".maestro/adapt-cache.json");
    assert!(
        cache.exists(),
        "the cache lives under the project's .maestro/"
    );
    let data = std::fs::read_to_string(cache).unwrap();
    let back: AdaptResults = serde_json::from_str(&data).unwrap();
    assert!(back.profile.is_some());
}

#[test]
fn configure_space_and_arrows_move_stop_after() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    s.selected_field = 1;

    s.handle_input(&key_event(KeyCode::Right), InputMode::Normal);
    assert_eq!(s.config.stop_after, StopAfter::Full);
    assert!(!s.config.dry_run, "the radio sets the flags");
    s.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert_eq!(s.config.stop_after, StopAfter::Scan);
    assert!(s.config.scan_only);
    s.handle_input(&key_event(KeyCode::Left), InputMode::Normal);
    assert_eq!(s.config.stop_after, StopAfter::Full);
}

#[test]
fn progress_esc_cancels_the_token() {
    // Materializing: Esc reaches the token and the screen stays.
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    s.step = AdaptStep::Materializing;
    let token = s.cancel.clone();

    s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);

    assert!(token.is_cancelled(), "Esc reaches the running materializer");
}

#[test]
fn plan_dry_run_writes_nothing() {
    // The default run end to end through the screen: the resume cache is
    // the only file it ever writes, and it is gone at Complete. The PRD
    // stays in the results (`tui/mod.rs` writes it only in the Scaffold
    // arm, after Review).
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    assert_eq!(s.config.stop_after, StopAfter::Plan);

    let next = s.complete_scan(profile(dir.path()));
    assert!(matches!(
        next,
        Some(crate::tui::app::TuiCommand::RunAdaptAnalyze(..))
    ));
    let next = s.complete_analyze(report());
    assert!(matches!(
        next,
        Some(crate::tui::app::TuiCommand::RunAdaptConsolidate(..))
    ));
    let next = s.complete_consolidate("# PRD".into());
    assert!(matches!(
        next,
        Some(crate::tui::app::TuiCommand::RunAdaptPlan(..))
    ));
    let next = s.complete_plan(plan());

    assert!(next.is_none(), "no scaffold or materialize command");
    assert_eq!(s.step, AdaptStep::Complete);
    assert_eq!(s.results.prd_content.as_deref(), Some("# PRD"));
    assert_eq!(
        files_under(dir.path()),
        Vec::<String>::new(),
        "nothing on disk"
    );
}

#[test]
fn esc_while_writing_stops_after_the_item_and_shows_the_result() {
    // Esc at a write step does not pop: the result of what was written
    // must still reach the screen, so a stopped run is never silent.
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    StopAfter::Full.apply(&mut s.config);
    s.step = AdaptStep::Scaffolding;
    s.results.plan = Some(plan());
    s.results.report = Some(report());

    let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::None, "stays to show what was written");
    assert!(!s.is_cancelled(), "the result is not dropped");
    assert!(s.cancel.is_cancelled());

    let next = s.complete_scaffold(ScaffoldResult {
        files: vec![],
        created_count: 0,
        skipped_count: 0,
    });
    assert!(next.is_none(), "no materialize after a stop");
    assert_eq!(s.step, AdaptStep::Complete);
}

#[test]
fn resume_with_cached_plan_goes_to_review_not_write() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    StopAfter::Full.apply(&mut s.config);
    s.loaded_from_cache = true;
    s.results.profile = Some(profile(dir.path()));
    s.results.report = Some(report());
    s.results.plan = Some(plan());

    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);

    assert_eq!(
        action,
        ScreenAction::None,
        "a cached plan never writes on ⏎"
    );
    assert_eq!(s.step, AdaptStep::Review);
}
