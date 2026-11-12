//! Project technology detection used by `maestro init` and the
//! Settings → "Reset Settings" action. See issue #505.

pub mod detector;
pub mod merge;
pub mod scaffold;
pub mod template;
pub mod walk;

pub use detector::{DetectedStack, FsProjectDetector, ProjectDetector};

#[cfg(test)]
pub use detector::FakeProjectDetector;

use anyhow::Result;
use std::path::Path;

/// What [`render_or_merge`] returns: either a freshly rendered template or
/// a merged TOML produced by combining detected defaults with an existing
/// user-edited file. `guesses` lists the keys whose values were inferred
/// rather than confirmed by the tree.
#[derive(Debug, Clone)]
pub enum RenderOutcome {
    Fresh {
        stacks: Vec<DetectedStack>,
        content: String,
        guesses: Vec<&'static str>,
    },
    Merged {
        stacks: Vec<DetectedStack>,
        report: merge::MergeReport,
    },
}

/// Top-level orchestration used by both the CLI and the TUI:
/// 1. Run the detector at `project_root`.
/// 2. Render the template for the detected stack list, refining the
///    primary Python defaults against the tree and filling `project.repo`
///    from the `origin` remote when it parses as a GitHub URL (#1150).
/// 3. If `existing` is `Some`, merge instead — preserving every key the
///    user already had on disk.
pub fn render_or_merge(
    detector: &dyn ProjectDetector,
    project_root: &Path,
    existing: Option<&str>,
) -> Result<RenderOutcome> {
    render_or_merge_with_provider(
        detector,
        project_root,
        existing,
        &template::ProviderTemplate::github(),
    )
}

pub fn render_or_merge_with_provider(
    detector: &dyn ProjectDetector,
    project_root: &Path,
    existing: Option<&str>,
    provider: &template::ProviderTemplate,
) -> Result<RenderOutcome> {
    let stacks = detector.detect(project_root);
    let repo = repo_slug_from_root(project_root);
    let (primary, guesses) = python_refinement(&stacks, project_root);
    let defaults =
        template::render_with_primary(&stacks, primary.as_ref(), provider, repo.as_deref());
    render_or_merge_defaults(stacks, defaults, existing, guesses)
}

fn render_or_merge_defaults(
    stacks: Vec<DetectedStack>,
    defaults: String,
    existing: Option<&str>,
    guesses: Vec<&'static str>,
) -> Result<RenderOutcome> {
    match existing {
        None => Ok(RenderOutcome::Fresh {
            stacks,
            content: defaults,
            guesses,
        }),
        Some(prev) => {
            let report = merge::merge_preserving_user_keys(prev, &defaults)?;
            Ok(RenderOutcome::Merged { stacks, report })
        }
    }
}

/// Refine the primary stack's defaults against the real tree. Only
/// Python carries refinements today (#1150): `pytest` only when pytest
/// configuration markers exist, else `python3 -m unittest`; and the
/// `python main.py` default is dropped (rendered as a comment) when
/// `main.py` is absent. The returned list names the keys whose values
/// were guessed rather than confirmed.
fn python_refinement(
    stacks: &[DetectedStack],
    root: &Path,
) -> (Option<template::StackDefaults>, Vec<&'static str>) {
    if stacks.first() != Some(&DetectedStack::Python) {
        return (None, Vec::new());
    }
    let mut defaults = template::StackDefaults::for_stack(DetectedStack::Python);
    let mut guesses = Vec::new();
    if !detector::pytest_configured(root) {
        defaults.test_command = "python3 -m unittest";
        guesses.push("test_command");
    }
    if !detector::python_entry_point(root) {
        defaults.run_command = "";
        guesses.push("run_command");
    }
    (Some(defaults), guesses)
}

/// Best-effort `owner/repo` slug parsed from the `origin` remote at
/// `root` (runs `git remote -v`). `None` when there is no origin or it
/// does not parse as a GitHub URL.
pub fn repo_slug_from_root(root: &Path) -> Option<String> {
    let verbose = git_remote_verbose(root);
    let url = first_origin_remote_url(&verbose)?;
    crate::provider::github_owner_repo(&url)
}

/// `git remote -v` output at `project_root`; empty when git fails or is
/// unavailable (e.g. the directory is not a repository).
pub fn git_remote_verbose(project_root: &Path) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project_root)
        .arg("remote")
        .arg("-v")
        .output();
    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        _ => String::new(),
    }
}

