//! Canonical text-input widget for the maestro TUI (#963).
//!
//! One editing engine every input site can share, replacing the three tiers
//! that exist today (rich `prompt_input`/`interaction`, decent
//! `wizard_fields::TextAreaField`, and hand-rolled `String`+cursor sites).
//! This is the foundation only — no site is migrated here; migrations land in
//! #968–#972.
//!
//! - [`Input`] / [`InputConfig`] — the widget and its builder ([`core`]).
//! - [`render`] — wrap + manual cursor placement ([`render`]).

mod core;
mod render;

// Foundation only: no non-test site consumes these yet, so the re-exports read
// as unused imports until the #968–#972 migrations land. `-A dead_code` in CI
// covers the types themselves but not the `pub use`, so allow it here.
#[allow(unused_imports)]
pub use core::{Input, InputConfig};
#[allow(unused_imports)]
pub use render::render;
