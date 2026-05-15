//! Static provider registry for `sync-templates`.
//!
//! Each entry pairs a provider id with the `template_rules()` singleton it
//! would return. Sync-templates only ever consults `template_rules()`;
//! constructing full `AgentProvider` instances (some require URLs/models)
//! would be wasted work and would leak runtime configuration into a
//! read-only render command.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::templates::{
    TemplateProviderRules, null_rules,
    provider_rules::{claude_rules, codex_rules, http_generic_rules},
};

pub struct ProviderEntry {
    pub id: &'static str,
    pub rules: fn() -> &'static dyn TemplateProviderRules,
}

pub const PROVIDERS: &[ProviderEntry] = &[
    ProviderEntry {
        id: "claude",
        rules: claude_rules,
    },
    ProviderEntry {
        id: "codex",
        rules: codex_rules,
    },
    ProviderEntry {
        id: "opencode",
        // OpenCode does not override AgentProvider::template_rules() today,
        // so it inherits the fail-closed NullRules default. Once OpenCode
        // wires its own rules, swap this pointer.
        rules: null_rules,
    },
    ProviderEntry {
        id: "qwen",
        rules: http_generic_rules,
    },
    ProviderEntry {
        id: "ollama",
        rules: http_generic_rules,
    },
    ProviderEntry {
        id: "minimax",
        rules: http_generic_rules,
    },
];

pub const COMMANDS: &[&str] = &["implement", "pushup", "plan-feature", "simplify"];

pub fn entries_for(filter: Option<&str>) -> impl Iterator<Item = &'static ProviderEntry> {
    PROVIDERS
        .iter()
        .filter(move |e| filter.is_none_or(|f| f == e.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_const_contains_six_known_ids() {
        let ids: Vec<&str> = PROVIDERS.iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), 6, "registry must contain 6 providers: {ids:?}");
        for expected in ["claude", "codex", "opencode", "qwen", "ollama", "minimax"] {
            assert!(ids.contains(&expected), "missing provider id: {expected}");
        }
    }

    #[test]
    fn entries_for_with_none_filter_yields_all_providers() {
        let count = entries_for(None).count();
        assert_eq!(count, PROVIDERS.len());
    }

    #[test]
    fn entries_for_with_named_filter_yields_only_matching_provider() {
        let entries: Vec<_> = entries_for(Some("claude")).collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "claude");
    }

    #[test]
    fn entries_for_with_unknown_filter_yields_empty_iterator() {
        let entries: Vec<_> = entries_for(Some("fictional")).collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn commands_const_lists_four_canonical_commands() {
        assert_eq!(COMMANDS.len(), 4);
        for expected in ["implement", "pushup", "plan-feature", "simplify"] {
            assert!(COMMANDS.contains(&expected), "missing command: {expected}");
        }
    }

    #[test]
    fn opencode_uses_null_rules_pending_wiring() {
        let entry = PROVIDERS.iter().find(|e| e.id == "opencode").unwrap();
        let rules = (entry.rules)();
        assert!(
            rules.is_null(),
            "opencode rules must still be NullRules pending wiring"
        );
    }
}
