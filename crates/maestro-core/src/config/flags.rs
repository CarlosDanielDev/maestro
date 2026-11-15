use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FlagsConfig {
    #[serde(flatten)]
    pub entries: HashMap<String, bool>,
}
