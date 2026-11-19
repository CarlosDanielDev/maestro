use std::fmt::Write;

use super::app::App;
use crate::session::transition::TransitionReason::UserQuit;
use crate::tui::icons::{self, IconId};

pub(super) fn print_summary(app: &App) {
    print!("{}", exit_summary_text(app));
}

/// The text `print_summary` writes after the TUI closes; empty when the
/// run had no sessions.
fn exit_summary_text(app: &App) -> String {
    let mut out = String::new();
    let summary = app.build_completion_summary();
    if summary.sessions.is_empty() {
        return out;
    }

    let all_sessions = app.pool.all_sessions();

    let _ = writeln!(out);
    let _ = writeln!(out, "=== Maestro Session Summary ===");
    let _ = writeln!(out);

    for (sl, session) in summary.sessions.iter().zip(all_sessions.iter()) {
        let _ = writeln!(
            out,
            "  {} {} {} ${:.2} {}",
            sl.status.symbol(),
            sl.label,
            sl.status.label(),
            sl.cost_usd,
            sl.elapsed,
        );

        if session.is_hollow_completion {
            let _ = writeln!(
                out,
                "    {} Hollow completion: session completed without performing any work",
                icons::get(IconId::Warning)
            );
        }
        if !session.last_message.is_empty() {
            let _ = writeln!(out, "    Last: {}", session.last_message);
        }
        if !session.files_touched.is_empty() {
            let _ = writeln!(out, "    Files: {}", session.files_touched.join(", "));
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
                let _ = writeln!(out, "    > {}", entry.message);
            }
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "Total cost: ${:.2}", summary.total_cost_usd);
    let paused = all_sessions
        .iter()
        .filter(|s| s.status == crate::session::types::SessionStatus::Paused)
        .count();
    if paused > 0 {
        let _ = writeln!(out, "{paused} paused · resume with maestro resume");
    }
    // Known gap (#1266): these statuses cannot pause, so quit killed them.
    let killed: Vec<String> = (all_sessions.iter())
        .filter(|s| s.status == crate::session::types::SessionStatus::Killed)
        .filter(|s| (s.transition_history.last()).is_some_and(|t| t.reason == UserQuit))
        .map(|s| super::app::helpers::session_label(s))
        .collect();
    if !killed.is_empty() {
        let _ = writeln!(out, "killed at quit (cannot pause): {}", killed.join(", "));
    }
    let _ = writeln!(out);
    out
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
