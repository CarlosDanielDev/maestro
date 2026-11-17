//! Keys on Home. Mission Control needs App data (the ranked queue, the
//! pool) next to its own state, so it is dispatched like the PRD screen
//! instead of through the `Screen` trait.

use crossterm::event::{Event, KeyEventKind, KeyModifiers};

use super::MissionCommand;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::attention::{AttentionItem, Subject};
use crate::tui::screens::ScreenAction;

pub fn dispatch_input(app: &mut App, event: &Event) -> ScreenAction {
    let Event::Key(key) = event else {
        return ScreenAction::None;
    };
    // Chords (Ctrl+…, Alt+…) are global keys, never a Home letter.
    if key.kind != KeyEventKind::Press
        || key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return ScreenAction::None;
    }
    let items = app.attention();
    let gate = app.action_gate();
    let command = app
        .screen_state
        .mission_control
        .handle_key(key.code, items.len(), &gate);
    match command {
        MissionCommand::None => ScreenAction::None,
        MissionCommand::Push(mode) => ScreenAction::Push(mode),
        MissionCommand::OpenAttention(i) => match items.get(i) {
            Some(item) => open(app, item),
            None => ScreenAction::None,
        },
    }
}

/// `⏎` on a Needs-you row: a session opens the session Overview with that
/// row selected; doctor, gh auth, budget and quota rows open Dashboard,
/// where the warnings and the spend live.
fn open(app: &mut App, item: &AttentionItem) -> ScreenAction {
    let Subject::Session { id, .. } = item.subject else {
        return ScreenAction::Push(TuiMode::Dashboard);
    };
    if let Some(index) = app.pool.all_sessions().iter().position(|s| s.id == id) {
        app.panel_view.selected = Some(index);
    }
    ScreenAction::Push(TuiMode::Overview)
}
