//! Dispatch shim for the PRD screen (#321).
//!
//! The PRD screen needs simultaneous mutable access to the screen state
//! (cursor/edit buffer) and the App-owned `Prd` document. Bridging that
//! through the `Screen` trait would require restructuring; instead this
//! module wires the screen directly into App state and translates the
//! returned `PrdAction` into the existing `ScreenAction` vocabulary.

#![deny(clippy::unwrap_used)]

use crate::prd::export::to_markdown;
use crate::prd::model::Prd;
use crate::prd::store::{FilePrdStore, PrdStore};
use crate::tui::activity_log::LogLevel;
use crate::tui::app::App;
use crate::tui::screens::ScreenAction;
use crate::tui::screens::prd::{PrdAction, PrdScreen, PrdSyncStatus};
use crossterm::event::Event;
use std::time::Instant;

pub fn dispatch_input(app: &mut App, event: &Event) -> ScreenAction {
    let Event::Key(key) = event else {
        return ScreenAction::None;
    };

    // Lazily seed in-memory PRD if missing — load from disk if present.
    ensure_loaded(app);

    // Explore panel intercepts keys when open.
    if app.prd_show_explore {
        return handle_explore_key(app, key.code);
    }

    // Top-level keys that aren't part of the focused-section editor.
    match key.code {
        crossterm::event::KeyCode::Char('o') => {
            app.prd_show_explore = true;
            app.prd_explore_cursor = 0;
            return ScreenAction::None;
        }
        crossterm::event::KeyCode::Char('R') => {
            reset_prd(app);
            return ScreenAction::None;
        }
        _ => {}
    }

    let Some(prd) = app.prd.as_mut() else {
        return ScreenAction::None;
    };
    let Some(screen) = app.prd_screen.as_mut() else {
        return ScreenAction::None;
    };

    let action = crate::tui::screens::prd::input::handle_key(screen, prd, *key);
    drop_guard(action, app)
}

