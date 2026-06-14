//! Per-agent provider map construction, shared by the TUI session pool and
//! the headless team-launch path (#897 mechanism, #1000 production wiring).
//!
//! Single source of truth so both `App::configure`/`apply_agents_config` and
//! `ProductionSchedulerRunner` build the same `agent_id → provider` map that
//! `AgentProviderFactory::provider_for_agent_id` consults for L1 per-role
//! routing. Lives under `commands` (not `tui`) so non-TUI callers can use it.

use std::collections::HashMap;
use std::sync::Arc;

use crate::agent_provider::AgentProvider;
use crate::config::Config;

/// Enabled agent ids from config. Falls back to a single synthetic `"claude"`
/// id when no agents are configured, matching the legacy single-provider
/// default.
pub(crate) fn enabled_agent_ids(config: &Config) -> Vec<String> {
    if config.agents.entries.is_empty() {
        return vec!["claude".to_string()];
    }
    config
        .agents
        .entries
        .iter()
        .filter(|(_, agent)| agent.enabled)
        .map(|(id, _)| id.clone())
        .collect()
}

/// Build the `agent_id → provider` map for L1 per-role routing. Each enabled
/// agent id resolves through `provider_for_agent`; ids that fail to resolve
/// are skipped — at dispatch time they fall back to the factory default via
/// `AgentProviderFactory::provider_for_agent_id` (the #897 edge-case contract).
pub(crate) fn build_agent_provider_map(config: &Config) -> HashMap<String, Arc<dyn AgentProvider>> {
    let mut providers = HashMap::new();
    for id in enabled_agent_ids(config) {
        match config
            .resolve_agent(Some(&id))
            .and_then(|resolved| crate::commands::run::provider_for_agent(&resolved))
        {
            Ok(provider) => {
                providers.insert(id, provider);
            }
            // Skip-on-error is intentional (the #897 fallback contract), but
            // warn so a headless run does not silently route a misconfigured
            // role to the default provider with no trace.
            Err(error) => tracing::warn!(
                agent_id = %id,
                %error,
                "skipping agent in provider map; role will fall back to factory default"
            ),
        }
    }
    providers
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn load_config(toml: &str) -> Config {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        write!(file, "{toml}").expect("write toml");
        Config::load(file.path()).expect("load config")
    }

    const BASE: &str = "[project]\nrepo = \"owner/repo\"\n\
        [sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
        [github]\n[notifications]\n";

    #[test]
    fn enabled_agent_ids_defaults_to_claude_when_entries_empty() {
        let cfg = load_config(BASE);
        assert_eq!(enabled_agent_ids(&cfg), vec!["claude".to_string()]);
    }

    #[test]
    fn enabled_agent_ids_excludes_disabled_entries() {
        let cfg = load_config(&format!(
            "{BASE}[agents]\ndefault = \"claude\"\n\
            [agents.claude]\nkind = \"claude\"\ncommand = \"claude\"\n\
            [agents.codex]\nkind = \"codex\"\ncommand = \"codex\"\nenabled = false\n"
        ));
        assert_eq!(enabled_agent_ids(&cfg), vec!["claude".to_string()]);
    }

    /// Binding RED gate for issue #1000: the headless launch path can build
    /// the per-role provider map without a TUI dependency.
    #[test]
    fn build_agent_provider_map_returns_entry_per_enabled_agent() {
        let cfg = load_config(&format!(
            "{BASE}[agents]\ndefault = \"claude\"\n\
            [agents.claude]\nkind = \"claude\"\ncommand = \"claude\"\n\
            [agents.codex]\nkind = \"codex\"\ncommand = \"codex\"\n"
        ));
        // Skip-on-error (provider construction fails after a valid resolve) is
        // covered by
        // agent_provider::types_tests::provider_for_agent_id_unknown_id_returns_default.
        let map = build_agent_provider_map(&cfg);
        assert_eq!(map.len(), 2, "one entry per enabled agent");
        assert!(map.contains_key("claude"), "map keyed by agent id");
        assert!(map.contains_key("codex"), "map keyed by agent id");
    }

    #[test]
    fn build_agent_provider_map_skips_disabled_agents() {
        let cfg = load_config(&format!(
            "{BASE}[agents]\ndefault = \"claude\"\n\
            [agents.claude]\nkind = \"claude\"\ncommand = \"claude\"\n\
            [agents.codex]\nkind = \"codex\"\ncommand = \"codex\"\nenabled = false\n"
        ));
        let map = build_agent_provider_map(&cfg);
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("claude"));
        assert!(!map.contains_key("codex"));
    }
}
