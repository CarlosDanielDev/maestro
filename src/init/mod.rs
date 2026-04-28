//! Project technology detection used by `maestro init` and the
//! Settings → "Reset Settings" action. See issue #505.

pub mod detector;
pub mod merge;
pub mod template;
pub mod walk;

pub use detector::{DetectedStack, FsProjectDetector, ProjectDetector};

#[cfg(test)]
pub use detector::FakeProjectDetector;

use anyhow::Result;
use std::path::Path;

/// What [`render_or_merge`] returns: either a freshly rendered template or
/// a merged TOML produced by combining detected defaults with an existing
/// user-edited file.
#[derive(Debug, Clone)]
pub enum RenderOutcome {
    Fresh {
        stacks: Vec<DetectedStack>,
        content: String,
    },
    Merged {
        stacks: Vec<DetectedStack>,
        report: merge::MergeReport,
    },
}

/// Top-level orchestration used by both the CLI and the TUI:
/// 1. Run the detector at `project_root`.
/// 2. Render the template for the detected stack list.
/// 3. If `existing` is `Some`, merge instead — preserving every key the
///    user already had on disk.
pub fn render_or_merge(
    detector: &dyn ProjectDetector,
    project_root: &Path,
    existing: Option<&str>,
) -> Result<RenderOutcome> {
    let stacks = detector.detect(project_root);
    let defaults = template::render(&stacks);
    match existing {
        None => Ok(RenderOutcome::Fresh {
            stacks,
            content: defaults,
        }),
        Some(prev) => {
            let report = merge::merge_preserving_user_keys(prev, &defaults)?;
            Ok(RenderOutcome::Merged { stacks, report })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_or_merge_fresh_returns_rendered_template() {
        let detector = FakeProjectDetector::new(vec![DetectedStack::Node]);
        let outcome = render_or_merge(&detector, std::path::Path::new("."), None).expect("fresh");
        match outcome {
            RenderOutcome::Fresh { stacks, content } => {
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
}
