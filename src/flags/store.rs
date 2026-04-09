use super::{Flag, FlagSource};
use std::collections::HashMap;

/// Runtime feature flag store.
///
/// Resolution priority: CLI disable > CLI enable > config file > compiled defaults.
#[derive(Debug, Clone, Default)]
pub struct FeatureFlags {
    overrides: HashMap<Flag, (bool, FlagSource)>,
}

impl FeatureFlags {
    /// Build a feature flags store from config and CLI overrides.
    ///
    /// `config_flags` maps snake_case flag names to bool values from maestro.toml.
    /// `cli_enable` and `cli_disable` are snake_case flag names from CLI args.
    /// Unknown flag names are silently ignored.
    pub fn new(
        config_flags: HashMap<String, bool>,
        cli_enable: Vec<String>,
        cli_disable: Vec<String>,
    ) -> Self {
        let mut overrides = HashMap::new();

        // Layer 1: config overrides
        for (name, value) in config_flags {
            if let Some(flag) = Self::parse_flag(&name) {
                overrides.insert(flag, (value, FlagSource::Config));
            }
        }

        // Layer 2: CLI enable (beats config)
        for name in cli_enable {
            if let Some(flag) = Self::parse_flag(&name) {
                overrides.insert(flag, (true, FlagSource::Cli));
            }
        }

        // Layer 3: CLI disable (beats CLI enable)
        for name in cli_disable {
            if let Some(flag) = Self::parse_flag(&name) {
                overrides.insert(flag, (false, FlagSource::Cli));
            }
        }

        Self { overrides }
    }

    /// Check if a flag is enabled. O(1) lookup.
    #[inline]
    pub fn is_enabled(&self, flag: Flag) -> bool {
        self.overrides
            .get(&flag)
            .map(|(val, _)| *val)
            .unwrap_or_else(|| flag.default_enabled())
    }

    /// Returns the source that determined the flag's current value.
    pub fn source(&self, flag: Flag) -> FlagSource {
        self.overrides
            .get(&flag)
            .map(|(_, src)| *src)
            .unwrap_or(FlagSource::Default)
    }

    /// All flags with their resolved state, for TUI display.
    pub fn all_with_state(&self) -> Vec<(Flag, bool)> {
        Flag::all()
            .iter()
            .map(|&f| (f, self.is_enabled(f)))
            .collect()
    }

    /// All flags with resolved state and source, for TUI display.
    pub fn all_with_source(&self) -> Vec<(Flag, bool, FlagSource)> {
        Flag::all()
            .iter()
            .map(|&f| (f, self.is_enabled(f), self.source(f)))
            .collect()
    }

