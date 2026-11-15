use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub repo: String,
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
    /// Primary detected language id ("rust", "node", "python", "go").
    /// Populated by `maestro init` auto-detection (#505).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// All detected language ids when the project is polyglot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,
    /// Stack-appropriate build command (e.g. "cargo build", "npm run build").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_command: Option<String>,
    /// Stack-appropriate test command (e.g. "cargo test", "npm test").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_command: Option<String>,
    /// Stack-appropriate run command (e.g. "cargo run", "npm start").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_command: Option<String>,
}

fn default_base_branch() -> String {
    "main".into()
}
