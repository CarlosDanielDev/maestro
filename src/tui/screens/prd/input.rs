//! PRD screen input handler (#321).

#![deny(clippy::unwrap_used)]

use crate::prd::model::Prd;
use crate::tui::screens::prd::state::{EditTarget, PrdAction, PrdScreen, PrdSection};
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(screen: &mut PrdScreen, prd: &mut Prd, key: KeyEvent) -> PrdAction {
    // First user interaction dismisses the welcome banner so it stops
    // taking screen real-estate after they've gotten oriented.
    screen.first_view = false;
    if let Some(edit) = screen.edit.as_ref() {
        let mode = screen_edit_target(edit);
        return handle_edit_key(mode, key, screen, prd);
    }
    handle_view_key(key, screen, prd)
}

fn screen_edit_target(target: &EditTarget) -> EditMode {
    match target {
        EditTarget::NewGoal { .. } => EditMode::NewGoal,
        EditTarget::NewNonGoal { .. } => EditMode::NewNonGoal,
    }
}

#[derive(Copy, Clone)]
enum EditMode {
    NewGoal,
    NewNonGoal,
}

fn handle_edit_key(
    mode: EditMode,
    key: KeyEvent,
    screen: &mut PrdScreen,
    prd: &mut Prd,
) -> PrdAction {
    match key.code {
        KeyCode::Esc => {
            screen.edit = None;
            PrdAction::None
        }
        KeyCode::Enter => {
            let committed = match mode {
                EditMode::NewGoal => screen.commit_new_goal(prd),
                EditMode::NewNonGoal => screen.commit_new_non_goal(prd),
            };
            if committed {
                PrdAction::Save
            } else {
                PrdAction::None
            }
        }
        KeyCode::Backspace => {
            if let Some(target) = screen.edit.as_mut() {
                let buf = match target {
                    EditTarget::NewGoal { buffer } => buffer,
                    EditTarget::NewNonGoal { buffer } => buffer,
                };
                buf.pop();
            }
            PrdAction::None
        }
        KeyCode::Char(c) => {
            if let Some(target) = screen.edit.as_mut() {
                let buf = match target {
                    EditTarget::NewGoal { buffer } => buffer,
                    EditTarget::NewNonGoal { buffer } => buffer,
                };
                buf.push(c);
            }
            PrdAction::None
        }
        _ => PrdAction::None,
    }
}

