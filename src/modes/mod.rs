#![allow(dead_code)]
use crate::config::{Config, ModeConfig};

/// Built-in mode definitions. These are available even without config.
pub fn builtin_modes() -> Vec<(&'static str, ModeConfig)> {
    vec![
        (
            "orchestrator",
            ModeConfig {
                system_prompt: String::new(),
                allowed_tools: Vec::new(),
                permission_mode: None,
            },
        ),
        (
            "vibe",
            ModeConfig {
                system_prompt: String::new(),
                allowed_tools: Vec::new(),
                permission_mode: None,
            },
        ),
        (
            "review",
            ModeConfig {
                system_prompt: "You are a code reviewer. Review the PR and leave comments. \
                    Focus on correctness, security, performance, and code quality."
                    .into(),
                allowed_tools: vec!["Read".into(), "Grep".into(), "Glob".into(), "Bash".into()],
                permission_mode: Some("plan".into()),
            },
        ),
    ]
}

/// Resolve a mode name to its configuration.
/// Priority: config-defined modes > built-in modes.
pub fn resolve_mode(name: &str, config: Option<&Config>) -> Option<ModeConfig> {
    // Check config-defined modes first
    if let Some(cfg) = config
        && let Some(mode) = cfg.modes.get(name)
    {
        return Some(mode.clone());
    }

    // Fall back to built-in modes
    builtin_modes()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, m)| m)
}

/// Extract mode name from issue labels. Looks for `maestro:mode:<name>` labels.
pub fn mode_from_labels(labels: &[String]) -> Option<String> {
    labels
        .iter()
        .find_map(|l| l.strip_prefix("maestro:mode:").map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_modes_has_three_defaults() {
        let modes = builtin_modes();
        assert_eq!(modes.len(), 3);
        let names: Vec<&str> = modes.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"orchestrator"));
        assert!(names.contains(&"vibe"));
        assert!(names.contains(&"review"));
    }

    #[test]
    fn resolve_mode_finds_builtin() {
        let mode = resolve_mode("review", None);
        assert!(mode.is_some());
        let mode = mode.unwrap();
        assert!(!mode.system_prompt.is_empty());
        assert!(mode.allowed_tools.contains(&"Read".to_string()));
    }

    #[test]
    fn resolve_mode_returns_none_for_unknown() {
        assert!(resolve_mode("nonexistent", None).is_none());
    }

    #[test]
    fn mode_from_labels_extracts_mode() {
        let labels = vec![
            "bug".into(),
            "maestro:mode:review".into(),
            "priority:P0".into(),
        ];
        assert_eq!(mode_from_labels(&labels), Some("review".into()));
    }

    #[test]
    fn mode_from_labels_returns_none_when_absent() {
        let labels = vec!["bug".into(), "priority:P1".into()];
        assert_eq!(mode_from_labels(&labels), None);
    }
}
