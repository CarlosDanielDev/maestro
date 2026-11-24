//! The Files lens of Dashboard Deps (§02.20 of
//! `docs/design-system/index.html`, #1292): agents on the left, the
//! repo-relative files they edit on the right, edges between, contested
//! files first. It replaced the ring picture of `TuiMode::AgentGraph`,
//! which is now an alias that opens this lens.

pub(crate) mod conflict_toast;
mod file_card;
pub(crate) mod ladder;
pub(crate) mod model;
pub(crate) mod personalities;
