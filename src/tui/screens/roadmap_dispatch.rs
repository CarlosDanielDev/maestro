//! Dispatch shim for the roadmap screen (#329).

#![deny(clippy::unwrap_used)]

use crate::tui::app::App;
use crate::tui::screens::ScreenAction;
use crate::tui::screens::roadmap::types::StatusFilter;
use crate::tui::screens::roadmap::{FilterField, RoadmapScreen};
use crossterm::event::{Event, KeyCode};

pub fn dispatch_input(app: &mut App, event: &Event) -> ScreenAction {
    let Event::Key(key) = event else {
        return ScreenAction::None;
    };

    ensure_loaded(app);

    let Some(screen) = app.roadmap_screen.as_mut() else {
        return ScreenAction::None;
    };

    if let Some(field) = screen.editing_filter.clone() {
        return handle_filter_edit(screen, field, key.code);
    }

    handle_view(app, key.code)
}

fn handle_filter_edit(
    screen: &mut RoadmapScreen,
    field: FilterField,
    code: KeyCode,
) -> ScreenAction {
    match code {
        KeyCode::Esc => {
            screen.editing_filter = None;
            ScreenAction::None
        }
        KeyCode::Enter => {
            screen.editing_filter = None;
            ScreenAction::None
        }
        KeyCode::Backspace => {
            match field {
                FilterField::Label => {
                    screen.filters.label.pop();
                }
                FilterField::Assignee => {
                    screen.filters.assignee.pop();
                }
                FilterField::Status => {}
            }
            ScreenAction::None
        }
        KeyCode::Char(c) => {
            match field {
                FilterField::Label => screen.filters.label.push(c),
                FilterField::Assignee => screen.filters.assignee.push(c),
                FilterField::Status => {}
            }
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

fn handle_view(app: &mut App, code: KeyCode) -> ScreenAction {
    let Some(screen) = app.roadmap_screen.as_mut() else {
        return ScreenAction::None;
    };
    match code {
        KeyCode::Esc | KeyCode::Char('q') => ScreenAction::Pop,
        KeyCode::Down | KeyCode::Char('j') => {
            screen.cursor_down();
            ScreenAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            screen.cursor_up();
            ScreenAction::None
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            screen.toggle_expand();
            ScreenAction::None
        }
        KeyCode::Char('D') => {
            // Drill into the focused milestone (#329 AC: drill-into-issue
            // navigation). Routes to the existing MilestoneView screen.
            if screen.focused_milestone().is_some() {
                ScreenAction::Push(crate::tui::app::TuiMode::MilestoneView)
            } else {
                ScreenAction::None
            }
        }
        KeyCode::Char('r') => {
            app.pending_commands
                .push(crate::tui::app::TuiCommand::SyncRoadmap);
            ScreenAction::None
        }
        KeyCode::Char('/') => {
            screen.editing_filter = Some(FilterField::Label);
            ScreenAction::None
        }
        KeyCode::Char('a') => {
            screen.editing_filter = Some(FilterField::Assignee);
            ScreenAction::None
        }
        KeyCode::Char('o') => {
            screen.filters.status = StatusFilter::Open;
            ScreenAction::None
        }
        KeyCode::Char('c') => {
            screen.filters.status = StatusFilter::Closed;
            ScreenAction::None
        }
        KeyCode::Char('x') => {
            screen.filters = Default::default();
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

pub fn ensure_loaded(app: &mut App) {
    if app.roadmap_screen.is_none() {
        app.roadmap_screen = Some(RoadmapScreen::new());
        app.pending_commands
            .push(crate::tui::app::TuiCommand::SyncRoadmap);
    }
}
