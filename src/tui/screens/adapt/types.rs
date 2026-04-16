use crate::adapt::AdaptConfig;
use crate::adapt::types::{AdaptPlan, AdaptReport, MaterializeResult, ProjectProfile};
use std::path::PathBuf;

/// Wizard step state machine for the adapt pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptStep {
    Configure,
    Scanning,
    Analyzing,
    Planning,
    Materializing,
    Complete,
    Failed,
}

impl AdaptStep {
    pub fn is_progress(&self) -> bool {
        matches!(
            self,
            Self::Scanning | Self::Analyzing | Self::Planning | Self::Materializing
        )
    }

}

/// Configuration form state for the adapt wizard.
#[derive(Debug, Clone)]
pub struct AdaptWizardConfig {
    pub path: String,
    pub dry_run: bool,
    pub scan_only: bool,
    pub no_issues: bool,
    pub model: String,
}

impl Default for AdaptWizardConfig {
    fn default() -> Self {
        Self {
            path: ".".to_string(),
            dry_run: false,
            scan_only: false,
            no_issues: false,
            model: "sonnet".to_string(),
        }
    }
}

impl AdaptWizardConfig {
    pub fn to_adapt_config(&self) -> AdaptConfig {
        AdaptConfig {
            path: PathBuf::from(&self.path),
            dry_run: self.dry_run,
            scan_only: self.scan_only,
            no_issues: self.no_issues,
            model: if self.model.is_empty() {
                None
            } else {
                Some(self.model.clone())
            },
        }
    }
}

/// Accumulated results from pipeline phases.
/// Serializable so completed phases can be cached and resumed on failure.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AdaptResults {
    pub profile: Option<ProjectProfile>,
    pub report: Option<AdaptReport>,
    pub plan: Option<AdaptPlan>,
    pub materialize: Option<MaterializeResult>,
}

impl AdaptResults {
    /// Cache file path for adapt pipeline resumption.
    fn cache_path() -> std::path::PathBuf {
        std::path::PathBuf::from(".maestro").join("adapt-cache.json")
    }

    /// Save current results to cache file. Silently ignores errors.
    pub fn save_cache(&self) {
        let path = Self::cache_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Load cached results if available.
    pub fn load_cache() -> Option<Self> {
        let path = Self::cache_path();
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Delete the cache file (called after successful completion).
    pub fn clear_cache() {
        let _ = std::fs::remove_file(Self::cache_path());
    }

    /// Determine which phase to resume from based on cached results.
    pub fn resume_step(&self) -> AdaptStep {
        if self.plan.is_some() {
            AdaptStep::Materializing
        } else if self.report.is_some() {
            AdaptStep::Planning
        } else if self.profile.is_some() {
            AdaptStep::Analyzing
        } else {
            AdaptStep::Scanning
        }
    }
}

/// Error from a failed adapt phase.
#[derive(Debug, Clone)]
pub struct AdaptError {
    pub phase: AdaptStep,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapt_step_copy_semantics() {
        let step = AdaptStep::Configure;
        let copied = step;
        assert_eq!(step, copied);
    }

    #[test]
    fn adapt_step_is_progress() {
        assert!(!AdaptStep::Configure.is_progress());
        assert!(AdaptStep::Scanning.is_progress());
        assert!(AdaptStep::Analyzing.is_progress());
        assert!(AdaptStep::Planning.is_progress());
        assert!(AdaptStep::Materializing.is_progress());
        assert!(!AdaptStep::Complete.is_progress());
        assert!(!AdaptStep::Failed.is_progress());
    }

    #[test]
    fn wizard_config_default_values() {
        let config = AdaptWizardConfig::default();
        assert_eq!(config.path, ".");
        assert!(!config.dry_run);
        assert!(!config.scan_only);
        assert!(!config.no_issues);
        assert_eq!(config.model, "sonnet");
    }

    #[test]
    fn wizard_config_to_adapt_config() {
        let config = AdaptWizardConfig::default();
        let adapt = config.to_adapt_config();
        assert_eq!(adapt.path, PathBuf::from("."));
        assert!(!adapt.dry_run);
        assert!(!adapt.scan_only);
        assert!(!adapt.no_issues);
        assert_eq!(adapt.model, Some("sonnet".to_string()));
    }

    #[test]
    fn wizard_config_empty_model_becomes_none() {
        let config = AdaptWizardConfig {
            model: String::new(),
            ..Default::default()
        };
        let adapt = config.to_adapt_config();
        assert_eq!(adapt.model, None);
    }

    #[test]
    fn adapt_results_default_is_all_none() {
        let results = AdaptResults::default();
        assert!(results.profile.is_none());
        assert!(results.report.is_none());
        assert!(results.plan.is_none());
        assert!(results.materialize.is_none());
    }

    // resume_step tests are in mod.rs tests (they need make_mock_profile helper)
}
