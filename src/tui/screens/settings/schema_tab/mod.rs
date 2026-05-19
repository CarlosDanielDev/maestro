//! Schema-driven settings renderer.
//!
//! Builds `SettingsField` entries from a `TableSchema` (defined in
//! `crate::config::schema`) and writes widget values back through a single
//! `toml::Value` round-trip. Consumed by every settings tab except Flags
//! (no widgets) and the multi-table tabs (Theme, Advanced) which dispatch
//! through `sync_multi_table`. The `schema_driven_settings` flag that
//! originally gated this path was removed in #716 after the per-tab
//! migrations landed.

#[allow(dead_code)]
pub(crate) mod build;
#[allow(dead_code)]
pub(crate) mod sync;

#[cfg(test)]
pub(crate) mod test_fixture;
#[cfg(test)]
mod tests;
