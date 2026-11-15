use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewsConfig {
    /// Enable the agent-graph render path. Defaults to `true` so a fresh
    /// install (or a startup-migration write failure) still gets the feature
    /// without the user having to opt in.
    #[serde(default = "default_agent_graph_enabled")]
    pub agent_graph_enabled: bool,
}

impl Default for ViewsConfig {
    fn default() -> Self {
        Self {
            agent_graph_enabled: default_agent_graph_enabled(),
        }
    }
}

fn default_agent_graph_enabled() -> bool {
    true
}
