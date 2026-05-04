use serde::{Deserialize, Serialize};

pub use crate::turboquant::types::{ApplyTarget, QuantStrategy};

/// TurboQuant quantization configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurboQuantConfig {
    /// Whether TurboQuant quantization is active.
    #[serde(default)]
    pub enabled: bool,
    /// Bit width for quantization (1-8).
    #[serde(default = "default_turbo_bit_width")]
    pub bit_width: u8,
    /// Quantization strategy.
    #[serde(default)]
    pub strategy: QuantStrategy,
    /// Which components to compress.
    #[serde(default)]
    pub apply_to: ApplyTarget,
    /// Automatically enable on context overflow events.
    #[serde(default)]
    pub auto_on_overflow: bool,
    /// Token budget for fork-handoff compression.
    #[serde(default = "default_fork_handoff_budget")]
    pub fork_handoff_budget: usize,
    /// Token budget for system-prompt compaction.
    #[serde(default = "default_system_prompt_budget")]
    pub system_prompt_budget: usize,
    /// Token budget for knowledge-base compression.
    #[serde(default = "default_knowledge_budget")]
    pub knowledge_budget: usize,
}

fn default_turbo_bit_width() -> u8 {
    4
}

fn default_fork_handoff_budget() -> usize {
    4096
}

fn default_system_prompt_budget() -> usize {
    2048
}

fn default_knowledge_budget() -> usize {
    4096
}

impl Default for TurboQuantConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bit_width: default_turbo_bit_width(),
            strategy: QuantStrategy::default(),
            apply_to: ApplyTarget::default(),
            auto_on_overflow: false,
            fork_handoff_budget: default_fork_handoff_budget(),
            system_prompt_budget: default_system_prompt_budget(),
            knowledge_budget: default_knowledge_budget(),
        }
    }
}
