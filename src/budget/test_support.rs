//! Shared test fakes for the budget module (#776/#848/#854).
//!
//! Gated `#[cfg(test)]` at the parent module declaration in `src/budget.rs`
//! so this code is never present in release builds. Used by:
//!   - `budget::quota_snapshot` tests
//!   - `tui::token_dashboard::provider_rollup` tests
//!   - `tui::snapshot_tests::token_dashboard_provider_rollup` tests

use std::collections::HashMap;

use crate::budget::quota_snapshot::{ProviderQuotaSnapshots, QuotaRow};

/// Test fake — returns whatever the caller seeded for each provider id.
pub struct FakeProviderQuotaSnapshots {
    entries: HashMap<String, QuotaRow>,
}

impl FakeProviderQuotaSnapshots {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn with(mut self, provider_id: &str, row: QuotaRow) -> Self {
        self.entries.insert(provider_id.to_string(), row);
        self
    }
}

impl ProviderQuotaSnapshots for FakeProviderQuotaSnapshots {
    fn quota_for(&self, provider_id: &str) -> Option<QuotaRow> {
        self.entries.get(provider_id).copied()
    }
}