fn handle_view_key(key: KeyEvent, screen: &mut PrdScreen, prd: &mut Prd) -> PrdAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => PrdAction::Back,
        KeyCode::Tab | KeyCode::Char('l') => {
            screen.focus_next();
            PrdAction::None
        }
        KeyCode::BackTab | KeyCode::Char('h') => {
            screen.focus_prev();
            PrdAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            screen.cursor_down_in_focus(prd);
            PrdAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            screen.cursor_up_in_focus();
            PrdAction::None
        }
        KeyCode::Char('s') => PrdAction::Save,
        KeyCode::Char('e') => PrdAction::Export,
        KeyCode::Char('y') => PrdAction::Sync,
        KeyCode::Char('n') => {
            screen.edit = Some(match screen.focus {
                PrdSection::Goals => EditTarget::NewGoal {
                    buffer: String::new(),
                },
                PrdSection::NonGoals => EditTarget::NewNonGoal {
                    buffer: String::new(),
                },
                _ => return PrdAction::None,
            });
            PrdAction::None
        }
        KeyCode::Char(' ') if matches!(screen.focus, PrdSection::Goals) => {
            if screen.toggle_goal_done(prd) {
                PrdAction::Save
            } else {
                PrdAction::None
            }
        }
        KeyCode::Char('d') => {
            let removed = match screen.focus {
                PrdSection::Goals => screen.delete_focused_goal(prd),
                PrdSection::NonGoals => screen.delete_focused_non_goal(prd),
                _ => false,
            };
            if removed {
                PrdAction::Save
            } else {
                PrdAction::None
            }
        }
        _ => PrdAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn esc_in_view_mode_returns_back() {
        let mut s = PrdScreen::new();
        let mut p = Prd::new();
        assert_eq!(
            handle_key(&mut s, &mut p, key(KeyCode::Esc)),
            PrdAction::Back
        );
    }

    #[test]
    fn s_returns_save() {
        let mut s = PrdScreen::new();
        let mut p = Prd::new();
        assert_eq!(
            handle_key(&mut s, &mut p, key(KeyCode::Char('s'))),
            PrdAction::Save
        );
    }

    #[test]
    fn e_returns_export() {
        let mut s = PrdScreen::new();
        let mut p = Prd::new();
        assert_eq!(
            handle_key(&mut s, &mut p, key(KeyCode::Char('e'))),
            PrdAction::Export
        );
    }

    #[test]
    fn tab_advances_focus_and_returns_none() {
        let mut s = PrdScreen::new();
        let mut p = Prd::new();
        let action = handle_key(&mut s, &mut p, key(KeyCode::Tab));
        assert_eq!(action, PrdAction::None);
        assert_eq!(s.focus, PrdSection::Goals);
    }

    #[test]
    fn n_in_goals_opens_edit_buffer_for_new_goal() {
        let mut s = PrdScreen::new();
        s.focus = PrdSection::Goals;
        let mut p = Prd::new();
        handle_key(&mut s, &mut p, key(KeyCode::Char('n')));
        assert!(matches!(s.edit, Some(EditTarget::NewGoal { .. })));
    }

    #[test]
    fn typing_in_edit_mode_appends_to_buffer() {
        let mut s = PrdScreen::new();
        s.focus = PrdSection::Goals;
        let mut p = Prd::new();
        handle_key(&mut s, &mut p, key(KeyCode::Char('n')));
        handle_key(&mut s, &mut p, key(KeyCode::Char('h')));
        handle_key(&mut s, &mut p, key(KeyCode::Char('i')));
        if let Some(EditTarget::NewGoal { buffer }) = &s.edit {
            assert_eq!(buffer, "hi");
        } else {
            panic!("expected NewGoal edit target");
        }
    }

    #[test]
    fn enter_in_edit_mode_commits_and_saves() {
        let mut s = PrdScreen::new();
        s.focus = PrdSection::Goals;
        let mut p = Prd::new();
        handle_key(&mut s, &mut p, key(KeyCode::Char('n')));
        for c in "ship".chars() {
            handle_key(&mut s, &mut p, key(KeyCode::Char(c)));
        }
        let action = handle_key(&mut s, &mut p, key(KeyCode::Enter));
        assert_eq!(action, PrdAction::Save);
        assert_eq!(p.goals.len(), 1);
        assert_eq!(p.goals[0].text, "ship");
        assert!(s.edit.is_none());
    }

    #[test]
    fn esc_in_edit_mode_cancels_without_save() {
        let mut s = PrdScreen::new();
        s.focus = PrdSection::Goals;
        let mut p = Prd::new();
        handle_key(&mut s, &mut p, key(KeyCode::Char('n')));
        handle_key(&mut s, &mut p, key(KeyCode::Char('x')));
        let action = handle_key(&mut s, &mut p, key(KeyCode::Esc));
        assert_eq!(action, PrdAction::None);
        assert!(s.edit.is_none());
        assert!(p.goals.is_empty());
    }

    #[test]
    fn backspace_in_edit_mode_pops_buffer() {
        let mut s = PrdScreen::new();
        s.focus = PrdSection::Goals;
        let mut p = Prd::new();
        handle_key(&mut s, &mut p, key(KeyCode::Char('n')));
        handle_key(&mut s, &mut p, key(KeyCode::Char('a')));
        handle_key(&mut s, &mut p, key(KeyCode::Char('b')));
        handle_key(&mut s, &mut p, key(KeyCode::Backspace));
        if let Some(EditTarget::NewGoal { buffer }) = &s.edit {
            assert_eq!(buffer, "a");
        } else {
            panic!("expected edit target");
        }
    }

    #[test]
    fn space_in_goals_toggles_done() {
        let mut s = PrdScreen::new();
        s.focus = PrdSection::Goals;
        let mut p = Prd::new();
        p.add_goal("first");
        let action = handle_key(&mut s, &mut p, key(KeyCode::Char(' ')));
        assert_eq!(action, PrdAction::Save);
        assert!(p.goals[0].done);
    }

    #[test]
    fn d_in_goals_deletes_focused() {
        let mut s = PrdScreen::new();
        s.focus = PrdSection::Goals;
        let mut p = Prd::new();
        p.add_goal("a");
        p.add_goal("b");
        let action = handle_key(&mut s, &mut p, key(KeyCode::Char('d')));
        assert_eq!(action, PrdAction::Save);
        assert_eq!(p.goals.len(), 1);
        assert_eq!(p.goals[0].text, "b");
    }
}