    /// Parse a snake_case string into a Flag, returning None for unknown names.
    fn parse_flag(name: &str) -> Option<Flag> {
        match name {
            "continuous_mode" => Some(Flag::ContinuousMode),
            "auto_fork" => Some(Flag::AutoFork),
            "ci_auto_fix" => Some(Flag::CiAutoFix),
            "review_council" => Some(Flag::ReviewCouncil),
            "model_routing" => Some(Flag::ModelRouting),
            "context_overflow" => Some(Flag::ContextOverflow),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // -- FeatureFlags::default --

    #[test]
    fn feature_flags_default_uses_all_enum_defaults() {
        let flags = FeatureFlags::default();
        for &flag in Flag::all() {
            assert_eq!(
                flags.is_enabled(flag),
                flag.default_enabled(),
                "default for {:?} must match Flag::default_enabled()",
                flag
            );
        }
    }

    // -- FeatureFlags::new with empty inputs --

    #[test]
    fn feature_flags_new_empty_inputs_uses_all_defaults() {
        let flags = FeatureFlags::new(HashMap::new(), vec![], vec![]);
        for &flag in Flag::all() {
            assert_eq!(
                flags.is_enabled(flag),
                flag.default_enabled(),
                "empty inputs must fall back to default for {:?}",
                flag
            );
        }
    }

    // -- Config override beats default --

    #[test]
    fn feature_flags_config_override_beats_default_enable() {
        let mut config = HashMap::new();
        config.insert("ci_auto_fix".to_string(), true);
        let flags = FeatureFlags::new(config, vec![], vec![]);
        assert!(
            flags.is_enabled(Flag::CiAutoFix),
            "config true must override default false"
        );
    }

    #[test]
    fn feature_flags_config_override_beats_default_disable() {
        let mut config = HashMap::new();
        config.insert("continuous_mode".to_string(), false);
        let flags = FeatureFlags::new(config, vec![], vec![]);
        assert!(
            !flags.is_enabled(Flag::ContinuousMode),
            "config false must override default true"
        );
    }

    // -- CLI enable beats config --

    #[test]
    fn feature_flags_cli_enable_beats_config_disable() {
        let mut config = HashMap::new();
        config.insert("continuous_mode".to_string(), false);
        let flags = FeatureFlags::new(config, vec!["continuous_mode".to_string()], vec![]);
        assert!(
            flags.is_enabled(Flag::ContinuousMode),
            "CLI enable must beat config disable"
        );
    }

    #[test]
    fn feature_flags_cli_enable_activates_default_off_flag() {
        let flags = FeatureFlags::new(HashMap::new(), vec!["ci_auto_fix".to_string()], vec![]);
        assert!(
            flags.is_enabled(Flag::CiAutoFix),
            "CLI enable must activate a default-off flag"
        );
    }

    // -- CLI disable beats CLI enable --

    #[test]
    fn feature_flags_cli_disable_beats_cli_enable() {
        let flags = FeatureFlags::new(
            HashMap::new(),
            vec!["continuous_mode".to_string()],
            vec!["continuous_mode".to_string()],
        );
        assert!(
            !flags.is_enabled(Flag::ContinuousMode),
            "CLI disable must beat CLI enable"
        );
    }

    #[test]
    fn feature_flags_cli_disable_beats_config_enable() {
        let mut config = HashMap::new();
        config.insert("ci_auto_fix".to_string(), true);
        let flags = FeatureFlags::new(config, vec![], vec!["ci_auto_fix".to_string()]);
        assert!(
            !flags.is_enabled(Flag::CiAutoFix),
            "CLI disable must beat config enable"
        );
    }

    // -- Unknown flag strings --

    #[test]
    fn feature_flags_unknown_config_key_is_silently_ignored() {
        let mut config = HashMap::new();
        config.insert("no_such_flag_ever".to_string(), true);
        let flags = FeatureFlags::new(config, vec![], vec![]);
        assert_eq!(
            flags.is_enabled(Flag::ContinuousMode),
            Flag::ContinuousMode.default_enabled()
        );
    }

    #[test]
    fn feature_flags_unknown_cli_enable_key_is_silently_ignored() {
        let flags = FeatureFlags::new(HashMap::new(), vec!["totally_made_up".to_string()], vec![]);
        assert_eq!(
            flags.is_enabled(Flag::AutoFork),
            Flag::AutoFork.default_enabled()
        );
    }

    // -- all_with_state --

    #[test]
    fn feature_flags_all_with_state_returns_all_six_flags() {
        let flags = FeatureFlags::default();
        let state = flags.all_with_state();
        assert_eq!(state.len(), 6);
    }

    #[test]
    fn feature_flags_all_with_state_reflects_resolved_states() {
        let mut config = HashMap::new();
        config.insert("ci_auto_fix".to_string(), true);
        let flags = FeatureFlags::new(config, vec![], vec![]);
        let state = flags.all_with_state();

        let ci_entry = state
            .iter()
            .find(|(f, _)| *f == Flag::CiAutoFix)
            .expect("CiAutoFix must be present");
        assert!(
            ci_entry.1,
            "all_with_state must reflect config-enabled state"
        );
    }

    // -- source --

    #[test]
    fn feature_flags_source_default_when_no_override() {
        let flags = FeatureFlags::default();
        for &flag in Flag::all() {
            assert_eq!(
                flags.source(flag),
                FlagSource::Default,
                "source must be Default for {:?} with no overrides",
                flag
            );
        }
    }

    #[test]
    fn feature_flags_source_config_when_config_override() {
        let mut config = HashMap::new();
        config.insert("ci_auto_fix".to_string(), true);
        let flags = FeatureFlags::new(config, vec![], vec![]);
        assert_eq!(flags.source(Flag::CiAutoFix), FlagSource::Config);
    }

    #[test]
    fn feature_flags_source_cli_when_cli_enable() {
        let flags = FeatureFlags::new(HashMap::new(), vec!["ci_auto_fix".to_string()], vec![]);
        assert_eq!(flags.source(Flag::CiAutoFix), FlagSource::Cli);
    }

    #[test]
    fn feature_flags_source_cli_when_cli_disable() {
        let flags = FeatureFlags::new(HashMap::new(), vec![], vec!["continuous_mode".to_string()]);
        assert_eq!(flags.source(Flag::ContinuousMode), FlagSource::Cli);
    }

    #[test]
    fn feature_flags_source_cli_beats_config() {
        let mut config = HashMap::new();
        config.insert("ci_auto_fix".to_string(), false);
        let flags = FeatureFlags::new(config, vec!["ci_auto_fix".to_string()], vec![]);
        assert_eq!(flags.source(Flag::CiAutoFix), FlagSource::Cli);
    }

    // -- all_with_source --

    #[test]
    fn feature_flags_all_with_source_returns_all_six_flags() {
        let flags = FeatureFlags::default();
        assert_eq!(flags.all_with_source().len(), 6);
    }

    #[test]
    fn feature_flags_all_with_source_reflects_resolved_state_and_source() {
        let mut config = HashMap::new();
        config.insert("ci_auto_fix".to_string(), true);
        let flags = FeatureFlags::new(config, vec!["model_routing".to_string()], vec![]);
        let entries = flags.all_with_source();

        let ci = entries
            .iter()
            .find(|(f, _, _)| *f == Flag::CiAutoFix)
            .unwrap();
        assert!(ci.1); // enabled
        assert_eq!(ci.2, FlagSource::Config);

        let mr = entries
            .iter()
            .find(|(f, _, _)| *f == Flag::ModelRouting)
            .unwrap();
        assert!(mr.1); // enabled
        assert_eq!(mr.2, FlagSource::Cli);

        let af = entries
            .iter()
            .find(|(f, _, _)| *f == Flag::AutoFork)
            .unwrap();
        assert!(af.1); // default enabled
        assert_eq!(af.2, FlagSource::Default);
    }

    #[test]
    fn feature_flags_all_with_state_contains_each_variant_exactly_once() {
        let flags = FeatureFlags::default();
        let state = flags.all_with_state();
        for &flag in Flag::all() {
            let count = state.iter().filter(|(f, _)| *f == flag).count();
            assert_eq!(count, 1, "flag {:?} must appear exactly once", flag);
        }
    }
}
