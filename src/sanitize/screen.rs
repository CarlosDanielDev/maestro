use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::screens::{Screen, ScreenAction};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use super::{Finding, SanitizeReport, Severity};

/// Filter state for severity cycling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeverityFilter {
    All,
    Critical,
    Warning,
    Info,
}

impl SeverityFilter {
    fn next(self) -> Self {
        match self {
            Self::All => Self::Critical,
            Self::Critical => Self::Warning,
            Self::Warning => Self::Info,
            Self::Info => Self::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Critical => "Critical",
            Self::Warning => "Warning",
            Self::Info => "Info",
        }
    }

    fn matches(self, severity: Severity) -> bool {
        match self {
            Self::All => true,
            Self::Critical => severity == Severity::Critical,
            Self::Warning => severity == Severity::Warning,
            Self::Info => severity == Severity::Info,
        }
    }
}

/// TUI screen for browsing sanitize results.
pub struct SanitizeScreen {
    pub report: SanitizeReport,
    pub severity_filter: SeverityFilter,
    pub selected_index: usize,
    /// Pre-sorted findings, computed once and cached.
    sorted_findings: Vec<Finding>,
    /// Indices into sorted_findings that pass the current filter.
    filtered_indices: Vec<usize>,
}

#[allow(dead_code)] // Reason: sanitize TUI screen — to be wired into screen dispatch
impl SanitizeScreen {
    pub fn new(report: SanitizeReport) -> Self {
        let sorted_findings: Vec<Finding> = {
            let refs = report.all_findings();
            refs.into_iter().cloned().collect()
        };
        let mut screen = Self {
            report,
            severity_filter: SeverityFilter::All,
            selected_index: 0,
            sorted_findings,
            filtered_indices: Vec::new(),
        };
        screen.rebuild_filter();
        screen
    }

    fn rebuild_filter(&mut self) {
        self.filtered_indices = self
            .sorted_findings
            .iter()
            .enumerate()
            .filter(|(_, f)| self.severity_filter.matches(f.severity))
            .map(|(i, _)| i)
            .collect();
        if self.selected_index >= self.filtered_indices.len() {
            self.selected_index = self.filtered_indices.len().saturating_sub(1);
        }
    }

    fn selected_finding(&self) -> Option<&Finding> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|&idx| self.sorted_findings.get(idx))
    }

    pub fn finding_count(&self) -> usize {
        self.filtered_indices.len()
    }
}