/// First `origin` URL from `git remote -v` output, preferring the
/// `(fetch)` entry when both push and fetch URLs exist.
pub fn first_origin_remote_url(remote_verbose: &str) -> Option<String> {
    let mut first_origin = None;
    for line in remote_verbose.lines() {
        let mut parts = line.split_whitespace();
        let name = parts.next();
        let url = parts.next();
        let kind = parts.next();
        if name != Some("origin") {
            continue;
        }
        let Some(url) = url else {
            continue;
        };
        if first_origin.is_none() {
            first_origin = Some(url.to_string());
        }
        if kind == Some("(fetch)") {
            return Some(url.to_string());
        }
    }
    first_origin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_or_merge_fresh_returns_rendered_template() {
        let detector = FakeProjectDetector::new(vec![DetectedStack::Node]);
        let outcome = render_or_merge(&detector, std::path::Path::new("."), None).expect("fresh");
        match outcome {
            RenderOutcome::Fresh {
                stacks, content, ..
            } => {
                assert_eq!(stacks, vec![DetectedStack::Node]);
                assert!(
                    content.contains("language = \"node\""),
                    "expected node language in:\n{content}"
                );
            }
            RenderOutcome::Merged { .. } => panic!("expected Fresh, got Merged"),
        }
    }

    #[test]
    fn render_or_merge_existing_returns_merged() {
        let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
        let existing = "[project]\ncustom_key = \"my-custom\"\n";
        let outcome =
            render_or_merge(&detector, std::path::Path::new("."), Some(existing)).expect("merge");
        match outcome {
            RenderOutcome::Merged { stacks, report } => {
                assert_eq!(stacks, vec![DetectedStack::Rust]);
                assert!(
                    report.merged_toml.contains("custom_key = \"my-custom\""),
                    "expected custom_key preserved in:\n{}",
                    report.merged_toml
                );
            }
            RenderOutcome::Fresh { .. } => panic!("expected Merged, got Fresh"),
        }
    }

    #[test]
    fn python_refinement_guesses_unittest_without_pytest_markers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n",
        )
        .unwrap();
        let (defaults, guesses) = python_refinement(&[DetectedStack::Python], dir.path());
        let defaults = defaults.expect("python stack refined");
        assert_eq!(defaults.test_command, "python3 -m unittest");
        assert_eq!(
            defaults.run_command, "",
            "no main.py → run_command refined away"
        );
        assert_eq!(guesses, vec!["test_command", "run_command"]);
    }

    #[test]
    fn python_refinement_keeps_pytest_with_markers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.pytest.ini_options]\nminversion = \"7.0\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("main.py"), "print(\"hi\")\n").unwrap();
        let (defaults, guesses) = python_refinement(&[DetectedStack::Python], dir.path());
        let defaults = defaults.expect("python stack refined");
        assert_eq!(defaults.test_command, "pytest");
        assert_eq!(defaults.run_command, "python main.py");
        assert!(guesses.is_empty(), "nothing guessed: {guesses:?}");
    }

    #[test]
    fn python_refinement_skips_non_python_primary() {
        let dir = tempfile::tempdir().unwrap();
        let (defaults, guesses) = python_refinement(&[DetectedStack::Rust], dir.path());
        assert!(defaults.is_none());
        assert!(guesses.is_empty());
    }

    #[test]
    fn render_or_merge_python_carries_refined_defaults_and_guesses() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n",
        )
        .unwrap();
        let detector = FakeProjectDetector::new(vec![DetectedStack::Python]);
        let outcome = render_or_merge(&detector, dir.path(), None).expect("fresh");
        match outcome {
            RenderOutcome::Fresh {
                stacks,
                content,
                guesses,
            } => {
                assert_eq!(stacks, vec![DetectedStack::Python]);
                assert_eq!(
                    content
                        .matches("test_command = \"python3 -m unittest\"")
                        .count(),
                    2,
                    "unittest in [project] and [gates]:\n{content}"
                );
                assert!(
                    !content.contains("\nrun_command = "),
                    "run_command commented without main.py:\n{content}"
                );
                assert_eq!(guesses, vec!["test_command", "run_command"]);
            }
            RenderOutcome::Merged { .. } => panic!("expected Fresh, got Merged"),
        }
    }

    #[test]
    fn repo_slug_none_for_directory_without_git() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(repo_slug_from_root(dir.path()), None);
    }

    #[test]
    fn repo_slug_parses_github_origin() {
        let dir = tempfile::tempdir().unwrap();
        let output = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .output()
            .expect("git available");
        assert!(output.status.success(), "git init failed");
        std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/acme/widgets.git",
            ])
            .current_dir(dir.path())
            .output()
            .expect("git available");
        assert_eq!(
            repo_slug_from_root(dir.path()).as_deref(),
            Some("acme/widgets")
        );
    }

    #[test]
    fn first_origin_remote_url_empty_for_no_origin() {
        assert_eq!(first_origin_remote_url(""), None);
        assert_eq!(
            first_origin_remote_url("upstream\thttps://example.com/x.git (fetch)\n"),
            None
        );
    }
}
