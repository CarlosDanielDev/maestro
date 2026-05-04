use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewsConfig {
    /// Enable the experimental agent-graph render path.
    #[serde(default)]
    pub agent_graph_enabled: bool,
}
