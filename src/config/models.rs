use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelsConfig {
    /// Routing rules: label pattern -> model name. First match wins.
    /// Example: { "priority:P0" = "opus", "type:docs" = "haiku" }
    #[serde(default)]
    pub routing: std::collections::HashMap<String, String>,
}
