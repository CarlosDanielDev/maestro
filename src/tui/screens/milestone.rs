use super::{Screen, ScreenAction, SessionConfig, draw_keybinds_bar, sanitize_for_terminal};
use crate::provider::github::types::{GhIssue, GhMilestone};
use crate::tui::app::TuiMode;
use crate::tui::icons::{self, IconId};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::panels::compact_gauge_bar_counts;
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MilestoneEntry {
    pub number: u64,
    pub title: String,
    pub description: String,
    pub state: String,
    pub open_issues: u32,
    pub closed_issues: u32,
    pub issues: Vec<GhIssue>,
}

impl From<(GhMilestone, Vec<GhIssue>)> for MilestoneEntry {
    fn from((ms, issues): (GhMilestone, Vec<GhIssue>)) -> Self {
        Self {
            number: ms.number,
            title: ms.title,
            description: ms.description,
            state: ms.state,
            open_issues: ms.open_issues,
            closed_issues: ms.closed_issues,
            issues,
        }
    }
}

impl MilestoneEntry {
    pub fn progress_ratio(&self) -> f64 {
        let total = self.open_issues as f64 + self.closed_issues as f64;
        if total == 0.0 {
            return 0.0;
        }
        self.closed_issues as f64 / total
    }

    pub fn total_issues(&self) -> u32 {
        self.open_issues + self.closed_issues
    }
}

/// Right-pane tabs in the compact milestone view (#325).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MilestoneTab {
    /// Scrollable list of issues belonging to the selected milestone.
    #[default]
    IssueList,
    /// Markdown preview of the focused issue.
    IssuePreview,
}

impl MilestoneTab {
    pub fn cycle(self) -> Self {
        match self {
            Self::IssueList => Self::IssuePreview,
            Self::IssuePreview => Self::IssueList,
        }
    }
}

pub struct MilestoneScreen {
    pub(crate) milestones: Vec<MilestoneEntry>,
    pub(crate) selected: usize,
    scroll_offset: usize,
    pub(crate) loading: bool,
    /// Last known visible slots from draw, used for scroll sync.
    last_visible_slots: usize,
    /// Active tab in the right pane (#325).
    pub active_tab: MilestoneTab,
    /// Focused issue index within the right-pane Issue List tab (#325).
    pub focused_issue: usize,
}

impl MilestoneScreen {
    pub fn new(milestones: Vec<MilestoneEntry>) -> Self {
        Self {
            milestones,
            selected: 0,
            scroll_offset: 0,
            loading: false,
            last_visible_slots: 6,
            active_tab: MilestoneTab::default(),
            focused_issue: 0,
        }
    }

    /// Cycle to the next right-pane tab. Returns the new tab.
    pub fn cycle_tab(&mut self) -> MilestoneTab {
        self.active_tab = self.active_tab.cycle();
        self.active_tab
    }

    pub fn set_tab(&mut self, tab: MilestoneTab) {
        self.active_tab = tab;
    }

