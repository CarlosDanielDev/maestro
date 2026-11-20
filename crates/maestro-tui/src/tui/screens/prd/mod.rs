//! Interactive PRD screen (#321) — full implementation in this module.
//!
//! Sections rendered: Vision, Goals, Non-Goals, Current State,
//! Stakeholders, Timeline. Goals and Non-Goals are editable in-screen.
//! Current State is auto-populated from GitHub via `prd::sync`.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod chips;
pub mod draw;
pub mod explore;
mod fit_hints;
pub mod input;
pub mod state;

#[allow(unused_imports)]
pub use state::{
    EditTarget, PrdAction, PrdPane, PrdSaveStatus, PrdScreen, PrdSection, PrdSyncStatus,
};
