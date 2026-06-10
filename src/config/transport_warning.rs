//! Pre-cutoff warning for the headless Claude transport (#750).
//!
//! Anthropic withdraws subscription billing from headless `claude --print`
//! on **2026-06-15**. From 2026-05-15 (one month out), maestro warns at
//! startup when any enabled claude agent still runs the headless transport,
//! so Pro/Max subscription users flip `transport = "interactive"` in time.
//! Suppressible via `MAESTRO_SILENCE_TRANSPORT_WARN=1` (read by the caller —
//! this module stays env-free so tests run in parallel safely).

use super::agents::{AgentKind, AgentsConfig};

/// 2026-05-15T00:00:00Z — the date the warning starts firing.
pub const TRANSPORT_WARN_EPOCH: u64 = 1_778_803_200;

/// Clock seam so tests can pin the date (RUST-GUARDRAILS.md §7).
pub trait Clock {
    fn epoch_secs(&self) -> u64;
}

/// Production clock backed by `SystemTime`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn epoch_secs(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Returns the warning message when it should fire, `None` otherwise.
///
/// Fires when ALL of:
/// - not silenced,
/// - the clock is at/after [`TRANSPORT_WARN_EPOCH`],
/// - at least one enabled claude-kind agent has no transport or `"headless"`.
pub fn headless_cutoff_warning(
    agents: &AgentsConfig,
    clock: &dyn Clock,
    silenced: bool,
) -> Option<String> {
    if silenced || clock.epoch_secs() < TRANSPORT_WARN_EPOCH {
        return None;
    }

    let headless_ids: Vec<&str> = agents
        .entries
        .iter()
        .filter(|(_, agent)| agent.enabled && agent.kind == AgentKind::Claude)
        .filter(|(_, agent)| {
            agent
                .transport
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .unwrap_or("headless")
                == "headless"
        })
        .map(|(id, _)| id.as_str())
        .collect();

    if headless_ids.is_empty() {
        return None;
    }

    Some(format!(
        "headless claude transport ({}): subscription billing ends 2026-06-15 — consider transport=\"interactive\" in maestro.toml (silence with MAESTRO_SILENCE_TRANSPORT_WARN=1)",
        headless_ids.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::super::agents::AgentConfig;
    use super::*;

    struct MockClock(u64);

    impl Clock for MockClock {
        fn epoch_secs(&self) -> u64 {
            self.0
        }
    }

    const BEFORE: MockClock = MockClock(TRANSPORT_WARN_EPOCH - 1);
    const AFTER: MockClock = MockClock(TRANSPORT_WARN_EPOCH + 1);

    fn agents_with(transport: Option<&str>) -> AgentsConfig {
        let mut claude = AgentConfig::builtin_claude("opus", "default", Vec::new());
        claude.transport = transport.map(str::to_string);
        let mut agents = AgentsConfig {
            default: "claude".to_string(),
            entries: Default::default(),
        };
        agents.entries.insert("claude".to_string(), claude);
        agents
    }

    #[test]
    fn fires_after_warn_date_for_headless_default() {
        let msg = headless_cutoff_warning(&agents_with(None), &AFTER, false);
        let msg = msg.expect("warning must fire for default transport after warn date");
        assert!(msg.contains("2026-06-15"));
        assert!(msg.contains("claude"), "must name the agent id: {msg}");
        assert!(msg.contains("MAESTRO_SILENCE_TRANSPORT_WARN"));
    }

    #[test]
    fn fires_for_explicit_headless() {
        assert!(headless_cutoff_warning(&agents_with(Some("headless")), &AFTER, false).is_some());
    }

    #[test]
    fn silent_before_warn_date() {
        assert!(headless_cutoff_warning(&agents_with(None), &BEFORE, false).is_none());
    }

    #[test]
    fn silent_when_interactive() {
        assert!(
            headless_cutoff_warning(&agents_with(Some("interactive")), &AFTER, false).is_none()
        );
    }

    #[test]
    fn silent_when_suppressed() {
        assert!(headless_cutoff_warning(&agents_with(None), &AFTER, true).is_none());
    }

    #[test]
    fn silent_when_claude_agent_disabled() {
        let mut agents = agents_with(None);
        if let Some(agent) = agents.entries.get_mut("claude") {
            agent.enabled = false;
        }
        assert!(headless_cutoff_warning(&agents, &AFTER, false).is_none());
    }

    #[test]
    fn silent_for_non_claude_agents() {
        let mut agents = agents_with(None);
        agents.entries.clear();
        let mut qwen = AgentConfig::builtin_claude("m", "default", Vec::new());
        qwen.kind = AgentKind::Qwen;
        qwen.command = Some("qwen".to_string());
        agents.entries.insert("qwen".to_string(), qwen);
        assert!(headless_cutoff_warning(&agents, &AFTER, false).is_none());
    }
}
