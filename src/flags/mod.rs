//! Feature flag definitions and runtime store.
//!
//! Flags are **user-facing boolean toggles** that gate experimental or optional
//! features (e.g. `CiAutoFix`, `ReviewCouncil`, `TurboQuant`). They are resolved
//! once at startup from three layers (compiled defaults → `maestro.toml` → CLI
//! arguments) and live in memory for the duration of the process.
//!
//! This is intentionally separate from [`crate::state`], which handles **persistent
//! session data** (JSON state file, progress, file claims, prompt history) that is
//! written to disk and survives across runs. Flags are ephemeral configuration;
//! state is durable runtime data.

pub mod store;

use serde::{Deserialize, Serialize};

/// Where a flag's current value was resolved from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagSource {
    /// Compiled-in default value.
    Default,
    /// Overridden by maestro.toml [flags] section.
    Config,
    /// Overridden by CLI --enable-flag / --disable-flag.
    Cli,
}

/// All feature flags known to maestro.
///
/// Adding a new flag requires three changes:
/// 1. Add the variant here
/// 2. Add a match arm in `default_enabled()` and `description()`
/// 3. Add the variant to `ALL_FLAGS`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flag {
    ContinuousMode,
    AutoFork,
    CiAutoFix,
    ReviewCouncil,
    ModelRouting,
    ContextOverflow,
    TurboQuant,
}

const ALL_FLAGS: &[Flag] = &[
    Flag::ContinuousMode,
    Flag::AutoFork,
    Flag::CiAutoFix,
    Flag::ReviewCouncil,
    Flag::ModelRouting,
    Flag::ContextOverflow,
    Flag::TurboQuant,
];

impl Flag {
    /// Conservative default: stable features on, experimental features off.
    pub fn default_enabled(self) -> bool {
        match self {
            Flag::ContinuousMode => true,
            Flag::AutoFork => true,
            Flag::CiAutoFix => false,
            Flag::ReviewCouncil => false,
            Flag::ModelRouting => false,
            Flag::ContextOverflow => false,
            Flag::TurboQuant => false,
        }
    }

    /// Snake_case name matching the config/CLI key.
    pub fn name(self) -> &'static str {
        match self {
            Flag::ContinuousMode => "continuous_mode",
            Flag::AutoFork => "auto_fork",
            Flag::CiAutoFix => "ci_auto_fix",
            Flag::ReviewCouncil => "review_council",
            Flag::ModelRouting => "model_routing",
            Flag::ContextOverflow => "context_overflow",
            Flag::TurboQuant => "turboquant",
        }
    }

    /// Human-readable explanation for TUI/help display.
    pub fn description(self) -> &'static str {
        match self {
            Flag::ContinuousMode => "Run sessions continuously until all issues are resolved",
            Flag::AutoFork => "Automatically fork sessions on context overflow",
            Flag::CiAutoFix => "Automatically create fix sessions for CI failures",
            Flag::ReviewCouncil => "Enable multi-model review council for code review",
            Flag::ModelRouting => "Route tasks to different models based on complexity",
            Flag::ContextOverflow => "Detect and handle context window overflow",
            Flag::TurboQuant => "Enable TurboQuant vector quantization for context compression",
        }
    }

    /// All known flag variants.
    pub fn all() -> &'static [Flag] {
        ALL_FLAGS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Flag::default_enabled --

    #[test]
    fn flag_default_enabled_continuous_mode_is_true() {
        assert!(Flag::ContinuousMode.default_enabled());
    }

    #[test]
    fn flag_default_enabled_auto_fork_is_true() {
        assert!(Flag::AutoFork.default_enabled());
    }

    #[test]
    fn flag_default_enabled_ci_auto_fix_is_false() {
        assert!(!Flag::CiAutoFix.default_enabled());
    }

    #[test]
    fn flag_default_enabled_review_council_is_false() {
        assert!(!Flag::ReviewCouncil.default_enabled());
    }

    #[test]
    fn flag_default_enabled_model_routing_is_false() {
        assert!(!Flag::ModelRouting.default_enabled());
    }

    #[test]
    fn flag_default_enabled_context_overflow_is_false() {
        assert!(!Flag::ContextOverflow.default_enabled());
    }

    #[test]
    fn flag_default_enabled_turboquant_is_false() {
        assert!(!Flag::TurboQuant.default_enabled());
    }

    // -- Flag::description --

    #[test]
    fn flag_description_returns_non_empty_str_for_each_variant() {
        for flag in Flag::all() {
            let desc = flag.description();
            assert!(
                !desc.is_empty(),
                "description() must not be empty for {:?}",
                flag
            );
        }
    }

    // -- Flag::all --

    #[test]
    fn flag_all_returns_exactly_seven_variants() {
        assert_eq!(Flag::all().len(), 7);
    }

    #[test]
    fn flag_all_contains_every_variant() {
        let all = Flag::all();
        assert!(all.contains(&Flag::ContinuousMode));
        assert!(all.contains(&Flag::AutoFork));
        assert!(all.contains(&Flag::CiAutoFix));
        assert!(all.contains(&Flag::ReviewCouncil));
        assert!(all.contains(&Flag::ModelRouting));
        assert!(all.contains(&Flag::ContextOverflow));
        assert!(all.contains(&Flag::TurboQuant));
    }

    // -- Flag::name --

    #[test]
    fn flag_name_returns_snake_case_for_each_variant() {
        assert_eq!(Flag::ContinuousMode.name(), "continuous_mode");
        assert_eq!(Flag::AutoFork.name(), "auto_fork");
        assert_eq!(Flag::CiAutoFix.name(), "ci_auto_fix");
        assert_eq!(Flag::ReviewCouncil.name(), "review_council");
        assert_eq!(Flag::ModelRouting.name(), "model_routing");
        assert_eq!(Flag::ContextOverflow.name(), "context_overflow");
    }

    #[test]
    fn flag_name_is_non_empty_for_all_variants() {
        for flag in Flag::all() {
            assert!(
                !flag.name().is_empty(),
                "name() must not be empty for {:?}",
                flag
            );
        }
    }

    // -- Serde round-trip --

    #[test]
    fn flag_serializes_continuous_mode_to_snake_case() {
        let json = serde_json::to_string(&Flag::ContinuousMode).unwrap();
        assert_eq!(json, r#""continuous_mode""#);
    }

    #[test]
    fn flag_deserializes_continuous_mode_from_snake_case() {
        let flag: Flag = serde_json::from_str(r#""continuous_mode""#).unwrap();
        assert_eq!(flag, Flag::ContinuousMode);
    }

    #[test]
    fn flag_serde_round_trip_all_variants() {
        for &flag in Flag::all() {
            let serialized = serde_json::to_string(&flag).unwrap();
            let deserialized: Flag = serde_json::from_str(&serialized).unwrap();
            assert_eq!(flag, deserialized, "round-trip failed for {:?}", flag);
        }
    }

    #[test]
    fn flag_serializes_ci_auto_fix_to_snake_case() {
        let json = serde_json::to_string(&Flag::CiAutoFix).unwrap();
        assert_eq!(json, r#""ci_auto_fix""#);
    }

    #[test]
    fn flag_serializes_review_council_to_snake_case() {
        let json = serde_json::to_string(&Flag::ReviewCouncil).unwrap();
        assert_eq!(json, r#""review_council""#);
    }
}
