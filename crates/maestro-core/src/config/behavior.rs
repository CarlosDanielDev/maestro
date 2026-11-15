use super::Config;
use serde::{Deserialize, Serialize};

impl Config {
    /// Hard-coded-overridable launch-dialog defaults: `(produce_pr, interaction)`.
    ///
    /// Collapses the `config.behavior.launch.*` walk to a single call site,
    /// keeping Demeter happy for the TUI overlay.
    pub fn launch_defaults(&self) -> (bool, bool) {
        (
            self.behavior.launch.default_produce_pr,
            self.behavior.launch.default_interaction,
        )
    }
}

/// `[behavior]` section — non-security style and launch-flow defaults.
///
/// Keys here are user-facing UX toggles only. Anything that gates a security
/// control must live in code or a runtime hook, never in this section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BehaviorConfig {
    #[serde(default)]
    pub launch: LaunchBehaviorConfig,
}

impl BehaviorConfig {
    /// True when every field is at its default — lets `Config` skip emitting
    /// an empty `[behavior]` table on serialize.
    pub fn is_default(&self) -> bool {
        *self == BehaviorConfig::default()
    }
}

/// `[behavior.launch]` — default states for the Issue Launch dialog checkboxes.
///
/// Missing keys fall back to the hard-coded defaults: `Produce PR` on,
/// `Interaction` off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchBehaviorConfig {
    #[serde(default = "default_produce_pr")]
    pub default_produce_pr: bool,
    #[serde(default = "default_interaction")]
    pub default_interaction: bool,
}

impl Default for LaunchBehaviorConfig {
    fn default() -> Self {
        Self {
            default_produce_pr: default_produce_pr(),
            default_interaction: default_interaction(),
        }
    }
}

fn default_produce_pr() -> bool {
    true
}

fn default_interaction() -> bool {
    false
}
