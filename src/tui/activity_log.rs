use crate::tui::theme::Theme;
use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub session_label: String,
    pub message: String,
    pub level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Info,
    Tool,
    Warn,
    Error,
}

impl LogLevel {
    pub fn color(&self, theme: &Theme) -> Color {
        match self {
            Self::Info => theme.text_primary,
            Self::Tool => theme.accent_info,
            Self::Warn => theme.accent_warning,
            Self::Error => theme.accent_error,
        }
    }
}

pub struct ActivityLog {
    entries: Vec<LogEntry>,
    max_entries: usize,
    pub scroll_offset: usize,
}

impl ActivityLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
            scroll_offset: 0,
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.drain(..self.entries.len() - self.max_entries);
            // Clamp scroll_offset after trim
            if self.scroll_offset >= self.entries.len() {
                self.scroll_offset = self.entries.len().saturating_sub(1);
            }
        }
    }

    pub fn push_simple(&mut self, label: String, message: String, level: LogLevel) {
        self.push(LogEntry {
            timestamp: Utc::now(),
            session_label: label,
            message,
            level,
        });
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        let max = self.entries.len().saturating_sub(1);
        if self.scroll_offset < max {
            self.scroll_offset += 1;
        }
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_inactive))
            .title(" Activity Log ");

        let inner_height = area.height.saturating_sub(2) as usize;
        let total = self.entries.len();

        if total == 0 {
            let msg = Paragraph::new("No activity yet")
                .style(Style::default().fg(theme.text_secondary))
                .block(block);
            f.render_widget(msg, area);
            return;
        }

        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(inner_height);

        let items: Vec<ListItem> = self.entries[start..end]
            .iter()
            .map(|entry| {
                let time = entry.timestamp.format("%H:%M:%S");
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", time), Style::default().fg(theme.text_secondary)),
                    Span::styled(
                        format!("[{}] ", entry.session_label),
                        Style::default().fg(theme.accent_identifier),
                    ),
                    Span::styled(
                        entry.message.clone(),
                        Style::default().fg(entry.level.color(theme)),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(msg: &str) -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            session_label: "S-test".to_string(),
            message: msg.to_string(),
            level: LogLevel::Info,
        }
    }

    #[test]
    fn push_adds_entry() {
        let mut log = ActivityLog::new(100);
        log.push(make_entry("hello"));
        assert_eq!(log.entries().len(), 1);
    }

    #[test]
    fn push_preserves_order() {
        let mut log = ActivityLog::new(100);
        log.push(make_entry("first"));
        log.push(make_entry("second"));
        log.push(make_entry("third"));
        assert_eq!(log.entries()[0].message, "first");
        assert_eq!(log.entries()[2].message, "third");
    }

    #[test]
    fn push_trims_oldest_when_exceeded() {
        let mut log = ActivityLog::new(3);
        log.push(make_entry("one"));
        log.push(make_entry("two"));
        log.push(make_entry("three"));
        log.push(make_entry("four"));
        assert_eq!(log.entries().len(), 3);
        assert_eq!(log.entries()[0].message, "two");
        assert_eq!(log.entries()[2].message, "four");
    }

    #[test]
    fn push_at_max_does_not_trim() {
        let mut log = ActivityLog::new(3);
        log.push(make_entry("a"));
        log.push(make_entry("b"));
        log.push(make_entry("c"));
        assert_eq!(log.entries().len(), 3);
        assert_eq!(log.entries()[0].message, "a");
    }

    #[test]
    fn scroll_offset_starts_at_zero() {
        let log = ActivityLog::new(100);
        assert_eq!(log.scroll_offset, 0);
    }

    #[test]
    fn scroll_down_increments() {
        let mut log = ActivityLog::new(100);
        log.push(make_entry("a"));
        log.push(make_entry("b"));
        log.push(make_entry("c"));
        log.scroll_down();
        assert_eq!(log.scroll_offset, 1);
        log.scroll_down();
        assert_eq!(log.scroll_offset, 2);
    }

    #[test]
    fn scroll_down_caps_at_last_entry() {
        let mut log = ActivityLog::new(100);
        log.push(make_entry("a"));
        log.push(make_entry("b"));
        for _ in 0..10 {
            log.scroll_down();
        }
        assert_eq!(log.scroll_offset, 1);
    }

    #[test]
    fn scroll_down_on_empty_stays_zero() {
        let mut log = ActivityLog::new(100);
        log.scroll_down();
        assert_eq!(log.scroll_offset, 0);
    }

    #[test]
    fn scroll_up_decrements() {
        let mut log = ActivityLog::new(100);
        log.push(make_entry("a"));
        log.push(make_entry("b"));
        log.push(make_entry("c"));
        log.scroll_down();
        log.scroll_down();
        assert_eq!(log.scroll_offset, 2);
        log.scroll_up();
        assert_eq!(log.scroll_offset, 1);
    }

    #[test]
    fn scroll_up_does_not_go_below_zero() {
        let mut log = ActivityLog::new(100);
        log.scroll_up();
        assert_eq!(log.scroll_offset, 0);
    }

    #[test]
    fn entries_empty_for_new_log() {
        let log = ActivityLog::new(100);
        assert!(log.entries().is_empty());
    }

    #[test]
    fn entries_returns_all() {
        let mut log = ActivityLog::new(100);
        for i in 0..5 {
            log.push(make_entry(&format!("entry {}", i)));
        }
        assert_eq!(log.entries().len(), 5);
    }

    #[test]
    fn scroll_offset_clamped_after_trim() {
        let mut log = ActivityLog::new(3);
        log.push(make_entry("a"));
        log.push(make_entry("b"));
        log.push(make_entry("c"));
        log.scroll_down();
        log.scroll_down(); // offset = 2
        log.push(make_entry("d")); // trims "a", len=3, max=2
        assert!(log.scroll_offset < log.entries().len());
    }
}
