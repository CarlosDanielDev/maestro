//! Concentric/radial bipartite agent-graph view.
//!
//! Renders active sessions as nodes on an inner ring and the files they touch
//! on an outer ring, with edges connecting agents to their files. The layout
//! is deterministic (no force-directed iteration) so snapshot tests can
//! verify the rendered output is pixel-stable.
//!
//! See `docs/adr/001-agent-graph-viz.md` for the design rationale.

mod animation;
pub(crate) mod layout;
pub(crate) mod model;
pub(crate) mod personalities;
pub(crate) mod render;
