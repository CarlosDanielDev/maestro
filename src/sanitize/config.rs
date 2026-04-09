use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::Severity;

/// Output format for sanitize reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Text,
    Json,
    Markdown,
}

/// Configuration for the sanitize command, constructed from CLI args.
#[derive(Debug, Clone)]
pub struct SanitizeConfig {
    pub path: PathBuf,
    pub output: OutputFormat,
    pub severity: Severity,
    pub skip_ai: bool,
    pub model: Option<String>,
}

impl Default for SanitizeConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("."),
            output: OutputFormat::Text,
            severity: Severity::Info,
            skip_ai: false,
            model: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_current_dir() {
        assert_eq!(SanitizeConfig::default().path, PathBuf::from("."));
    }

    #[test]
    fn default_config_output_is_text() {
        assert_eq!(SanitizeConfig::default().output, OutputFormat::Text);
    }

    #[test]
    fn default_config_severity_is_info() {
        assert_eq!(SanitizeConfig::default().severity, Severity::Info);
    }

    #[test]
    fn default_config_skip_ai_is_false() {
        assert!(!SanitizeConfig::default().skip_ai);
    }

    #[test]
    fn default_config_model_is_none() {
        assert!(SanitizeConfig::default().model.is_none());
    }

    #[test]
    fn output_format_all_variants_exist() {
        // Verify all three variants are usable
        let variants = [OutputFormat::Text, OutputFormat::Json, OutputFormat::Markdown];
        assert_eq!(variants.len(), 3);
    }
}
