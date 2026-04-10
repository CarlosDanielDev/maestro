use crate::state::store::StateStore;

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

    println!("=== Maestro Spending Report ===");
    println!("Total: ${:.2}", state.total_cost_usd);
    println!();

    for session in &state.sessions {
        let label = match session.issue_number {
            Some(n) => format!("#{:<6}", n),
            None => session.id.to_string()[..8].to_string(),
        };
        println!(
            "  {} ${:.2} ({})",
            label,
            session.cost_usd,
            session.status.label(),
        );
    }

    Ok(())
}
