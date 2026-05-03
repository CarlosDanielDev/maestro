//! Dispatch shim for the bypass-warning screen (#328).

#![deny(clippy::unwrap_used)]

use crate::tui::app::App;
use crate::tui::screens::ScreenAction;
use crate::tui::screens::bypass_warning::{BypassWarningOutcome, handle_key as warning_handle_key};
use crossterm::event::Event;

pub fn dispatch_input(app: &mut App, event: &Event) -> ScreenAction {
    let Event::Key(key) = event else {
        return ScreenAction::None;
    };
    let Some(state) = app.screen_state.bypass_warning_screen.as_mut() else {
        return ScreenAction::Pop;
    };
    let outcome = warning_handle_key(state, *key);
    match outcome {
        BypassWarningOutcome::Pending => ScreenAction::None,
        BypassWarningOutcome::Confirmed => {
            app.screen_state.bypass_warning_screen = None;
            app.confirm_bypass_activation("tui");
            ScreenAction::Pop
        }
        BypassWarningOutcome::Cancelled => {
            app.screen_state.bypass_warning_screen = None;
            ScreenAction::Pop
        }
    }
}
