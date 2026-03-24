use std::collections::HashMap;

use crate::github::types::GhIssue;

/// Routes issues to specific models based on label-matching rules.
pub struct ModelRouter {
    /// Ordered rules: pattern -> model name. First match wins.
    rules: Vec<(String, String)>,
    default_model: String,
}

impl ModelRouter {
    pub fn new(rules: HashMap<String, String>, default_model: String) -> Self {
        // Convert to ordered vec sorted by key for deterministic matching
        let mut rules: Vec<(String, String)> = rules.into_iter().collect();
        rules.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            rules,
            default_model,
        }
    }

    /// Resolve which model to use for a given issue.
    /// Checks issue labels against routing rules. First match wins.
    /// Rules are patterns like "priority:P0" or "type:docs".
    pub fn resolve(&self, issue: &GhIssue) -> &str {
        for (pattern, model) in &self.rules {
            if issue.labels.iter().any(|l| l == pattern) {
                return model;
            }
        }
        &self.default_model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_issue(labels: &[&str]) -> GhIssue {
        GhIssue {
            number: 1,
            title: "Test".into(),
            body: String::new(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            state: "open".into(),
            html_url: String::new(),
        }
    }

    #[test]
    fn resolve_returns_default_when_no_rules() {
        let router = ModelRouter::new(HashMap::new(), "sonnet".into());
        let issue = make_issue(&["priority:P0"]);
        assert_eq!(router.resolve(&issue), "sonnet");
    }

    #[test]
    fn resolve_matches_priority_label() {
        let mut rules = HashMap::new();
        rules.insert("priority:P0".into(), "opus".into());
        let router = ModelRouter::new(rules, "sonnet".into());

        let issue = make_issue(&["priority:P0", "maestro:ready"]);
        assert_eq!(router.resolve(&issue), "opus");
    }

    #[test]
    fn resolve_returns_default_when_no_match() {
        let mut rules = HashMap::new();
        rules.insert("priority:P0".into(), "opus".into());
        let router = ModelRouter::new(rules, "sonnet".into());

        let issue = make_issue(&["priority:P2"]);
        assert_eq!(router.resolve(&issue), "sonnet");
    }

    #[test]
    fn resolve_first_matching_rule_wins() {
        let mut rules = HashMap::new();
        rules.insert("priority:P0".into(), "opus".into());
        rules.insert("type:docs".into(), "haiku".into());
        let router = ModelRouter::new(rules, "sonnet".into());

        // Issue has both labels — sorted rules mean "priority:P0" comes first
        let issue = make_issue(&["type:docs", "priority:P0"]);
        assert_eq!(router.resolve(&issue), "opus");
    }

    #[test]
    fn resolve_type_docs_routes_to_configured_model() {
        let mut rules = HashMap::new();
        rules.insert("type:docs".into(), "haiku".into());
        let router = ModelRouter::new(rules, "sonnet".into());

        let issue = make_issue(&["type:docs"]);
        assert_eq!(router.resolve(&issue), "haiku");
    }

    #[test]
    fn resolve_with_empty_labels() {
        let mut rules = HashMap::new();
        rules.insert("priority:P0".into(), "opus".into());
        let router = ModelRouter::new(rules, "sonnet".into());

        let issue = make_issue(&[]);
        assert_eq!(router.resolve(&issue), "sonnet");
    }
}
