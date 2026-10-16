//! Global picker for external Claude Code sessions (#1054).
//!
//! Reached from the Landing menu. Lists the `DiscoveredSession`s produced by
//! [`crate::session::discovery::scan_transcripts`] (already sorted
//! last-active-first), filters as you type, and adopts the selected row:
//! the session is created in the pool via
//! [`crate::session::pool::SessionPool::create_adopted_session`] and the app
//! routes to the Interaction screen. Rows whose transcript was touched
//! inside [`crate::session::adopt::ADOPT_QUIET_PERIOD`] carry a live badge
//! and ask for confirmation before adopting (council verdict: warn, never
//! block).

mod draw;

use super::{Screen, ScreenAction};
use crate::session::adopt::ADOPT_QUIET_PERIOD;
use crate::session::discovery::DiscoveredSession;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};
use std::path::PathBuf;
use std::time::SystemTime;

/// List-and-filter picker over every Claude Code transcript on the machine.
pub struct AdoptPickerScreen {
    sessions: Vec<DiscoveredSession>,
    /// Indices into `sessions` matching `filter_text`, in scan order
    /// (last-active first).
    filtered_indices: Vec<usize>,
    selected: usize,
    scroll_offset: usize,
    pub(crate) filter_text: String,
    /// Reference clock for the live badge and the relative times. Injected
    /// so tests (and snapshots) are deterministic; production passes
    /// `SystemTime::now()` at screen construction.
    now: SystemTime,
    /// `Some(filtered_index)` while the live-session confirm modal is open.
    confirm_live: Option<usize>,
    /// Visible list height from the last draw, for scroll sync.
    last_visible_height: usize,
    /// One-line feedback for the last refused adoption. Cleared by the
    /// next key, so it never goes stale.
    status: Option<String>,
}

impl AdoptPickerScreen {
    /// Build the picker over a scanned session list. `now` pins the clock
    /// the live badge is judged against.
    pub fn new(sessions: Vec<DiscoveredSession>, now: SystemTime) -> Self {
        let mut screen = Self {
            sessions,
            filtered_indices: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            filter_text: String::new(),
            now,
            confirm_live: None,
            last_visible_height: 20,
            status: None,
        };
        screen.reapply_filter();
        screen
    }

    /// Scan this machine's transcripts and open the picker. An unset `HOME`
    /// or a missing `~/.claude/projects` is the normal first-run condition:
    /// an empty picker, never an error.
    pub fn scan(now: SystemTime) -> Self {
        let sessions = match std::env::var_os("HOME") {
            Some(home) => crate::session::discovery::scan_transcripts(&PathBuf::from(home)),
            None => Vec::new(),
        };
        Self::new(sessions, now)
    }

    /// Sessions matching `filter_text` (lowercase substring on the repo name
    /// and the title), in scan order.
    pub fn filtered_sessions(&self) -> Vec<&DiscoveredSession> {
        self.filtered_indices
            .iter()
            .map(|&i| &self.sessions[i])
            .collect()
    }

    /// The row under the cursor, if any.
    pub fn selected_session(&self) -> Option<&DiscoveredSession> {
        self.filtered_indices
            .get(self.selected)
            .map(|&i| &self.sessions[i])
    }

    /// True when the transcript was touched inside the adopt quiet period —
    /// the source session may still be live somewhere else (#1053).
    pub fn is_live(&self, session: &DiscoveredSession) -> bool {
        // A future mtime (clock skew) counts as maximally fresh, matching
        // `session::adopt::modified_within`.
        self.now
            .duration_since(session.last_active)
            .unwrap_or_default()
            < ADOPT_QUIET_PERIOD
    }

    /// Show a refusal (or other feedback) on the picker itself.
    pub fn set_status(&mut self, message: String) {
        self.status = Some(message);
    }

    /// The feedback line currently shown, if any.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// True while the live-session confirm modal is open.
    pub fn confirm_pending(&self) -> bool {
        self.confirm_live.is_some()
    }

    fn reapply_filter(&mut self) {
        let filter = self.filter_text.to_lowercase();
        self.filtered_indices = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                filter.is_empty()
                    || s.title.to_lowercase().contains(&filter)
                    || repo_name(&s.cwd).to_lowercase().contains(&filter)
            })
            .map(|(i, _)| i)
            .collect();
        self.selected = self
            .selected
            .min(self.filtered_indices.len().saturating_sub(1));
        self.sync_scroll();
    }

    fn sync_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.last_visible_height {
            self.scroll_offset = self.selected - self.last_visible_height + 1;
        }
    }

    /// Data carried to the dispatcher for the row under the cursor.
    fn adopt_payload(&self) -> Option<ScreenAction> {
        let session = self.selected_session()?;
        Some(ScreenAction::AdoptSession {
            agent_session_id: session.session_id.clone(),
            cwd: session.cwd.clone(),
            title: Some(session.title.clone()).filter(|t| !t.is_empty()),
        })
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.sync_scroll();
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.filtered_indices.len() {
            self.selected += 1;
            self.sync_scroll();
        }
    }
}

/// Last path component of `cwd` — the repo column ("maestro" for
/// `/home/carlos/projects/maestro`). Falls back to the whole path.
pub(super) fn repo_name(cwd: &std::path::Path) -> String {
    cwd.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| cwd.to_string_lossy().into_owned())
}

impl KeymapProvider for AdoptPickerScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Adopt Session",
            bindings: vec![
                KeyBinding {
                    key: "↑/↓",
                    description: "Select session",
                },
                KeyBinding {
                    key: "Type",
                    description: "Filter by repo/title",
                },
                KeyBinding {
                    key: "Enter",
                    description: "Adopt selected",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Clear filter / Back",
                },
            ],
        }]
    }
}

impl Screen for AdoptPickerScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };
        self.status = None;

        // Live-session confirm modal: swallows everything else so typing
        // cannot change the list behind it.
        // Cancel is the default: only `y` adopts; every other key closes
        // the modal without adopting.
        if self.confirm_live.is_some() {
            self.confirm_live = None;
            return match code {
                KeyCode::Char('y' | 'Y') => self.adopt_payload().unwrap_or(ScreenAction::None),
                _ => ScreenAction::None,
            };
        }

        match code {
            KeyCode::Esc => {
                if self.filter_text.is_empty() {
                    ScreenAction::Pop
                } else {
                    self.filter_text.clear();
                    self.reapply_filter();
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
            KeyCode::Backspace => {
                self.filter_text.pop();
                self.reapply_filter();
                ScreenAction::None
            }
            KeyCode::Enter => {
                let Some(session) = self.selected_session() else {
                    return ScreenAction::None;
                };
                if self.is_live(session) {
                    self.confirm_live = Some(self.selected);
                    ScreenAction::None
                } else {
                    self.adopt_payload().unwrap_or(ScreenAction::None)
                }
            }
            KeyCode::Char(c) => {
                self.filter_text.push(*c);
                self.reapply_filter();
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

#[cfg(test)]
mod snapshot_tests;
#[cfg(test)]
mod tests;
