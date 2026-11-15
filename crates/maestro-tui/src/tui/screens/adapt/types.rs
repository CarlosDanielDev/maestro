use crate::adapt::AdaptConfig;
use crate::adapt::prd_source::PrdSource;
use crate::adapt::types::{
    AdaptPlan, AdaptReport, MaterializeResult, ProjectProfile, ScaffoldResult,
};
use std::path::PathBuf;

/// Wizard step state machine for the adapt pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptStep {
    Configure,
    Scanning,
    Analyzing,
    Consolidating,
    Planning,
    Scaffolding,
    Materializing,
    Complete,
    Failed,
}

impl AdaptStep {
    pub fn is_progress(&self) -> bool {
        matches!(
            self,
            Self::Scanning
                | Self::Analyzing
                | Self::Consolidating
                | Self::Planning
                | Self::Scaffolding
                | Self::Materializing
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
    pub prd_source: PrdSource,
}

impl Default for AdaptWizardConfig {
    fn default() -> Self {
        Self {
            path: ".".to_string(),
            dry_run: false,
            scan_only: false,
            no_issues: false,
            model: "sonnet".to_string(),
            prd_source: PrdSource::default(),
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
            prd_source: self.prd_source,
        }
    }
}

/// Accumulated results from pipeline phases.
/// Serializable so completed phases can be cached and resumed on failure.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AdaptResults {
    pub profile: Option<ProjectProfile>,
    pub report: Option<AdaptReport>,
    #[serde(default)]
    pub prd_content: Option<String>,
    pub plan: Option<AdaptPlan>,
    #[serde(default)]
    pub scaffold: Option<ScaffoldResult>,
    pub materialize: Option<MaterializeResult>,
}

impl AdaptResults {
    /// Cache file path for adapt pipeline resumption.
    fn cache_path() -> std::path::PathBuf {
        std::path::PathBuf::from(".maestro").join("adapt-cache.json")
    }

    /// Save current results to cache file. A failed write is logged, never
    /// fatal: the cache only speeds up a resumed run.
    pub fn save_cache(&self) {
        if let Err(e) = self.save_cache_in(std::path::Path::new(".")) {
            tracing::warn!("adapt cache not saved: {e}");
        }
    }

    fn save_cache_in(&self, root: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        // Refuses a repo-planted `.maestro -> ~/x` or symlinked cache file (#1313).
        crate::util::write_contained(root, &Self::cache_path(), &json)
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
        if self.scaffold.is_some() {
            AdaptStep::Materializing
        } else if self.plan.is_some() {
            AdaptStep::Scaffolding
        } else if self.prd_content.is_some() {
            AdaptStep::Planning
        } else if self.report.is_some() {
            AdaptStep::Consolidating
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

    // ── Cache symlink hardening (#1313) ───────────────────────────────

    #[test]
    fn save_cache_in_writes_cache_under_real_maestro_dir() {
        let dir = tempfile::TempDir::new().unwrap();

        AdaptResults::default().save_cache_in(dir.path()).unwrap();

        let written = dir.path().join(".maestro/adapt-cache.json");
        let data = std::fs::read_to_string(written).unwrap();
        assert!(serde_json::from_str::<AdaptResults>(&data).is_ok());
    }

    #[test]
    fn save_cache_in_refuses_symlinked_maestro_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".maestro")).unwrap();

        let err = AdaptResults::default()
            .save_cache_in(dir.path())
            .unwrap_err();

        assert!(err.to_string().contains("symlink"), "got: {err}");
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn save_cache_in_refuses_symlinked_cache_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let victim = outside.path().join("victim.json");
        std::fs::write(&victim, "ORIGINAL").unwrap();
        std::fs::create_dir_all(dir.path().join(".maestro")).unwrap();
        std::os::unix::fs::symlink(&victim, dir.path().join(".maestro/adapt-cache.json")).unwrap();

        AdaptResults::default()
            .save_cache_in(dir.path())
            .unwrap_err();

        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "ORIGINAL");
    }

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
        assert!(AdaptStep::Consolidating.is_progress());
        assert!(AdaptStep::Planning.is_progress());
        assert!(AdaptStep::Scaffolding.is_progress());
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

    #[test]
    fn adapt_results_prd_content_defaults_to_none() {
        let results = AdaptResults::default();
        assert!(results.prd_content.is_none());
    }

    #[test]
    fn adapt_results_prd_content_survives_json_round_trip() {
        let results = AdaptResults {
            prd_content: Some("# PRD".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&results).unwrap();
        let rt: AdaptResults = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.prd_content, Some("# PRD".into()));
    }

    #[test]
    fn adapt_results_json_without_prd_field_deserializes_as_none() {
        let raw = r#"{"profile":null,"report":null,"plan":null,"materialize":null}"#;
        let result: Result<AdaptResults, _> = serde_json::from_str(raw);
        assert!(result.is_ok());
        assert!(result.unwrap().prd_content.is_none());
    }

    #[test]
    fn adapt_results_scaffold_defaults_to_none() {
        let results = AdaptResults::default();
        assert!(results.scaffold.is_none());
    }

    #[test]
    fn adapt_results_json_without_scaffold_field_deserializes_as_none() {
        let raw = r#"{"profile":null,"report":null,"plan":null,"materialize":null}"#;
        let result: Result<AdaptResults, _> = serde_json::from_str(raw);
        assert!(result.is_ok());
        assert!(result.unwrap().scaffold.is_none());
    }

    // resume_step tests are in mod.rs tests (they need make_mock_profile helper)
}
