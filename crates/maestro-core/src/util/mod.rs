//! Shared utility helpers, grouped by domain.

pub mod formatting;
pub mod sanitize;
pub mod symlink_write;
pub mod validation;

// Re-export all public items so existing `use crate::util::*` call sites continue to work.
pub use formatting::*;
pub use symlink_write::*;
pub use validation::*;