impl Screen for SanitizeScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Char('q') | KeyCode::Esc => return ScreenAction::Pop,
                KeyCode::Char('j') | KeyCode::Down if !self.filtered_indices.is_empty() => {
                    self.selected_index =
                        (self.selected_index + 1).min(self.filtered_indices.len() - 1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
                KeyCode::Char('s') => {
                    self.severity_filter = self.severity_filter.next();
                    self.rebuild_filter();
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let total = self.sorted_findings.len();
        let dead_lines = self.report.total_dead_lines();

        // Layout: header + content + footer
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(5),    // content
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header_text = format!(
            " {} findings total — {} dead lines — Filter: {} ({} shown) ",
            total,
            dead_lines,
            self.severity_filter.label(),
            self.filtered_indices.len()
        );
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                " SANITIZE ",
                Style::default()
                    .fg(theme.branding_fg)
                    .bg(theme.accent_info)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(header_text, Style::default().fg(theme.text_primary)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_active)),
        );
        f.render_widget(header, chunks[0]);

        // Content: two-panel layout
        if self.filtered_indices.is_empty() {
            let empty = Paragraph::new("No findings matching filter.")
                .style(Style::default().fg(theme.text_muted))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Findings ")
                        .border_style(Style::default().fg(theme.border_inactive)),
                );
            f.render_widget(empty, chunks[1]);
        } else {
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(chunks[1]);

            // Left panel: finding list
            let items: Vec<ListItem> = self
                .filtered_indices
                .iter()
                .enumerate()
                .map(|(i, &idx)| {
                    if let Some(finding) = self.sorted_findings.get(idx) {
                        let icon = match finding.severity {
                            Severity::Critical => "!!",
                            Severity::Warning => " !",
                            Severity::Info => " i",
                        };
                        let color = match finding.severity {
                            Severity::Critical => theme.accent_error,
                            Severity::Warning => theme.accent_warning,
                            Severity::Info => theme.accent_info,
                        };
                        let selected = i == self.selected_index;
                        let style = if selected {
                            Style::default()
                                .fg(color)
                                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                        } else {
                            Style::default().fg(color)
                        };
                        let text = format!(
                            "{} {}:{}",
                            icon,
                            finding.location.file.display(),
                            finding.location.line_start
                        );
                        ListItem::new(text).style(style)
                    } else {
                        ListItem::new("???")
                    }
                })
                .collect();

            let list = List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Findings ")
                    .border_style(Style::default().fg(theme.border_active)),
            );
            f.render_widget(list, panels[0]);

            // Right panel: detail view
            let detail = if let Some(finding) = self.selected_finding() {
                let sev_label = match finding.severity {
                    Severity::Critical => "CRITICAL",
                    Severity::Warning => "WARNING",
                    Severity::Info => "INFO",
                };
                let cat_label = format!("{:?}", finding.category);
                let lines = vec![
                    Line::from(vec![
                        Span::styled("Severity: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::styled(
                            sev_label,
                            Style::default().fg(match finding.severity {
                                Severity::Critical => theme.accent_error,
                                Severity::Warning => theme.accent_warning,
                                Severity::Info => theme.accent_info,
                            }),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Category: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(cat_label),
                    ]),
                    Line::from(vec![
                        Span::styled("Location: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(format!(
                            "{}:{}-{}",
                            finding.location.file.display(),
                            finding.location.line_start,
                            finding.location.line_end
                        )),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Message: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(&finding.message),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            "Dead lines: ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(finding.dead_lines.to_string()),
                    ]),
                ];
                Paragraph::new(lines).wrap(Wrap { trim: false })
            } else {
                Paragraph::new("No finding selected.")
            };

            let detail_widget = detail.block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Detail ")
                    .border_style(Style::default().fg(theme.border_active)),
            );
            f.render_widget(detail_widget, panels[1]);
        }

        // Footer: keybindings
        let footer = Line::from(vec![
            Span::styled("[s]", Style::default().fg(theme.keybind_key)),
            Span::raw("everity "),
            Span::styled("[j/k]", Style::default().fg(theme.keybind_key)),
            Span::raw("navigate "),
            Span::styled("[q/Esc]", Style::default().fg(theme.keybind_key)),
            Span::raw("back"),
        ]);
        f.render_widget(Paragraph::new(footer), chunks[2]);
    }
}

