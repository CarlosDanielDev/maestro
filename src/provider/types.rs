use serde::{Deserialize, Serialize};

/// Which code hosting provider is configured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    Github,
    AzureDevops,
}

// Re-export all types from github::types under provider-agnostic names.
// Re-export GitHub types under provider-agnostic names.
// These are used by consumers that import from provider::types.
#[allow(unused_imports)]
pub use crate::provider::github::types::GhIssue as Issue;
#[allow(unused_imports)]
pub use crate::provider::github::types::GhMilestone as Milestone;
#[allow(unused_imports)]
pub use crate::provider::github::types::{MaestroLabel, Priority, SessionMode};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_default_is_github() {
        assert_eq!(ProviderKind::default(), ProviderKind::Github);
    }

    #[test]
    fn provider_kind_serializes_github() {
        let s = serde_json::to_string(&ProviderKind::Github).unwrap();
        assert_eq!(s, "\"github\"");
    }

    #[test]
    fn provider_kind_serializes_azure_devops() {
        let s = serde_json::to_string(&ProviderKind::AzureDevops).unwrap();
        assert_eq!(s, "\"azure_devops\"");
    }

    #[test]
    fn provider_kind_deserializes_github() {
        let k: ProviderKind = serde_json::from_str("\"github\"").unwrap();
        assert_eq!(k, ProviderKind::Github);
    }

    #[test]
    fn provider_kind_deserializes_azure_devops() {
        let k: ProviderKind = serde_json::from_str("\"azure_devops\"").unwrap();
        assert_eq!(k, ProviderKind::AzureDevops);
    }

    #[test]
    fn provider_kind_unknown_returns_err() {
        assert!(serde_json::from_str::<ProviderKind>("\"gitlab\"").is_err());
    }
}
