//! Schema-driven settings renderer.
//!
//! Builds `SettingsField` entries from a `TableSchema` (defined in
//! `crate::config::schema`) and writes widget values back through a single
//! `toml::Value` round-trip. Wired behind the `schema_driven_settings`
//! feature flag (default off). No production tab consumes this module yet —
//! it is an opt-in path for future tab migrations.

#[allow(dead_code)]
pub(crate) mod build;
#[allow(dead_code)]
pub(crate) mod sync;

#[cfg(test)]
pub(crate) mod test_fixture;
#[cfg(test)]
mod tests;