impl KeymapProvider for SanitizeScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Sanitize",
            bindings: vec![
                KeyBinding {
                    key: "s",
                    description: "Cycle severity filter",
                },
                KeyBinding {
                    key: "j/k",
                    description: "Navigate findings",
                },
                KeyBinding {
                    key: "q/Esc",
                    description: "Back to dashboard",
                },
            ],
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sanitize::{AnalysisResult, ScanResult, SmellCategory, SourceLocation};
    use std::path::PathBuf;

    fn make_finding(severity: Severity, file: &str) -> Finding {
        Finding {
            severity,
            category: SmellCategory::UnusedFunction,
            location: SourceLocation {
                file: PathBuf::from(file),
                line_start: 10,
                line_end: 20,
            },
            message: format!("test for {}", file),
            dead_lines: 5,
        }
    }

    fn make_report(findings: Vec<Finding>) -> SanitizeReport {
        SanitizeReport {
            scan: ScanResult { findings },
            analysis: AnalysisResult::default(),
        }
    }

    fn key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        })
    }

    #[test]
    fn new_screen_shows_all_findings() {
        let report = make_report(vec![
            make_finding(Severity::Critical, "a.rs"),
            make_finding(Severity::Info, "b.rs"),
        ]);
        let screen = SanitizeScreen::new(report);
        assert_eq!(screen.finding_count(), 2);
        assert_eq!(screen.selected_index, 0);
    }

    #[test]
    fn severity_filter_cycles_correctly() {
        let report = make_report(vec![
            make_finding(Severity::Critical, "a.rs"),
            make_finding(Severity::Warning, "b.rs"),
            make_finding(Severity::Info, "c.rs"),
        ]);
        let mut screen = SanitizeScreen::new(report);

        assert_eq!(screen.severity_filter, SeverityFilter::All);
        assert_eq!(screen.finding_count(), 3);

        // Press 's' to cycle to Critical
        screen.handle_input(&key_event(KeyCode::Char('s')), InputMode::Normal);
        assert_eq!(screen.severity_filter, SeverityFilter::Critical);
        assert_eq!(screen.finding_count(), 1);

        // Press 's' to cycle to Warning
        screen.handle_input(&key_event(KeyCode::Char('s')), InputMode::Normal);
        assert_eq!(screen.severity_filter, SeverityFilter::Warning);
        assert_eq!(screen.finding_count(), 1);

        // Press 's' to cycle to Info
        screen.handle_input(&key_event(KeyCode::Char('s')), InputMode::Normal);
        assert_eq!(screen.severity_filter, SeverityFilter::Info);
        assert_eq!(screen.finding_count(), 1);

        // Press 's' to cycle back to All
        screen.handle_input(&key_event(KeyCode::Char('s')), InputMode::Normal);
        assert_eq!(screen.severity_filter, SeverityFilter::All);
        assert_eq!(screen.finding_count(), 3);
    }

    #[test]
    fn navigation_increments_and_decrements() {
        let report = make_report(vec![
            make_finding(Severity::Critical, "a.rs"),
            make_finding(Severity::Warning, "b.rs"),
            make_finding(Severity::Info, "c.rs"),
        ]);
        let mut screen = SanitizeScreen::new(report);

        assert_eq!(screen.selected_index, 0);

        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_index, 1);

        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_index, 2);

        // Should not go past end
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_index, 2);

        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_index, 1);

        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_index, 0);

        // Should not go below 0
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_index, 0);
    }

    #[test]
    fn empty_report_shows_zero_findings() {
        let screen = SanitizeScreen::new(SanitizeReport::default());
        assert_eq!(screen.finding_count(), 0);
    }

    #[test]
    fn filter_to_empty_shows_no_findings() {
        let report = make_report(vec![make_finding(Severity::Info, "a.rs")]);
        let mut screen = SanitizeScreen::new(report);

        // Filter to Critical — no findings match
        screen.handle_input(&key_event(KeyCode::Char('s')), InputMode::Normal);
        assert_eq!(screen.severity_filter, SeverityFilter::Critical);
        assert_eq!(screen.finding_count(), 0);
    }

    #[test]
    fn q_returns_pop_action() {
        let mut screen = SanitizeScreen::new(SanitizeReport::default());
        let action = screen.handle_input(&key_event(KeyCode::Char('q')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn esc_returns_pop_action() {
        let mut screen = SanitizeScreen::new(SanitizeReport::default());
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn selected_index_clamps_when_filter_shrinks() {
        let report = make_report(vec![
            make_finding(Severity::Critical, "a.rs"),
            make_finding(Severity::Info, "b.rs"),
            make_finding(Severity::Info, "c.rs"),
        ]);
        let mut screen = SanitizeScreen::new(report);

        // Navigate to last item
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_index, 2);

        // Filter to Critical — only 1 item, index should clamp
        screen.handle_input(&key_event(KeyCode::Char('s')), InputMode::Normal);
        assert_eq!(screen.finding_count(), 1);
        assert_eq!(screen.selected_index, 0);
    }
}
