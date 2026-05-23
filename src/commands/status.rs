use crate::state::store::StateStore;
use crate::state::types::MaestroState;
use std::collections::BTreeMap;

/// Build the `cargo run -- cost` report text. Pure formatter over
/// `MaestroState` so tests can assert output without touching disk.
/// Adds a `=== Per-provider breakdown ===` section grouped by
/// `Session.agent_id` (falling back to `"unknown"`) to satisfy
/// issue #769 AC #4 — closes the v0.29.5 observability umbrella.
pub fn format_cost_report(state: &MaestroState) -> String {
    let mut out = String::new();
    out.push_str("=== Maestro Spending Report ===\n");
    out.push_str(&format!("Total: ${:.2}\n\n", state.total_cost_usd));

    if state.sessions.is_empty() {
        return out;
    }

    let mut by_provider: BTreeMap<String, (f64, u32)> = BTreeMap::new();
    for s in &state.sessions {
        let key = s.agent_id.clone().unwrap_or_else(|| "unknown".to_string());
        let entry = by_provider.entry(key).or_insert((0.0, 0));
        entry.0 += s.cost_usd;
        entry.1 += 1;
    }

    out.push_str("=== Per-provider breakdown ===\n");
    for (provider, (cost, count)) in &by_provider {
        let session_word = if *count == 1 { "session" } else { "sessions" };
        let free_marker = if cost.abs() < f64::EPSILON {
            " (free)"
        } else {
            ""
        };
        out.push_str(&format!(
            "  {:<10} ${:.2} ({} {}{})\n",
            provider, cost, count, session_word, free_marker,
        ));
    }
    out.push('\n');

    for session in &state.sessions {
        let label = match session.issue_number {
            Some(n) => format!("#{:<6}", n),
            None => session.id.to_string()[..8].to_string(),
        };
        out.push_str(&format!(
            "  {} ${:.2} ({})\n",
            label,
            session.cost_usd,
            session.status.label(),
        ));
    }

    out
}

pub fn cmd_status() -> anyhow::Result<()> {
    let store = StateStore::new(StateStore::default_path());
    let state = store.load()?;

    if state.sessions.is_empty() {
        println!("No sessions recorded.");
        return Ok(());
    }

    println!(
        "Sessions: {} total, {} active",
        state.sessions.len(),
        state.active_sessions().len()
    );
    println!("Total cost: ${:.2}", state.total_cost_usd);
    println!();

    for session in &state.sessions {
        let label = match session.issue_number {
            Some(n) => format!("#{}", n),
            None => session.id.to_string()[..8].to_string(),
        };
        println!(
            "  {} {} {} ${:.2} {}",
            session.status.symbol(),
            label,
            session.status.label(),
            session.cost_usd,
            session.elapsed_display(),
        );
    }

    Ok(())
}

pub fn cmd_cost() -> anyhow::Result<()> {
    let store = StateStore::new(StateStore::default_path());
    let state = store.load()?;
    print!("{}", format_cost_report(&state));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{Session, SessionStatus};
    use crate::state::types::MaestroState;

    fn make_cost_session(agent_id: Option<&str>, issue_number: Option<u64>, cost: f64) -> Session {
        let mut s = Session::new(
            "test".into(),
            "model".into(),
            "orchestrator".into(),
            issue_number,
            None,
        );
        s.agent_id = agent_id.map(str::to_string);
        s.cost_usd = cost;
        s.status = SessionStatus::Completed;
        s
    }

    #[test]
    fn format_cost_report_empty_state_returns_header_only() {
        let state = MaestroState::default();
        let report = format_cost_report(&state);
        assert!(report.contains("=== Maestro Spending Report ==="));
        assert!(report.contains("Total: $0.00"));
        assert!(
            !report.contains("Per-provider breakdown"),
            "empty state must not include rollup section"
        );
    }

    #[test]
    fn format_cost_report_three_sessions_two_providers() {
        let mut state = MaestroState::default();
        state.sessions = vec![
            make_cost_session(Some("claude"), Some(301), 0.15),
            make_cost_session(Some("claude"), Some(302), 0.08),
            make_cost_session(Some("minimax"), Some(303), 0.00),
        ];
        state.update_total_cost();
        let report = format_cost_report(&state);

        assert!(report.contains("=== Per-provider breakdown ==="));
        assert!(report.contains("claude"));
        assert!(report.contains("minimax"));

        let claude_pos = report.find("claude").expect("claude row present");
        let minimax_pos = report.find("minimax").expect("minimax row present");
        assert!(
            claude_pos < minimax_pos,
            "BTreeMap ordering: claude before minimax"
        );
    }

    #[test]
    fn format_cost_report_ollama_zero_cost_shows_free_marker() {
        let mut state = MaestroState::default();
        state.sessions = vec![make_cost_session(Some("ollama"), Some(400), 0.0)];
        state.update_total_cost();
        let report = format_cost_report(&state);
        assert!(report.contains("ollama"));
        assert!(
            report.contains("(free)"),
            "zero-cost provider must carry (free) marker; got:\n{}",
            report
        );
    }

    #[test]
    fn format_cost_report_minimax_zero_cost_shows_free_marker() {
        let mut state = MaestroState::default();
        state.sessions = vec![make_cost_session(Some("minimax"), Some(500), 0.0)];
        state.update_total_cost();
        let report = format_cost_report(&state);
        assert!(report.contains("minimax"));
        assert!(report.contains("(free)"));
    }

    #[test]
    fn format_cost_report_session_with_no_agent_id_groups_under_unknown() {
        let mut state = MaestroState::default();
        state.sessions = vec![make_cost_session(None, Some(42), 0.05)];
        state.update_total_cost();
        let report = format_cost_report(&state);
        assert!(report.contains("unknown"));
        assert!(
            !report.contains("(free)"),
            "non-zero cost must NOT show (free) marker; got:\n{}",
            report
        );
    }
}
