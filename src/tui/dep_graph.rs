use crate::tui::theme::Theme;
use crate::work::assigner::WorkAssigner;
use crate::work::types::WorkStatus;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

/// Render an ASCII dependency graph visualization.
pub fn draw_dep_graph(f: &mut Frame, assigner: Option<&WorkAssigner>, area: Rect, theme: &Theme) {
    let lines = match assigner {
        Some(assigner) => build_graph_lines(assigner, theme),
        None => vec![Line::from(Span::styled(
            " No work assigner active (prompt-only mode)",
            Style::default().fg(theme.text_secondary),
        ))],
    };

    let block = theme
        .styled_block("Dependency Graph", false)
        .border_style(Style::default().fg(theme.accent_info));

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn build_graph_lines<'a>(assigner: &WorkAssigner, theme: &Theme) -> Vec<Line<'a>> {
    let items = assigner.all_items();
    if items.is_empty() {
        return vec![Line::from(" No work items")];
    }

    let mut lines = Vec::new();

    for item in items {
        let status_color = match item.status {
            WorkStatus::Pending | WorkStatus::Blocked => theme.text_secondary,
            WorkStatus::InProgress => theme.accent_success,
            WorkStatus::Done => theme.status_completed,
            WorkStatus::Failed => theme.accent_error,
        };

        let status_symbol = match item.status {
            WorkStatus::Pending => "○",
            WorkStatus::Blocked => "⊘",
            WorkStatus::InProgress => "●",
            WorkStatus::Done => "✓",
            WorkStatus::Failed => "✗",
        };

        let deps_str = if item.blocked_by.is_empty() {
            String::new()
        } else {
            let deps: Vec<String> = item.blocked_by.iter().map(|d| format!("#{}", d)).collect();
            format!(" ← [{}]", deps.join(", "))
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", status_symbol),
                Style::default().fg(status_color),
            ),
            Span::styled(
                format!("#{:<4}", item.number()),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                format!(" {:?} ", item.priority),
                Style::default().fg(theme.accent_warning),
            ),
            Span::styled(item.title().to_string(), Style::default().fg(status_color)),
            Span::styled(deps_str, Style::default().fg(theme.text_secondary)),
        ]));
    }

    lines
}
