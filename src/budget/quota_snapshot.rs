//! Provider quota snapshots — read-only view of per-provider quota state for
//! the TUI rollup (#776/#848).
//!
//! The trait `ProviderQuotaSnapshots` returns a `QuotaRow` for providers that
//! report quotas (MiniMax today; OpenCode/Codex may follow). Returns `None`
//! for providers without a quota concept.
//!
//! Production impl `MinimaxQuotaSnapshots` wraps `Arc<MinimaxQuota>` and only
//! reports for `provider_id == "minimax"`.

use std::sync::Arc;

use crate::agent_provider::minimax::quota::{MinimaxQuota, QuotaStatus};

/// One quota row for a provider, ready to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaRow {
    pub used: u32,
    pub limit: u32,
    pub window_label: &'static str,
    pub status: QuotaBucket,
}

/// Render bucket — drives row colour in the TUI rollup. Distinct from
/// `QuotaStatus` because the latter carries `pct` (not needed by the view-model)
/// and we want to keep the TUI free of agent-provider crate types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaBucket {
    Ok,
    Warn,
    Refused,
}

/// Source of quota rows. Tests inject `FakeProviderQuotaSnapshots`;
/// production wires `MinimaxQuotaSnapshots`.
pub trait ProviderQuotaSnapshots: Send + Sync {
    fn quota_for(&self, provider_id: &str) -> Option<QuotaRow>;
}

/// Production impl: returns a `QuotaRow` for `provider_id == "minimax"` only.
/// Wraps `Arc<MinimaxQuota>` (already shipped by #774).
#[derive(Debug, Clone)]
pub struct MinimaxQuotaSnapshots {
    quota: Arc<MinimaxQuota>,
}

impl MinimaxQuotaSnapshots {
    pub fn new(quota: Arc<MinimaxQuota>) -> Self {
        Self { quota }
    }
}

impl ProviderQuotaSnapshots for MinimaxQuotaSnapshots {
    fn quota_for(&self, provider_id: &str) -> Option<QuotaRow> {
        if provider_id != "minimax" {
            return None;
        }
        let bucket = match self.quota.check() {
            QuotaStatus::Ok { .. } => QuotaBucket::Ok,
            QuotaStatus::Warn { .. } => QuotaBucket::Warn,
            QuotaStatus::Refused { .. } => QuotaBucket::Refused,
        };
        Some(QuotaRow {
            used: self.quota.used_in_window(),
            limit: self.quota.limit(),
            window_label: "5h",
            status: bucket,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Test fake — returns whatever the caller seeded for each provider id.
    struct FakeProviderQuotaSnapshots {
        entries: HashMap<String, QuotaRow>,
    }

    impl FakeProviderQuotaSnapshots {
        fn new() -> Self {
            Self {
                entries: HashMap::new(),
            }
        }

        fn with(mut self, provider_id: &str, row: QuotaRow) -> Self {
            self.entries.insert(provider_id.to_string(), row);
            self
        }
    }

    impl ProviderQuotaSnapshots for FakeProviderQuotaSnapshots {
        fn quota_for(&self, provider_id: &str) -> Option<QuotaRow> {
            self.entries.get(provider_id).copied()
        }
    }

    #[test]
    fn fake_quota_snapshots_returns_row_for_known_provider() {
        let row = QuotaRow {
            used: 4,
            limit: 5,
            window_label: "5h",
            status: QuotaBucket::Warn,
        };
        let fake = FakeProviderQuotaSnapshots::new().with("minimax", row);
        assert_eq!(fake.quota_for("minimax"), Some(row));
    }

    #[test]
    fn fake_quota_snapshots_returns_none_for_unknown_provider() {
        let fake = FakeProviderQuotaSnapshots::new();
        assert!(fake.quota_for("unknown").is_none());
    }

    #[test]
    fn quota_bucket_at_95_pct_is_refused() {
        let row = QuotaRow {
            used: 95,
            limit: 100,
            window_label: "5h",
            status: QuotaBucket::Refused,
        };
        assert!(matches!(row.status, QuotaBucket::Refused));
    }

    #[test]
    fn quota_bucket_at_100_pct_is_refused() {
        let row = QuotaRow {
            used: 100,
            limit: 100,
            window_label: "5h",
            status: QuotaBucket::Refused,
        };
        assert!(matches!(row.status, QuotaBucket::Refused));
    }

    #[test]
    fn quota_bucket_at_50_pct_is_ok() {
        let row = QuotaRow {
            used: 50,
            limit: 100,
            window_label: "5h",
            status: QuotaBucket::Ok,
        };
        assert!(matches!(row.status, QuotaBucket::Ok));
    }
}