fn handle_explore_key(app: &mut App, code: crossterm::event::KeyCode) -> ScreenAction {
    use crossterm::event::KeyCode;
    match code {
        KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('q') => {
            app.prd_show_explore = false;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.prd_explore_cursor + 1 < app.prd_candidates.len() {
                app.prd_explore_cursor += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.prd_explore_cursor = app.prd_explore_cursor.saturating_sub(1);
        }
        KeyCode::Enter => {
            ingest_chosen_candidate(app);
            app.prd_show_explore = false;
        }
        _ => {}
    }
    ScreenAction::None
}

fn ingest_chosen_candidate(app: &mut App) {
    let Some(candidate) = app.prd_candidates.get(app.prd_explore_cursor).cloned() else {
        return;
    };
    let parsed = crate::prd::ingest::parse_markdown(&candidate.body);
    if parsed.is_empty() {
        app.activity_log.push_simple(
            "PRD".into(),
            format!(
                "Selected source [{}] had no parseable PRD content",
                candidate.source.label()
            ),
            LogLevel::Warn,
        );
        return;
    }
    let Some(prd) = app.prd.as_mut() else {
        return;
    };
    let report = prd.merge_ingested(&parsed);
    if let Some(s) = app.prd_screen.as_mut() {
        s.dirty = true;
    }
    let identifier = if candidate.number > 0 {
        format!("#{}", candidate.number)
    } else {
        candidate.title.clone()
    };
    let summary = crate::tui::app::data_handler::format_merge_summary(
        &report,
        &identifier,
        candidate.source.label(),
    )
    .unwrap_or_else(|| {
        format!(
            "no new content from [{}] {identifier}",
            candidate.source.label()
        )
    });
    app.activity_log
        .push_simple("PRD".into(), summary, LogLevel::Info);
    save_prd(app);
}

/// Wipe the local PRD and re-sync from sources. Triggered by `[R]`.
fn reset_prd(app: &mut App) {
    let store = FilePrdStore::new(repo_root());
    let _ = std::fs::remove_file(store.prd_path());
    app.prd = Some(Prd::new());
    if let Some(s) = app.prd_screen.as_mut() {
        s.dirty = false;
        s.sync_status = crate::tui::screens::prd::PrdSyncStatus::Idle;
        s.save_status = crate::tui::screens::prd::PrdSaveStatus::default();
    }
    app.activity_log.push_simple(
        "PRD".into(),
        "PRD reset — re-discovering sources from GitHub + local + Azure…".into(),
        LogLevel::Warn,
    );
    queue_sync(app);
}

/// Translate `PrdAction` → `ScreenAction` while performing side effects on
/// App state (save/sync/export). Pulled out so the dispatcher above stays
/// focused on borrow scoping.
fn drop_guard(action: PrdAction, app: &mut App) -> ScreenAction {
    match action {
        PrdAction::None => ScreenAction::None,
        PrdAction::Back => ScreenAction::Pop,
        PrdAction::Save => {
            save_prd(app);
            ScreenAction::None
        }
        PrdAction::Export => {
            export_prd(app);
            ScreenAction::None
        }
        PrdAction::Sync => {
            queue_sync(app);
            ScreenAction::None
        }
    }
}

pub fn ensure_loaded(app: &mut App) {
    if app.prd.is_some() && app.prd_screen.is_some() {
        return;
    }
    let store = FilePrdStore::new(repo_root());
    let (prd, was_seeded) = match store.load() {
        Ok(Some(p)) => (p, false),
        Ok(None) => (Prd::new(), true),
        Err(e) => {
            app.activity_log.push_simple(
                "PRD".into(),
                format!("Failed to load PRD: {e}"),
                LogLevel::Warn,
            );
            (Prd::new(), true)
        }
    };
    app.prd.get_or_insert(prd);
    app.prd_screen.get_or_insert_with(PrdScreen::new);
    // First-load on a fresh PRD: auto-sync from GitHub so Current State +
    // Timeline are populated immediately rather than blank. The user can
    // still press [y] to refresh later.
    if was_seeded {
        app.activity_log.push_simple(
            "PRD".into(),
            "First PRD load — fetching milestones + issues from GitHub…".into(),
            LogLevel::Info,
        );
        app.pending_commands
            .push(crate::tui::app::TuiCommand::SyncPrd);
    }
}

fn save_prd(app: &mut App) {
    let Some(prd) = app.prd.as_ref() else {
        return;
    };
    let store = FilePrdStore::new(repo_root());
    match store.save(prd) {
        Ok(()) => {
            if let Some(s) = app.prd_screen.as_mut() {
                s.dirty = false;
                s.save_status.last_saved = Some(Instant::now());
                s.save_status.last_error = None;
            }
            app.activity_log.push_simple(
                "PRD".into(),
                format!("PRD saved to {}", store.prd_path().display()),
                LogLevel::Info,
            );
        }
        Err(e) => {
            if let Some(s) = app.prd_screen.as_mut() {
                s.save_status.last_error = Some(e.to_string());
            }
            app.activity_log.push_simple(
                "PRD".into(),
                format!("PRD save failed: {e}"),
                LogLevel::Error,
            );
        }
    }
}

fn export_prd(app: &mut App) {
    let Some(prd) = app.prd.as_ref() else {
        return;
    };
    let path = repo_root().join("PRD.md");
    let body = to_markdown(prd);
    match std::fs::write(&path, body) {
        Ok(()) => app.activity_log.push_simple(
            "PRD".into(),
            format!("Exported PRD to {}", path.display()),
            LogLevel::Info,
        ),
        Err(e) => app.activity_log.push_simple(
            "PRD".into(),
            format!("Export failed: {e}"),
            LogLevel::Error,
        ),
    }
}

fn queue_sync(app: &mut App) {
    // Mark the screen state so the header chip flips to "⟳ Syncing…" on
    // the next render — without this the user has no immediate feedback
    // that their keypress did anything.
    if let Some(s) = app.prd_screen.as_mut() {
        s.sync_status = PrdSyncStatus::Syncing {
            started_at: Instant::now(),
        };
    }
    app.activity_log.push_simple(
        "PRD".into(),
        "Fetching milestones + issues from GitHub…".into(),
        LogLevel::Info,
    );
    app.pending_commands
        .push(crate::tui::app::TuiCommand::SyncPrd);
}

fn repo_root() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}
