use super::app::App;

pub(super) fn print_summary(app: &App) {
    let summary = app.build_completion_summary();
    if summary.sessions.is_empty() {
        return;
    }

    let all_sessions = app.pool.all_sessions();

    println!();
    println!("=== Maestro Session Summary ===");
    println!();

    for (sl, session) in summary.sessions.iter().zip(all_sessions.iter()) {
        println!(
            "  {} {} {} ${:.2} {}",
            sl.status.symbol(),
            sl.label,
            sl.status.label(),
            sl.cost_usd,
            sl.elapsed,
        );

        if session.is_hollow_completion {
            println!(
                "    \u{26A0} Hollow completion: session completed without performing any work"
            );
        }
        if !session.last_message.is_empty() {
            println!("    Last: {}", session.last_message);
        }
        if !session.files_touched.is_empty() {
            println!("    Files: {}", session.files_touched.join(", "));
        }
        if sl.status == crate::session::types::SessionStatus::Errored {
            for entry in session
                .activity_log
                .iter()
                .rev()
                .take(3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                println!("    > {}", entry.message);
            }
        }
    }

    println!();
    println!("Total cost: ${:.2}", summary.total_cost_usd);
    println!();
}
