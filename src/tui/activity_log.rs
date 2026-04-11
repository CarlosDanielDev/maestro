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
pub struct ToolMeta {
    pub tool_name: String,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub session_label: String,
    pub message: String,
    pub level: LogLevel,
    pub tool_meta: Option<ToolMeta>,
}

/// Map a tool name to an ASCII prefix icon for the activity log.
pub fn tool_icon_ascii(tool_name: &str) -> &'static str {
    match tool_name {
        "Read" => "[R]",
        "Write" => "[W]",
        "Edit" => "[E]",
        "Bash" => "[$]",
        "Grep" => "[?]",
        "Glob" => "[*]",
        "WebFetch" => "[@]",
        _ => "[~]",
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Info,
    Tool,
    #[allow(dead_code)] // Reason: thinking event display — to be wired into stream event handler
    Thinking,
    Warn,
    Error,
}

impl LogLevel {
    pub fn color(&self, theme: &Theme) -> Color {
        match self {
            Self::Info => theme.text_primary,
            Self::Tool => theme.accent_info,
            Self::Thinking => theme.accent_success,
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
            tool_meta: None,
        });
    }

    pub fn push_tool(
        &mut self,
        label: String,
        message: String,
        level: LogLevel,
        tool_name: String,
    ) {
        self.push(LogEntry {
            timestamp: Utc::now(),
            session_label: label,
            message,
            level,
            tool_meta: Some(ToolMeta { tool_name }),
        });
    }

    #[allow(dead_code)] // Reason: public API for future activity log scroll keybindings
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    #[allow(dead_code)] // Reason: public API for future activity log scroll keybindings
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
                let mut spans = vec![
                    Span::styled(
                        format!("{} ", time),
                        Style::default().fg(theme.text_secondary),
                    ),
                    Span::styled(
                        format!("[{}] ", entry.session_label),
                        Style::default().fg(theme.accent_identifier),
                    ),
                ];

                if let Some(ref meta) = entry.tool_meta {
                    spans.push(Span::styled(
                        format!("{} ", tool_icon_ascii(&meta.tool_name)),
                        Style::default().fg(theme.accent_info),
                    ));
                }

                spans.push(Span::styled(
                    entry.message.clone(),
                    Style::default().fg(entry.level.color(theme)),
                ));

                ListItem::new(Line::from(spans))
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
            tool_meta: None,
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

    // --- Issue #102: LogLevel::Thinking tests ---

    #[test]
    fn log_level_thinking_has_a_color_that_does_not_panic() {
        let theme = Theme::default();
        let _ = LogLevel::Thinking.color(&theme);
    }

    #[test]
    fn log_level_thinking_color_differs_from_error_color() {
        let theme = Theme::default();
        let thinking_color = LogLevel::Thinking.color(&theme);
        let error_color = LogLevel::Error.color(&theme);
        assert_ne!(
            thinking_color, error_color,
            "Thinking log level must use a visually distinct color from Error"
        );
    }

    #[test]
    fn log_entry_with_thinking_level_is_stored_and_retrievable() {
        let mut log = ActivityLog::new(100);
        log.push(LogEntry {
            timestamp: Utc::now(),
            session_label: "S-test".to_string(),
            message: "Thinking block started".to_string(),
            level: LogLevel::Thinking,
            tool_meta: None,
        });
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].level, LogLevel::Thinking);
    }

    #[test]
    fn push_simple_with_thinking_level_stores_correctly() {
        let mut log = ActivityLog::new(100);
        log.push_simple(
            "S-1".to_string(),
            "Thought for 3s".to_string(),
            LogLevel::Thinking,
        );
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].level, LogLevel::Thinking);
        assert_eq!(log.entries()[0].message, "Thought for 3s");
    }

    // --- Issue #200: Tool icon and tool metadata tests ---

    #[test]
    fn tool_icon_ascii_maps_known_tools() {
        assert_eq!(tool_icon_ascii("Read"), "[R]");
        assert_eq!(tool_icon_ascii("Write"), "[W]");
        assert_eq!(tool_icon_ascii("Edit"), "[E]");
        assert_eq!(tool_icon_ascii("Bash"), "[$]");
        assert_eq!(tool_icon_ascii("Grep"), "[?]");
        assert_eq!(tool_icon_ascii("Glob"), "[*]");
        assert_eq!(tool_icon_ascii("WebFetch"), "[@]");
    }

    #[test]
    fn tool_icon_ascii_unknown_tool_returns_generic() {
        assert_eq!(tool_icon_ascii("SomeNewTool"), "[~]");
        assert_eq!(tool_icon_ascii(""), "[~]");
    }

    #[test]
    fn push_simple_has_no_tool_meta() {
        let mut log = ActivityLog::new(100);
        log.push_simple("S-1".into(), "test".into(), LogLevel::Info);
        assert!(log.entries()[0].tool_meta.is_none());
    }

    #[test]
    fn push_tool_includes_tool_meta() {
        let mut log = ActivityLog::new(100);
        log.push_tool(
            "S-1".into(),
            "Read: /foo".into(),
            LogLevel::Tool,
            "Read".into(),
        );
        assert!(log.entries()[0].tool_meta.is_some());
        assert_eq!(
            log.entries()[0].tool_meta.as_ref().unwrap().tool_name,
            "Read"
        );
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