    /// Issues for the selected milestone, sorted ascending by the count of
    /// `#NNN` references inside their `## Blocked By` section. Issues with
    /// no parsed dependencies sort first (Level 0). Ties break on issue
    /// number ascending.
    pub fn sorted_issues(&self) -> Vec<&GhIssue> {
        let Some(entry) = self.milestones.get(self.selected) else {
            return Vec::new();
        };
        let mut with_levels: Vec<(usize, &GhIssue)> = entry
            .issues
            .iter()
            .map(|i| (count_blocked_by(&i.body), i))
            .collect();
        with_levels.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.number.cmp(&b.1.number)));
        with_levels.into_iter().map(|(_, i)| i).collect()
    }

    fn focused_issue_clamped(&self) -> usize {
        let len = self.sorted_issues().len();
        if len == 0 {
            0
        } else {
            self.focused_issue.min(len - 1)
        }
    }

    fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        // #325 compact layout: left = milestone list, right = tabbed
        // (issue list / preview). Bottom row is keybindings.
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(1)])
            .split(area);

        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(outer[0]);

        self.draw_milestone_list(f, body[0], theme);
        self.draw_right_pane(f, body[1], theme);
        draw_keybinds_bar(
            f,
            outer[1],
            &[
                ("Enter", "View Issues"),
                ("Tab", "Switch Tab"),
                ("r", "Run All Open"),
                ("Esc", "Back"),
            ],
            theme,
        );
    }

    fn draw_right_pane(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(area);
        self.draw_tab_bar(f, chunks[0], theme);
        match self.active_tab {
            MilestoneTab::IssueList => self.draw_tab_issue_list(f, chunks[1], theme),
            MilestoneTab::IssuePreview => self.draw_tab_issue_preview(f, chunks[1], theme),
        }
    }

    fn draw_tab_bar(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mk = |label: &'static str, key: char, tab: MilestoneTab| {
            let active = self.active_tab == tab;
            let style = if active {
                Style::default()
                    .fg(theme.accent_identifier)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(theme.text_secondary)
            };
            Span::styled(format!("[{}] {}  ", key, label), style)
        };
        let line = Line::from(vec![
            mk("Issues", '1', MilestoneTab::IssueList),
            mk("Preview", '2', MilestoneTab::IssuePreview),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }

    fn draw_tab_issue_list(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Issues", false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let issues = self.sorted_issues();
        if issues.is_empty() {
            let msg = if let Some(entry) = self.milestones.get(self.selected) {
                format!("  {} — no issues loaded", entry.title)
            } else {
                "  Select a milestone".to_string()
            };
            f.render_widget(
                Paragraph::new(msg).style(Style::default().fg(theme.text_secondary)),
                inner,
            );
            return;
        }

        let focused = self.focused_issue_clamped();
        let lines: Vec<Line> = issues
            .iter()
            .take(inner.height as usize)
            .enumerate()
            .map(|(i, issue)| {
                let (symbol, symbol_color) = if issue.state == "closed" {
                    (icons::get(IconId::IssueClosed), theme.accent_success)
                } else {
                    (icons::get(IconId::IssueOpened), theme.accent_warning)
                };
                let cursor = if i == focused { ">" } else { " " };
                Line::from(vec![
                    Span::raw(format!("{} ", cursor)),
                    Span::styled(format!("{} ", symbol), Style::default().fg(symbol_color)),
                    Span::styled(
                        format!("#{} ", issue.number),
                        Style::default()
                            .fg(theme.accent_identifier)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        sanitize_for_terminal(&issue.title),
                        Style::default().fg(theme.text_secondary),
                    ),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_tab_issue_preview(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Preview", false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let issues = self.sorted_issues();
        let focused = self.focused_issue_clamped();
        let Some(issue) = issues.get(focused) else {
            f.render_widget(
                Paragraph::new("  No issue to preview")
                    .style(Style::default().fg(theme.text_secondary)),
                inner,
            );
            return;
        };

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(
                format!("#{}  ", issue.number),
                Style::default()
                    .fg(theme.accent_identifier)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                sanitize_for_terminal(&issue.title),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));
        for raw in issue.body.lines().take(inner.height.saturating_sub(2) as usize) {
            lines.push(Line::from(sanitize_for_terminal(raw)));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }

    #[allow(dead_code)]
    #[allow(clippy::needless_pass_by_ref_mut)] // Reason: &mut reserved for future tick-driven state mutations
    pub fn tick(&mut self) {}

    pub fn selected_milestone(&self) -> Option<&MilestoneEntry> {
        self.milestones.get(self.selected)
    }

    fn sync_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.last_visible_slots {
            self.scroll_offset = self.selected - self.last_visible_slots + 1;
        }
    }

    /// #326: open the Issue Wizard pre-filled with the selected milestone
    /// and a suggested Blocked By list derived from the milestone's open
    /// issues.
    fn handle_create_issue(&self) -> ScreenAction {
        let Some(entry) = self.milestones.get(self.selected) else {
            return ScreenAction::None;
        };
        let suggested = suggest_blocked_by_for_new_issue(&entry.issues);
        ScreenAction::OpenIssueWizardForMilestone {
            milestone: entry.number,
            suggested_blocked_by: suggested,
        }
    }

    fn handle_run_all(&self) -> ScreenAction {
        if let Some(entry) = self.milestones.get(self.selected) {
            if entry.issues.is_empty() {
                return ScreenAction::None;
            }
            let configs: Vec<SessionConfig> = entry
                .issues
                .iter()
                .filter(|i| i.state == "open")
                .map(|i| SessionConfig {
                    issue_number: Some(i.number),
                    title: i.title.clone(),
                    custom_prompt: None,
                })
                .collect();
            if configs.is_empty() {
                return ScreenAction::None;
            }
            return ScreenAction::LaunchSessions(configs);
        }
        ScreenAction::None
    }

    fn draw_milestone_list(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let title = format!("{} Milestones", icons::get(IconId::Milestone));
        let block = theme
            .styled_block(&title, false)
            .border_style(Style::default().fg(theme.border_active));

        if self.loading {
            let para = Paragraph::new("  Loading...")
                .style(Style::default().fg(theme.accent_warning))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        if self.milestones.is_empty() {
            let para = Paragraph::new("  No milestones found")
                .style(Style::default().fg(theme.text_secondary))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        let inner = block.inner(area);
        f.render_widget(block, area);

        let visible_slots = (inner.height as usize) / 3;
        self.last_visible_slots = visible_slots.max(1);
        let milestones_to_show: Vec<(usize, &MilestoneEntry)> = self
            .milestones
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(visible_slots)
            .collect();

        for (display_idx, (idx, entry)) in milestones_to_show.iter().enumerate() {
            let y = inner.y + (display_idx * 3) as u16;
            if y + 2 >= inner.y + inner.height {
                break;
            }

            let is_selected = *idx == self.selected;
            let cursor = if is_selected {
                format!("{} ", icons::get(IconId::ChevronRight))
            } else {
                "  ".to_string()
            };

            let title_style = if is_selected {
                Style::default()
                    .fg(theme.selection_fg)
                    .bg(theme.selection_bg)
                    .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK)
            } else {
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            };

            let title_line = Line::from(vec![
                Span::styled(cursor, title_style),
                Span::styled(format!("{} ", icons::get(IconId::Milestone)), title_style),
                Span::styled(sanitize_for_terminal(&entry.title), title_style),
            ]);
            let title_area = Rect::new(inner.x, y, inner.width, 1);
            f.render_widget(Paragraph::new(title_line), title_area);

            let ratio = entry.progress_ratio();
            let pct = ratio * 100.0;
            let gauge_area = Rect::new(inner.x + 2, y + 1, inner.width.saturating_sub(4), 1);
            let bar_width = gauge_area.width.saturating_sub(20) as usize;
            let (filled, empty) = compact_gauge_bar_counts(pct, bar_width);
            let gauge_color = theme.milestone_gauge_color(pct);
            let gauge_line = Line::from(vec![
                Span::styled("[", Style::default().fg(gauge_color)),
                Span::styled(
                    icons::get(IconId::GaugeFilled).repeat(filled),
                    Style::default().fg(gauge_color),
                ),
                Span::styled(
                    icons::get(IconId::GaugeEmpty).repeat(empty),
                    Style::default().fg(theme.gauge_background),
                ),
                Span::styled(
                    format!(
                        "] {}/{} issues ({:.0}%)",
                        entry.closed_issues,
                        entry.total_issues(),
                        pct
                    ),
                    Style::default().fg(gauge_color),
                ),
            ]);
            f.render_widget(Paragraph::new(gauge_line), gauge_area);

            let status_line = Line::from(vec![
                Span::styled(
                    format!("  {} ", icons::get(IconId::IssueClosed)),
                    Style::default().fg(theme.accent_success),
                ),
                Span::styled(
                    entry.closed_issues.to_string(),
                    Style::default()
                        .fg(theme.accent_success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("    "),
                Span::styled(
                    format!("{} ", icons::get(IconId::IssueOpened)),
                    Style::default().fg(theme.accent_warning),
                ),
                Span::styled(
                    entry.open_issues.to_string(),
                    Style::default().fg(theme.accent_warning),
                ),
            ]);
            let status_area = Rect::new(inner.x, y + 2, inner.width, 1);
            f.render_widget(Paragraph::new(status_line), status_area);
        }
    }

}

/// #326: suggest a `Blocked By` list for a new issue inserted into a
/// milestone. Picks the open issues at the deepest existing dependency
/// level — typically a leaf of the current chain — so a new "next step"
/// issue is wired up correctly without manual graph editing.
pub fn suggest_blocked_by_for_new_issue(issues: &[GhIssue]) -> Vec<u64> {
    let open: Vec<&GhIssue> = issues.iter().filter(|i| i.state == "open").collect();
    if open.is_empty() {
        return Vec::new();
    }
    let max_level = open
        .iter()
        .map(|i| count_blocked_by(&i.body))
        .max()
        .unwrap_or(0);
    let mut leaves: Vec<u64> = open
        .into_iter()
        .filter(|i| count_blocked_by(&i.body) == max_level)
        .map(|i| i.number)
        .collect();
    leaves.sort();
    leaves
}

/// #326: insert a new issue into the milestone's `## Dependency Graph`
/// section. Adds a new "Level N" if `blocked_by` is non-empty and the
/// referenced issues live in the previous level; otherwise appends to
/// Level 0.
pub fn update_milestone_dependency_graph(
    description: &str,
    new_issue_number: u64,
    new_issue_title: &str,
    blocked_by: &[u64],
) -> String {
    let bullet = format!("• #{} {}", new_issue_number, new_issue_title);

    // Locate the dep-graph section. If absent, append a fresh one.
    let Some(start) = find_section_start(description, "## Dependency Graph") else {
        let mut out = description.to_string();
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("\n## Dependency Graph (Implementation Order)\n\n");
        out.push_str(if blocked_by.is_empty() {
            "Level 0 — no dependencies:\n"
        } else {
            "Level 1 — depends on prior issues:\n"
        });
        out.push_str(&bullet);
        out.push('\n');
        return out;
    };

    // Find the section's body and split off the trailing content.
    let after = &description[start..];
    let (section_body, rest) = match after.find("\n## ") {
        Some(idx) => (&after[..idx], &after[idx..]),
        None => (after, ""),
    };

    let updated = if blocked_by.is_empty() {
        insert_into_level_zero(section_body, &bullet)
    } else {
        append_new_level(section_body, &bullet, blocked_by)
    };

    let mut out = String::with_capacity(description.len() + bullet.len() + 64);
    out.push_str(&description[..start]);
    out.push_str(&updated);
    out.push_str(rest);
    out
}

fn find_section_start(description: &str, header: &str) -> Option<usize> {
    description
        .lines()
        .scan(0usize, |pos, line| {
            let p = *pos;
            *pos += line.len() + 1;
            Some((p, line))
        })
        .find(|(_, line)| line.trim_start().starts_with(header))
        .map(|(p, _)| p)
}

fn insert_into_level_zero(section: &str, bullet: &str) -> String {
    let mut out = String::with_capacity(section.len() + bullet.len() + 1);
    let mut found = false;
    for (i, line) in section.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line);
        if !found && line.trim_start().starts_with("Level 0") {
            out.push('\n');
            out.push_str(bullet);
            found = true;
        }
    }
    if !found {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("\nLevel 0 — no dependencies:\n");
        out.push_str(bullet);
    }
    if !section.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn append_new_level(section: &str, bullet: &str, blocked_by: &[u64]) -> String {
    // Find the highest existing level number.
    let max_level = section
        .lines()
        .filter_map(|l| {
            let trimmed = l.trim_start();
            trimmed.strip_prefix("Level ").and_then(|rest| {
                rest.split_whitespace().next().and_then(|n| n.parse::<u32>().ok())
            })
        })
        .max()
        .unwrap_or(0);
    let new_level = max_level + 1;
    let dep_list = blocked_by
        .iter()
        .map(|n| format!("#{}", n))
        .collect::<Vec<_>>()
        .join(", ");

    let mut out = section.trim_end().to_string();
    out.push('\n');
    out.push_str(&format!(
        "\nLevel {} — depends on {}:\n{}\n",
        new_level, dep_list, bullet
    ));
    out
}

/// Counts `#NNN` references inside the `## Blocked By` section of an issue
/// body. Used by `MilestoneScreen::sorted_issues` to approximate the
/// dependency level — Level 0 = no dependencies, Level N = depends on N
/// other issues. Returns 0 if the section is missing or contains "None".
pub(crate) fn count_blocked_by(body: &str) -> usize {
    let mut in_section = false;
    let mut count = 0usize;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_section = trimmed.eq_ignore_ascii_case("## blocked by");
            continue;
        }
        if !in_section {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        // Stop counting once we hit the next section.
        if trimmed.starts_with('#') && !trimmed.starts_with("#") {
            break;
        }
        // A line like "- #123 title" or "* #45 ..." or "- None"
        let after_bullet = trimmed.trim_start_matches(['-', '*', ' ']);
        if after_bullet.starts_with('#') {
            // Skip the '#' and check the next char is a digit.
            let after_hash = &after_bullet[1..];
            if after_hash.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                count += 1;
            }
        }
    }
    count
}

impl KeymapProvider for MilestoneScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Milestones",
            bindings: vec![
                KeyBinding {
                    key: "j/Down",
                    description: "Move down",
                },
                KeyBinding {
                    key: "k/Up",
                    description: "Move up",
                },
                KeyBinding {
                    key: "Enter",
                    description: "View issues",
                },
                KeyBinding {
                    key: "r",
                    description: "Run all open issues",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Back",
                },
            ],
        }]
    }
}

