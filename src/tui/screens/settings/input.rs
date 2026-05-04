use crate::tui::navigation::InputMode;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetAction;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, layout::Rect};

use super::{SettingsScreen, SettingsTab};
use crate::tui::screens::{Screen, ScreenAction};

impl Screen for SettingsScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: crossterm::event::KeyEventKind::Press,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };

        if self.active_widget_needs_insert() {
            let idx = self.field_index;
            let tab = self.active_tab;
            let key_event = KeyEvent::new(*code, *modifiers);
            if let Some(field) = self.fields_per_tab[tab].get_mut(idx) {
                field.widget.handle_input(key_event);
            }
            self.sync_widgets_to_config();
            self.run_all_validations();
            return ScreenAction::None;
        }

        // Handle discard confirmation
        if self.confirm_discard {
            return match *code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm_discard = false;
                    // Clear preview on discard — the Pop handler will also clear it
                    ScreenAction::Pop
                }
                _ => {
                    self.confirm_discard = false;
                    ScreenAction::None
                }
            };
        }

        match (*code, *modifiers) {
            (KeyCode::Esc, _) => {
                if self.is_dirty() {
                    self.confirm_discard = true;
                    ScreenAction::None
                } else {
                    ScreenAction::Pop
                }
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                if self.has_validation_errors() {
                    let summary = self
                        .validation_error_summary()
                        .unwrap_or_else(|| "validation failed".to_string());
                    self.save_error_flash = Some((summary, std::time::Instant::now()));
                    return ScreenAction::None;
                }
                match self.save_config() {
                    Ok(()) => {
                        let config = self.config.clone();
                        // Promote preview to actual theme on save
                        self.live_preview = false;
                        ScreenAction::UpdateConfig(Box::new(config))
                    }
                    Err(e) => {
                        tracing::error!("Settings save failed: {:#}", e);
                        let stored: String = format!("{:#}", e).chars().take(512).collect();
                        self.save_error_flash = Some((stored, std::time::Instant::now()));
                        ScreenAction::None
                    }
                }
            }
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                self.reset_to_original();
                self.live_preview = false;
                ScreenAction::PreviewTheme(None)
            }
            (KeyCode::Tab, _) => {
                self.next_tab();
                ScreenAction::None
            }
            (KeyCode::BackTab, _) => {
                self.prev_tab();
                ScreenAction::None
            }
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                if self.active_tab() == SettingsTab::Flags {
                    self.flags_selected = self.flags_selected.saturating_sub(1);
                } else {
                    self.field_index = self.field_index.saturating_sub(1);
                }
                ScreenAction::None
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                if self.active_tab() == SettingsTab::Flags {
                    let max = crate::flags::Flag::all().len().saturating_sub(1);
                    if self.flags_selected < max {
                        self.flags_selected += 1;
                    }
                } else if self.field_count() > 0 && self.field_index + 1 < self.field_count() {
                    self.field_index += 1;
                }
                ScreenAction::None
            }
            _ => {
                // Flags tab is read-only — skip widget delegation
                if self.active_tab() == SettingsTab::Flags {
                    return ScreenAction::None;
                }
                // Special-case the "Reset Settings" row on the Project tab
                // — Enter/Space triggers re-detection rather than the
                // toggle's normal behaviour.
                if self.active_tab() == SettingsTab::Project
                    && matches!(*code, KeyCode::Enter | KeyCode::Char(' '))
                {
                    let label = self
                        .current_fields()
                        .get(self.field_index)
                        .map(|f| f.widget.label())
                        .unwrap_or("");
                    if label.starts_with("Reset Settings") {
                        return ScreenAction::ResetSettingsFromDetection;
                    }
                }
                // Delegate to active widget for non-navigation keys
                let idx = self.field_index;
                let tab = self.active_tab;
                let key_event = KeyEvent::new(*code, *modifiers);
                let changed = self.fields_per_tab[tab]
                    .get_mut(idx)
                    .map(|f| f.widget.handle_input(key_event))
                    == Some(WidgetAction::Changed);
                if changed {
                    self.sync_widgets_to_config();
                    self.run_all_validations();
                    if self.live_preview {
                        return ScreenAction::PreviewTheme(Some(self.config.tui.theme.clone()));
                    }
                }
                ScreenAction::None
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_screen(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        if self.active_widget_needs_insert() {
            Some(InputMode::Insert)
        } else {
            Some(InputMode::Normal)
        }
    }
}
