//! Interactive PRD module (#321).
//!
//! `prd::model` defines the `Prd` data structure that backs the PRD TUI
//! screen. `prd::store` persists it to `<repo_root>/.maestro/prd.toml`.
//! `prd::export` renders to markdown for sharing.

pub mod discover;
pub mod export;
pub mod ingest;
pub mod model;
pub mod store;
pub mod sync;
