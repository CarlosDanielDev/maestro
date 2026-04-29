//! Spike-only module (issue #513, ADR 001). Never landed on `main`.
//!
//! Lives at the top of the crate (rather than under `tui/`) because it must be
//! reachable from the `examples/agent_graph_spike.rs` binary via `src/lib.rs`,
//! and `crate::tui` is not part of the library facade.
//!
//! See `docs/adr/001-agent-graph-viz.md` for the full design rationale and
//! cleanup instructions.

pub mod layout;
pub mod model;
pub mod render;
