//! Header status chips for the PRD screen (#321) — extracted from
//! `draw.rs` to keep that file under the file-size cap. Pure helpers
//! returning styled spans for the sync + save state indicators.

#![deny(clippy::unwrap_used)]

use crate::tui::screens::prd::state::{PrdSaveStatus, PrdSyncStatus};
use crate::tui::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

pub fn sync_chip(status: &PrdSyncStatus, theme: &Theme) -> Option<Span<'static>> {
    match status {
        PrdSyncStatus::Idle => None,
        PrdSyncStatus::Syncing { started_at } => {
            let elapsed = started_at.elapsed().as_secs();
            Some(Span::styled(
                format!("⟳ Syncing… ({elapsed}s)"),
                Style::default()
                    .fg(theme.accent_info)
                    .add_modifier(Modifier::BOLD),
            ))
        }
        PrdSyncStatus::SyncedAt(at) => {
            let secs = at.elapsed().as_secs();
            Some(Span::styled(
                format!("✓ Synced {secs}s ago"),
                Style::default().fg(theme.accent_success),
            ))
        }
        PrdSyncStatus::Failed { at, message } => {
            // Truncate long error messages so the header line stays readable.
            let short = message.chars().take(60).collect::<String>();
            let secs = at.elapsed().as_secs();
            Some(Span::styled(
                format!("⚠ Sync failed {secs}s ago: {short}"),
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ))
        }
    }
}

pub fn save_chip(save_status: &PrdSaveStatus, dirty: bool, theme: &Theme) -> Option<Span<'static>> {
    if let Some(ref err) = save_status.last_error {
        let short = err.chars().take(50).collect::<String>();
        return Some(Span::styled(
            format!("⚠ Save failed: {short}"),
            Style::default().fg(theme.accent_error),
        ));
    }
    if dirty {
        return Some(Span::styled(
            "● unsaved (press [s])".to_string(),
            Style::default().fg(theme.accent_warning),
        ));
    }
    if let Some(at) = save_status.last_saved {
        let secs = at.elapsed().as_secs();
        if secs <= 10 {
            return Some(Span::styled(
                format!("✓ Saved {secs}s ago"),
                Style::default().fg(theme.accent_success),
            ));
        }
    }
    None
}
