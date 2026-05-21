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

        // Sidebar search input. When active, captures every key event so
        // chars type into the query, Backspace pops, Enter/Esc exit. Esc
        // also clears the query; Enter keeps it so the filter sticks.
        // After every mutation we clamp `active_tab` onto the visible
        // (filtered) set so the selection bar always stays in view.
        if self.sidebar_search_active {
            match *code {
                KeyCode::Esc => {
                    self.sidebar_search.clear();
                    self.sidebar_search_active = false;
                }
                KeyCode::Enter => {
                    self.sidebar_search_active = false;
                }
                KeyCode::Backspace => {
                    self.sidebar_search.pop();
                }
                KeyCode::Char(c) if c.is_ascii() && !c.is_control() => {
                    self.sidebar_search.push(c);
                }
                _ => {}
            }
            self.clamp_active_to_visible();
            return ScreenAction::None;
        }

        // Enter search mode on `/` (vim-style) when no widget is taking
        // text input. `/` typed inside a TextInput sub-field stays a
        // literal because that path returns earlier via
        // active_widget_needs_insert.
        if matches!(*code, KeyCode::Char('/')) && modifiers.is_empty() {
            self.sidebar_search_active = true;
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
                    self.save_error_flash = Some((summary.clone(), std::time::Instant::now()));
                    return ScreenAction::LogActivity {
                        tag: "SETTINGS".into(),
                        message: format!("Save blocked — {}", summary),
                        level: crate::tui::activity_log::LogLevel::Error,
                    };
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
                        self.save_error_flash = Some((stored.clone(), std::time::Instant::now()));
                        ScreenAction::LogActivity {
                            tag: "SETTINGS".into(),
                            message: format!("Save failed — {}", stored),
                            level: crate::tui::activity_log::LogLevel::Error,
                        }
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
                    // Delegate first so DynamicMap/DynamicRows can walk
                    // back through their internal focus state. Only when
                    // the widget reports it is at the top boundary
                    // (return value `false`) does the outer cursor climb
                    // up to the previous field — that's how Up can return
                    // to the Default-provider dropdown above the agents
                    // DynamicMap instead of skipping entry fields.
                    let idx = self.field_index;
                    let tab = self.active_tab;
                    let inner_consumed = self.fields_per_tab[tab]
                        .get_mut(idx)
                        .map(|f| f.widget.try_focus_prev())
                        .unwrap_or(false);
                    if !inner_consumed && self.field_index > 0 {
                        self.field_index -= 1;
                    }
                }
                ScreenAction::None
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                if self.active_tab() == SettingsTab::Flags {
                    let max = crate::flags::Flag::all().len().saturating_sub(1);
                    if self.flags_selected < max {
                        self.flags_selected += 1;
                    }
                } else {
                    let idx = self.field_index;
                    let tab = self.active_tab;
                    let inner_consumed = self.fields_per_tab[tab]
                        .get_mut(idx)
                        .map(|f| f.widget.try_focus_next())
                        .unwrap_or(false);
                    if !inner_consumed
                        && self.field_count() > 0
                        && self.field_index + 1 < self.field_count()
                    {
                        self.field_index += 1;
                    }
                }
                ScreenAction::None
            }
            _ => {
                // Flags tab is read-only — skip widget delegation
                if self.active_tab() == SettingsTab::Flags {
                    return ScreenAction::None;
                }
                // Tab-level shortcuts for dynamic-cardinality widgets.
                // `a` add, `d` delete, `u` undo route to the FIRST
                // DynamicMap / DynamicRows widget on the active tab,
                // regardless of which scalar field currently owns the
                // cursor. Without this, pressing `a` while focused on
                // `max_concurrent` (a NumberStepper, no `a` handler)
                // would silently do nothing even though the tab also
                // hosts `completion_gates.commands` with a visible
                // "[a] add first row" hint.
                if matches!(
                    *code,
                    KeyCode::Char('a') | KeyCode::Char('d') | KeyCode::Char('u')
                ) && modifiers.is_empty()
                {
                    let idx = self.field_index;
                    let tab = self.active_tab;
                    let key_event = KeyEvent::new(*code, *modifiers);
                    // Don't double-handle if the current field is itself
                    // a dynamic widget — fall through to the normal
                    // delegation arm below so its own state machine sees
                    // the key exactly once.
                    let current_is_dynamic = self.fields_per_tab[tab]
                        .get(idx)
                        .map(|f| {
                            matches!(
                                f.widget,
                                crate::tui::widgets::WidgetKind::DynamicMap(_)
                                    | crate::tui::widgets::WidgetKind::DynamicRows(_)
                            )
                        })
                        .unwrap_or(false);
                    if !current_is_dynamic
                        && let Some(target_idx) = self.fields_per_tab[tab].iter().position(|f| {
                            matches!(
                                f.widget,
                                crate::tui::widgets::WidgetKind::DynamicMap(_)
                                    | crate::tui::widgets::WidgetKind::DynamicRows(_)
                            )
                        })
                    {
                        self.field_index = target_idx;
                        if let Some(field) = self.fields_per_tab[tab].get_mut(target_idx) {
                            field.widget.handle_input(key_event);
                        }
                        return ScreenAction::None;
                    }
                }
                // Special-case Project action rows: Enter/Space triggers
                // file operations rather than the toggle's normal behaviour.
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
                    if label.starts_with("Normalize Agent Config") {
                        return ScreenAction::NormalizeAgentConfig;
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
