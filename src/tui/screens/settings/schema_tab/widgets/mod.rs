//! Widget primitives for dynamic-cardinality schema fields.
//!
//! `DynamicMapWidget` renders a `FieldKind::Map` as a sub-tab strip; each
//! sub-tab owns a per-entry field group. `DynamicRowsWidget` renders a
//! `FieldKind::VecOfStruct` as an ordered row table. Both expose Add and
//! Remove modals, a 5-second undo window, and writeback via
//! `serialize_to_toml`.

pub mod clock;
pub mod dynamic_map;
mod dynamic_map_draw;
pub mod dynamic_rows;
mod dynamic_rows_draw;
pub mod entry_state;
pub mod identifier;
pub mod undo;

#[cfg(test)]
pub(crate) mod test_fixture;

#[cfg(test)]
mod dynamic_map_tests;
#[cfg(test)]
mod dynamic_rows_tests;

pub use dynamic_map::DynamicMapWidget;
pub use dynamic_rows::DynamicRowsWidget;
