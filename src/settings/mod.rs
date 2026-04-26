pub mod claude_settings;

#[cfg(test)]
mod claude_settings_tests;

pub use claude_settings::{CavemanModeState, FsSettingsStore, SettingsStore};

// `CavemanWriteError` is exported only via `claude_settings::` for tests
// and library consumers; the binary itself never names it directly.
#[cfg(test)]
pub use claude_settings::CavemanWriteError;
