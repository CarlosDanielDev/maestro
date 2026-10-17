//! Fleet board: one screen for maestro pool sessions and external Claude
//! Code transcripts machine-wide (#1067). Scans on open; no timer.

mod draw;
pub(crate) mod rows;

pub use rows::{FleetKind, FleetRow, FleetSort, FleetSource};

use super::{Screen, ScreenAction};
use crate::session::discovery::scan_transcripts;
use crate::session::types::Session;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::screens::adopt_picker::repo_name;
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Frame, layout::Rect};
use std::path::PathBuf;
use std::time::SystemTime;

/// One table over every maestro pool session and every transcript on the
/// machine. Filter-as-you-type over repo and title; Tab flips the sort.
pub struct FleetBoardScreen {
    rows: Vec<FleetRow>,
    /// Indices into `rows` matching `filter_text`, in `sort` order.
    visible: Vec<usize>,
    selected: usize,
    scroll_offset: usize,
    pub(crate) filter_text: String,
    sort: FleetSort,
    /// Reference clock for the relative times. Injected so snapshots are
    /// deterministic; production passes `SystemTime::now()`.
    now: SystemTime,
    /// Visible list height from the last draw, for scroll sync.
    last_visible_height: usize,
    /// One-line feedback for the last refused action. Cleared by the next
    /// key, so it never goes stale.
    status: Option<String>,
}

impl FleetBoardScreen {
    /// Build the board over an already merged row list.
    pub fn new(rows: Vec<FleetRow>, now: SystemTime) -> Self {
        let mut screen = Self {
            rows,
            visible: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            filter_text: String::new(),
            sort: FleetSort::default(),
            now,
            last_visible_height: 20,
            status: None,
        };
        screen.reapply();
        screen
    }

    /// Scan this machine's transcripts and merge them with the pool. The
    /// repo column for pool rows is the directory maestro runs in. An unset
    /// `HOME` or a missing `~/.claude/projects` is an empty scan, never an
    /// error.
    ///
    /// Known limitation: the scan is synchronous and reads every line of
    /// every transcript. On a transcript tree that is gigabytes in size, the
    /// raw read alone takes seconds on a cold cache, and the UI thread waits.
    /// A follow-up will bound the cost for this screen and the adopt picker
    /// alike, by reading only a tail window for the title and/or running the
    /// scan on a background thread.
    pub fn scan(pool: &[&Session], now: SystemTime) -> Self {
        let transcripts = match std::env::var_os("HOME") {
            Some(home) => scan_transcripts(&PathBuf::from(home)),
            None => Vec::new(),
        };
        let cwd = std::env::current_dir().unwrap_or_default();
        let rows = rows::build_rows(pool, &repo_name(&cwd), &transcripts);
        Self::new(rows, now)
    }

    /// Rows matching `filter_text`, in the current sort order.
    pub fn filtered_rows(&self) -> Vec<&FleetRow> {
        self.visible.iter().map(|&i| &self.rows[i]).collect()
    }

    /// The row under the cursor, if any.
    pub fn selected_row(&self) -> Option<&FleetRow> {
        self.visible.get(self.selected).map(|&i| &self.rows[i])
    }

    pub fn sort(&self) -> FleetSort {
        self.sort
    }

    /// Show a refusal (or other feedback) on the board itself.
    pub fn set_status(&mut self, message: String) {
        self.status = Some(message);
    }

    /// The feedback line currently shown, if any.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(super) fn rows_is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub(super) fn selected_index(&self) -> usize {
        self.selected
    }

    pub(super) fn now(&self) -> SystemTime {
        self.now
    }

    /// Record the list height from a draw, then keep the cursor in view.
    pub(super) fn set_visible_height(&mut self, height: usize) {
        self.last_visible_height = height;
        self.sync_scroll();
    }

    /// At most `height` visible rows from the scroll offset, paired with
    /// their display index.
    pub(super) fn visible_window(&self, height: usize) -> Vec<(usize, &FleetRow)> {
        self.visible
            .iter()
            .map(|&i| &self.rows[i])
            .enumerate()
            .skip(self.scroll_offset)
            .take(height)
            .collect()
    }

    fn reapply(&mut self) {
        self.visible = rows::sorted_indices(&self.rows, &self.filter_text, self.sort);
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        self.sync_scroll();
    }

    fn sync_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.last_visible_height {
            self.scroll_offset = self.selected - self.last_visible_height + 1;
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.sync_scroll();
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.visible.len() {
            self.selected += 1;
            self.sync_scroll();
        }
    }

    /// Enter on a pool row asks the dispatcher to open its screen. An
    /// external row opens nothing: adopting it is the Adopt Session screen's
    /// job (#1068 will move that action here).
    fn activate(&mut self) -> ScreenAction {
        let Some(source) = self.selected_row().map(|r| r.source.clone()) else {
            return ScreenAction::None;
        };
        match source {
            FleetSource::Pool { session_id } => ScreenAction::OpenFleetRow { session_id },
            FleetSource::Transcript { .. } => {
                self.set_status(
                    "External session: adopt it from Adopt Session (a) on the Landing menu."
                        .to_string(),
                );
                ScreenAction::None
            }
        }
    }
}

impl KeymapProvider for FleetBoardScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Fleet Board",
            bindings: vec![
                KeyBinding {
                    key: "↑/↓",
                    description: "Select row",
                },
                KeyBinding {
                    key: "Type",
                    description: "Filter by repo/title",
                },
                KeyBinding {
                    key: "Tab",
                    description: "Toggle sort",
                },
                KeyBinding {
                    key: "Enter",
                    description: "Open row",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Clear filter / Back",
                },
            ],
        }]
    }
}

impl Screen for FleetBoardScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };
        self.status = None;

        match code {
            KeyCode::Esc => {
                if self.filter_text.is_empty() {
                    ScreenAction::Pop
                } else {
                    self.filter_text.clear();
                    self.reapply();
                    ScreenAction::None
                }
            }
            KeyCode::Up => {
                self.move_up();
                ScreenAction::None
            }
            KeyCode::Down => {
                self.move_down();
                ScreenAction::None
            }
            KeyCode::Tab => {
                self.sort = self.sort.toggled();
                self.reapply();
                ScreenAction::None
            }
            KeyCode::Backspace => {
                self.filter_text.pop();
                self.reapply();
                ScreenAction::None
            }
            KeyCode::Enter => self.activate(),
            KeyCode::Char(c) if is_text_input(*modifiers) => {
                self.filter_text.push(*c);
                self.reapply();
                ScreenAction::None
            }
            _ => ScreenAction::None,
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        Some(InputMode::Insert)
    }
}

/// Only plain and Shift-modified keys type filter text. Ctrl, Alt and Super
/// chords are commands, so they must not leak into the filter.
fn is_text_input(modifiers: KeyModifiers) -> bool {
    !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
}

#[cfg(test)]
mod snapshot_tests;
#[cfg(test)]
mod tests;
