//! Focus walking and cooperative inner-widget delegation for
//! [`super::DynamicMapWidget`].
//!
//! Split out of `dynamic_map.rs` (#908). Owns the Up/Down/Tab walks and
//! the chord delegation that lets focused nested widgets (`ListEditor`,
//! nested `DynamicMap`) own `a`/`d`/`u`/`Enter` before the outer
//! Add/Remove/Undo chords steal them.

use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::widgets::{WidgetAction, WidgetKind};

use super::{DynamicMapWidget, MapFocus};

impl DynamicMapWidget {
    /// Walk inner focus one step backward without consuming the outer
    /// SettingsScreen Up arrow. Returns true when focus moved so the
    /// outer can leave `field_index` alone — only a return of `false`
    /// signals the boundary (`SubtabStrip`) and lets the outer cursor
    /// climb up to the previous field.
    pub fn try_focus_prev(&mut self) -> bool {
        if self.try_advance_focused_inner(false) {
            return true;
        }
        if !matches!(self.focus, MapFocus::EntryField(_)) {
            return false;
        }
        let before = self.focus.clone();
        self.focus_prev_field();
        self.focus != before
    }

    /// Mirror of [`try_focus_prev`] for the Down arrow. Returns true when
    /// focus advanced into (or further inside) the entry-field group.
    pub fn try_focus_next(&mut self) -> bool {
        if self.try_advance_focused_inner(true) {
            return true;
        }
        match self.focus {
            MapFocus::SubtabStrip if self.active_entry().is_some() => {
                let before = self.focus.clone();
                self.focus_next_field();
                self.focus != before
            }
            MapFocus::EntryField(_) => {
                let before = self.focus.clone();
                self.focus_next_field();
                self.focus != before
            }
            _ => false,
        }
    }

    /// Try to advance the focused inner widget's own focus stack one step.
    /// Returns true when the inner moved its focus (outer should stop);
    /// false when the inner has nothing to advance to (outer should
    /// continue walking its own fields). `forward=true` is Down/Tab,
    /// `forward=false` is Up/BackTab.
    pub(super) fn try_advance_focused_inner(&mut self, forward: bool) -> bool {
        let MapFocus::EntryField(n) = self.focus else {
            return false;
        };
        let Some(active) = self.active_idx else {
            return false;
        };
        let Some(entry) = self.entries.get_mut(active) else {
            return false;
        };
        let Some(field) = entry.fields.get_mut(n) else {
            return false;
        };
        if forward {
            field.widget.try_focus_next()
        } else {
            field.widget.try_focus_prev()
        }
    }

    pub(super) fn focus_next_field(&mut self) {
        let visible = self.visible_field_indices();
        match self.focus {
            MapFocus::SubtabStrip if self.active_entry().is_some() => {
                if let Some(&first) = visible.first() {
                    self.focus = MapFocus::EntryField(first);
                }
            }
            MapFocus::EntryField(n) => {
                if let Some(next) = visible.iter().find(|&&idx| idx > n).copied() {
                    self.focus = MapFocus::EntryField(next);
                }
            }
            _ => {}
        }
    }

    pub(super) fn focus_prev_field(&mut self) {
        let visible = self.visible_field_indices();
        if let MapFocus::EntryField(n) = self.focus {
            let prev = visible.iter().rev().find(|&&idx| idx < n).copied();
            match prev {
                Some(idx) => self.focus = MapFocus::EntryField(idx),
                None => self.focus = MapFocus::SubtabStrip,
            }
        }
    }

    /// True when the focused entry-field widget claims `key` as one of its
    /// own chords. `ListEditor` owns `a`, `d`, and `Enter` in its
    /// non-editing mode; a nested `DynamicMapWidget` (e.g. the
    /// `role_overrides` editor inside a team entry) owns `a`, `d`, and
    /// `u` so the outer "Add team entry" / "Remove team entry" /
    /// "undo team entry" chords do not steal them while the user is
    /// adding / removing / undoing a role.
    pub(super) fn focused_field_owns_chord(&self, key: KeyEvent) -> bool {
        let MapFocus::EntryField(n) = self.focus else {
            return false;
        };
        let Some(active) = self.active_idx else {
            return false;
        };
        let Some(entry) = self.entries.get(active) else {
            return false;
        };
        let Some(field) = entry.fields.get(n) else {
            return false;
        };
        match field.widget {
            WidgetKind::ListEditor(_) => matches!(
                key.code,
                KeyCode::Char('a') | KeyCode::Char('d') | KeyCode::Enter
            ),
            WidgetKind::DynamicMap(_) => matches!(
                key.code,
                KeyCode::Char('a') | KeyCode::Char('d') | KeyCode::Char('u')
            ),
            _ => false,
        }
    }

    pub(super) fn delegate_to_focused_field(&mut self, key: KeyEvent) -> WidgetAction {
        let MapFocus::EntryField(n) = self.focus else {
            return WidgetAction::None;
        };
        let Some(active) = self.active_idx else {
            return WidgetAction::None;
        };
        let Some(entry) = self.entries.get_mut(active) else {
            return WidgetAction::None;
        };
        let Some(field) = entry.fields.get_mut(n) else {
            return WidgetAction::None;
        };
        let action = field.widget.handle_input(key);
        self.clamp_focus_to_visible();
        action
    }
}