impl Screen for MilestoneScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Esc => return ScreenAction::Pop,
                KeyCode::Tab => {
                    self.cycle_tab();
                }
                KeyCode::Char('1') => {
                    self.set_tab(MilestoneTab::IssueList);
                }
                KeyCode::Char('2') => {
                    self.set_tab(MilestoneTab::IssuePreview);
                }
                KeyCode::Char('J') => {
                    let len = self.sorted_issues().len();
                    if len > 0 && self.focused_issue + 1 < len {
                        self.focused_issue += 1;
                    }
                }
                KeyCode::Char('K') => {
                    self.focused_issue = self.focused_issue.saturating_sub(1);
                }
                KeyCode::Char('j') | KeyCode::Down
                    if !self.milestones.is_empty() && self.selected < self.milestones.len() - 1 =>
                {
                    self.selected += 1;
                    self.focused_issue = 0;
                    self.sync_scroll();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    let prev = self.selected;
                    self.selected = self.selected.saturating_sub(1);
                    if self.selected != prev {
                        self.focused_issue = 0;
                    }
                    self.sync_scroll();
                }
                KeyCode::Enter => {
                    if self.milestones.is_empty() {
                        return ScreenAction::None;
                    }
                    return ScreenAction::Push(TuiMode::IssueBrowser);
                }
                KeyCode::Char('r') => {
                    return self.handle_run_all();
                }
                KeyCode::Char('c') => {
                    return self.handle_create_issue();
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        Some(InputMode::Normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;
    use crossterm::event::KeyCode;

    fn make_issue(number: u64) -> GhIssue {
        GhIssue {
            number,
            title: format!("Issue #{}", number),
            body: String::new(),
            labels: vec![],
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{}", number),
            milestone: None,
            assignees: vec![],
        }
    }

    fn make_entry(number: u64, open: u32, closed: u32) -> MilestoneEntry {
        MilestoneEntry {
            number,
            title: format!("Milestone v{}", number),
            description: String::new(),
            state: "open".to_string(),
            open_issues: open,
            closed_issues: closed,
            issues: vec![],
        }
    }

    fn make_entry_with_issues(number: u64, issues: Vec<GhIssue>) -> MilestoneEntry {
        let open = issues.len() as u32;
        MilestoneEntry {
            number,
            title: format!("Milestone v{}", number),
            description: String::new(),
            state: "open".to_string(),
            open_issues: open,
            closed_issues: 0,
            issues,
        }
    }

    // ---- #325 compact view: tab switching and dependency sorting ----

    fn make_issue_with_body(number: u64, body: &str) -> GhIssue {
        let mut i = make_issue(number);
        i.body = body.to_string();
        i
    }

    #[test]
    fn new_screen_starts_on_issue_list_tab() {
        let s = MilestoneScreen::new(vec![]);
        assert_eq!(s.active_tab, MilestoneTab::IssueList);
    }

    #[test]
    fn tab_key_cycles_to_preview() {
        let mut s = MilestoneScreen::new(vec![make_entry(1, 0, 0)]);
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        assert_eq!(s.active_tab, MilestoneTab::IssuePreview);
    }

    #[test]
    fn tab_key_cycles_back_to_list() {
        let mut s = MilestoneScreen::new(vec![make_entry(1, 0, 0)]);
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        assert_eq!(s.active_tab, MilestoneTab::IssueList);
    }

    #[test]
    fn key_2_jumps_to_preview() {
        let mut s = MilestoneScreen::new(vec![make_entry(1, 0, 0)]);
        s.handle_input(&key_event(KeyCode::Char('2')), InputMode::Normal);
        assert_eq!(s.active_tab, MilestoneTab::IssuePreview);
    }

    #[test]
    fn key_1_jumps_to_list() {
        let mut s = MilestoneScreen::new(vec![make_entry(1, 0, 0)]);
        s.set_tab(MilestoneTab::IssuePreview);
        s.handle_input(&key_event(KeyCode::Char('1')), InputMode::Normal);
        assert_eq!(s.active_tab, MilestoneTab::IssueList);
    }

    #[test]
    fn count_blocked_by_returns_zero_for_empty_body() {
        assert_eq!(count_blocked_by(""), 0);
    }

    #[test]
    fn count_blocked_by_returns_zero_for_none() {
        let body = "## Blocked By\n\n- None\n";
        assert_eq!(count_blocked_by(body), 0);
    }

    #[test]
    fn count_blocked_by_counts_issue_references() {
        let body = "## Overview\nx\n\n## Blocked By\n\n- #100 first\n- #101 second\n";
        assert_eq!(count_blocked_by(body), 2);
    }

    #[test]
    fn sorted_issues_orders_level_zero_before_dependent() {
        let issues = vec![
            make_issue_with_body(20, "## Blocked By\n\n- #10 dep\n"),
            make_issue_with_body(10, "## Blocked By\n\n- None\n"),
        ];
        let entry = make_entry_with_issues(1, issues);
        let s = MilestoneScreen::new(vec![entry]);
        let sorted = s.sorted_issues();
        assert_eq!(sorted[0].number, 10);
        assert_eq!(sorted[1].number, 20);
    }

    #[test]
    fn navigating_milestone_resets_focused_issue() {
        let issues = vec![make_issue(10), make_issue(11), make_issue(12)];
        let mut s = MilestoneScreen::new(vec![
            make_entry_with_issues(1, issues),
            make_entry(2, 0, 0),
        ]);
        s.focused_issue = 2;
        s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(s.focused_issue, 0);
    }

    #[test]
    fn shift_j_advances_focused_issue() {
        let issues = vec![make_issue(10), make_issue(11), make_issue(12)];
        let mut s = MilestoneScreen::new(vec![make_entry_with_issues(1, issues)]);
        s.handle_input(&key_event(KeyCode::Char('J')), InputMode::Normal);
        assert_eq!(s.focused_issue, 1);
    }

    #[test]
    fn shift_k_does_not_underflow_focused_issue() {
        let issues = vec![make_issue(10)];
        let mut s = MilestoneScreen::new(vec![make_entry_with_issues(1, issues)]);
        s.handle_input(&key_event(KeyCode::Char('K')), InputMode::Normal);
        assert_eq!(s.focused_issue, 0);
    }

    // ---- #326 create-from-milestone ----

    #[test]
    fn key_c_opens_issue_wizard_for_milestone() {
        let mut s = MilestoneScreen::new(vec![make_entry(7, 0, 0)]);
        let action = s.handle_input(&key_event(KeyCode::Char('c')), InputMode::Normal);
        match action {
            ScreenAction::OpenIssueWizardForMilestone { milestone, .. } => {
                assert_eq!(milestone, 7);
            }
            other => panic!("expected OpenIssueWizardForMilestone, got {:?}", other),
        }
    }

    #[test]
    fn suggest_blocked_by_returns_empty_when_no_open_issues() {
        assert!(suggest_blocked_by_for_new_issue(&[]).is_empty());
    }

    #[test]
    fn suggest_blocked_by_returns_deepest_level_leaves() {
        let issues = vec![
            make_issue_with_body(10, "## Blocked By\n\n- None\n"),
            make_issue_with_body(11, "## Blocked By\n\n- #10\n"),
            make_issue_with_body(12, "## Blocked By\n\n- #10\n- #11\n"),
            make_issue_with_body(13, "## Blocked By\n\n- #10\n- #11\n"),
        ];
        let suggested = suggest_blocked_by_for_new_issue(&issues);
        assert_eq!(suggested, vec![12, 13]);
    }

    #[test]
    fn suggest_blocked_by_skips_closed_issues() {
        let mut closed = make_issue_with_body(10, "## Blocked By\n\n- None\n");
        closed.state = "closed".into();
        let issues = vec![closed, make_issue_with_body(11, "## Blocked By\n\n- None\n")];
        let suggested = suggest_blocked_by_for_new_issue(&issues);
        assert_eq!(suggested, vec![11]);
    }

    #[test]
    fn update_dep_graph_appends_new_level_when_blocked_by_present() {
        let desc = "intro\n\n## Dependency Graph (Implementation Order)\n\nLevel 0 — no dependencies:\n• #10 first\n";
        let updated = update_milestone_dependency_graph(desc, 11, "second", &[10]);
        assert!(updated.contains("Level 1 — depends on #10"));
        assert!(updated.contains("• #11 second"));
        // Must preserve the existing Level 0 entries.
        assert!(updated.contains("• #10 first"));
    }

    #[test]
    fn update_dep_graph_inserts_into_level_zero_when_no_blockers() {
        let desc = "## Dependency Graph (Implementation Order)\n\nLevel 0 — no dependencies:\n• #10 first\n";
        let updated = update_milestone_dependency_graph(desc, 11, "second", &[]);
        assert!(updated.contains("• #10 first"));
        assert!(updated.contains("• #11 second"));
        // No new level header was added.
        assert!(!updated.contains("Level 1"));
    }

    #[test]
    fn update_dep_graph_creates_section_when_missing() {
        let desc = "no graph here";
        let updated = update_milestone_dependency_graph(desc, 11, "first", &[]);
        assert!(updated.contains("## Dependency Graph (Implementation Order)"));
        assert!(updated.contains("• #11 first"));
    }

    // ---- initial state ----

    #[test]
    fn milestone_screen_initial_selected_is_zero() {
        let screen = MilestoneScreen::new(vec![make_entry(1, 3, 7), make_entry(2, 1, 2)]);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn milestone_screen_loading_flag_initially_false() {
        let screen = MilestoneScreen::new(vec![make_entry(1, 0, 5)]);
        assert!(!screen.loading);
    }

    // ---- navigation ----

    #[test]
    fn milestone_screen_key_j_advances_cursor() {
        let mut screen = MilestoneScreen::new(vec![
            make_entry(1, 0, 0),
            make_entry(2, 0, 0),
            make_entry(3, 0, 0),
        ]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn milestone_screen_key_down_advances_cursor() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 0), make_entry(2, 0, 0)]);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn milestone_screen_key_k_moves_cursor_up() {
        let mut screen = MilestoneScreen::new(vec![
            make_entry(1, 0, 0),
            make_entry(2, 0, 0),
            make_entry(3, 0, 0),
        ]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn milestone_screen_key_up_moves_cursor_up() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 0), make_entry(2, 0, 0)]);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn milestone_screen_cursor_does_not_underflow() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 0), make_entry(2, 0, 0)]);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn milestone_screen_cursor_does_not_overflow() {
        let mut screen = MilestoneScreen::new(vec![
            make_entry(1, 0, 0),
            make_entry(2, 0, 0),
            make_entry(3, 0, 0),
        ]);
        for _ in 0..10 {
            screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(screen.selected, 2);
    }

    // ---- screen actions ----

    #[test]
    fn milestone_screen_esc_returns_pop() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 0)]);
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn milestone_screen_enter_returns_push_issue_browser_with_milestone_number() {
        let mut screen = MilestoneScreen::new(vec![make_entry(7, 3, 0), make_entry(12, 1, 5)]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::Push(TuiMode::IssueBrowser) => {}
            other => panic!("Expected Push(IssueBrowser), got {:?}", other),
        }
    }

    #[test]
    fn milestone_screen_empty_list_enter_returns_none() {
        let mut screen = MilestoneScreen::new(vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn milestone_screen_key_r_on_milestone_returns_launch_sessions_for_all_open_issues() {
        let issues = vec![make_issue(10), make_issue(11)];
        let mut screen = MilestoneScreen::new(vec![make_entry_with_issues(1, issues)]);
        let action = screen.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
        match action {
            ScreenAction::LaunchSessions(configs) => {
                assert_eq!(configs.len(), 2);
            }
            other => panic!("Expected LaunchSessions, got {:?}", other),
        }
    }

    #[test]
    fn milestone_screen_key_r_on_empty_milestone_returns_none() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 5)]);
        let action = screen.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    // ---- MilestoneEntry::progress_ratio ----

    #[test]
    fn milestone_entry_progress_ratio_computed_correctly() {
        let entry = make_entry(1, 3, 7);
        let ratio = entry.progress_ratio();
        assert!((ratio - 0.7_f64).abs() < f64::EPSILON * 10.0);
    }

    #[test]
    fn milestone_entry_progress_ratio_zero_when_all_open() {
        let entry = make_entry(1, 5, 0);
        assert_eq!(entry.progress_ratio(), 0.0);
    }

    #[test]
    fn milestone_entry_progress_ratio_one_when_all_closed() {
        let entry = make_entry(1, 0, 5);
        assert_eq!(entry.progress_ratio(), 1.0);
    }

    #[test]
    fn milestone_entry_progress_ratio_zero_when_no_issues() {
        let entry = make_entry(1, 0, 0);
        assert_eq!(entry.progress_ratio(), 0.0);
    }

    // ---- tick ----

    #[test]
    fn milestone_screen_tick_does_not_panic() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 2, 3)]);
        screen.tick();
        screen.tick();
        screen.tick();
    }
}
