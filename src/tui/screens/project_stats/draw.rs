use super::ProjectStatsScreen;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table},
};

impl ProjectStatsScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, _theme: &Theme) {
        if self.loading {
            let msg = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Loading project stats…",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
            ])
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Project Stats"),
            );
            f.render_widget(msg, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(self.milestones_height()),
                Constraint::Length(7),
                Constraint::Length(6),
                Constraint::Min(3),
            ])
            .split(area);

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Project Stats",
                Style::default().add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            chunks[0],
        );

        self.draw_milestones(f, chunks[1]);
        self.draw_issues(f, chunks[2]);
        self.draw_sessions(f, chunks[3]);
        self.draw_recent(f, chunks[4]);
    }

    fn milestones_height(&self) -> u16 {
        let body = self.data.milestones.len() as u16;
        body.saturating_add(2).clamp(3, 12)
    }

    fn draw_milestones(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Milestones");
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.data.milestones.is_empty() {
            f.render_widget(
                Paragraph::new("No open milestones.").alignment(Alignment::Center),
                inner,
            );
            return;
        }

        let rows: Vec<Constraint> = self
            .data
            .milestones
            .iter()
            .take(inner.height as usize)
            .map(|_| Constraint::Length(1))
            .collect();
        if rows.is_empty() {
            return;
        }
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(rows)
            .split(inner);

        for (i, ms) in self.data.milestones.iter().take(chunks.len()).enumerate() {
            let label = format!(
                "{} ({}/{})",
                truncate_label(&ms.title, 40),
                ms.closed,
                ms.total
            );
            let gauge = Gauge::default().ratio(ms.ratio()).label(format!(
                "{:>3}%  {}",
                ms.percent(),
                label
            ));
            f.render_widget(gauge, chunks[i]);
        }
    }

    fn draw_issues(&self, f: &mut Frame, area: Rect) {
        let i = &self.data.issues;
        let header = Row::new(vec!["Open", "Closed", "ready", "done", "failed"])
            .style(Style::default().add_modifier(Modifier::BOLD));
        let row = Row::new(vec![
            Cell::from(i.open.to_string()),
            Cell::from(i.closed.to_string()),
            Cell::from(i.ready.to_string()),
            Cell::from(i.done.to_string()),
            Cell::from(i.failed.to_string()),
        ]);
        let widths = [
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
        ];
        let table = Table::new(vec![row], widths)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Issues"));
        f.render_widget(table, area);
    }

    fn draw_sessions(&self, f: &mut Frame, area: Rect) {
        let m = &self.data.sessions;
        let lines = vec![
            Line::from(format!(
                "Total: {}   Completed: {}   Success rate: {:.0}%",
                m.total_sessions,
                m.completed_sessions,
                m.success_rate() * 100.0
            )),
            Line::from(format!(
                "Cost: ${:.4}   Tokens in/out: {} / {}",
                m.total_cost_usd, m.total_input_tokens, m.total_output_tokens
            )),
        ];
        f.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Sessions")),
            area,
        );
    }

    fn draw_recent(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Recent activity");
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.data.recent_activity.is_empty() {
            f.render_widget(
                Paragraph::new("No recent sessions.").alignment(Alignment::Center),
                inner,
            );
            return;
        }

        let visible_height = inner.height as usize;
        let rows: Vec<Line> = self
            .data
            .recent_activity
            .iter()
            .skip(self.scroll_offset)
            .take(visible_height)
            .map(|r| {
                let issue = r
                    .issue_number
                    .map(|n| format!("#{:<5}", n))
                    .unwrap_or_else(|| "      ".to_string());
                Line::from(format!(
                    "{} {:<32} {:<10} ${:<7.4} {}",
                    issue,
                    truncate_label(&r.label, 32),
                    truncate_label(&r.status, 10),
                    r.cost_usd,
                    r.elapsed
                ))
            })
            .collect();
        f.render_widget(Paragraph::new(rows), inner);
    }
}

fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
