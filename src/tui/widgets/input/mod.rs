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

pub use core::{Input, InputConfig};
pub use render::render;
