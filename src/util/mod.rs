//! Shared utility helpers, grouped by domain.

pub mod formatting;
pub mod validation;

// Re-export all public items so existing `use crate::util::*` call sites continue to work.
pub use formatting::*;
pub use validation::*;
