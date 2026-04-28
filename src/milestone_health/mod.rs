//! Milestone health-check analysis layer (#500).
//!
//! Pure-function modules that inspect a milestone's open issues and its
//! description for DOR readiness and dependency-graph coherence, then
//! generate a corrected description string. The TUI wizard at
//! `crate::tui::screens::milestone_health` consumes these primitives.

pub mod dor;
pub mod graph;
pub mod patch;
pub mod report;
pub mod types;

pub use dor::check_issues;
pub use graph::analyze;
pub use patch::generate_patch;
