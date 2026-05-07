use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout as UiLayout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::model::{GraphNode, NodeId, NodeKind};
use super::personalities::{Sprite, glyph_for_role, role_abbrev, role_color};
use super::render::{GraphRenderOptions, graph_block, status_modifier};
use crate::session::types::{Session, SessionStatus};
use crate::tui::spinner::{AnimationPhase, animation_phase, graph_node_frame};
use crate::tui::theme::Theme;

pub(super) fn draw_single_agent_empty_state(
    f: &mut Frame,
    area: Rect,
    nodes: &[GraphNode],
    options: GraphRenderOptions<'_>,
) {
    let theme = options.theme;
    let Some(node) = nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Agent { .. }))
    else {
        draw_no_agents(f, area, theme);
        return;
    };

    let session = match &node.id {
        NodeId::Agent(id) => options.sessions.iter().copied().find(|s| s.id == *id),
        NodeId::File(_) => None,
    };
    let status = match node.kind {
        NodeKind::Agent { status } => status,
        NodeKind::File => SessionStatus::Queued,
    };
    let is_terminal = status.is_terminal();
    let role = session.map(|s| s.role).unwrap_or_default();
    let status_style = Style::default()
        .fg(theme.status_color(status))
        .add_modifier(status_modifier(status));
    let role_style = Style::default().fg(role_color(role));
    let secondary = Style::default().fg(theme.text_secondary);
    let activity = session
        .map(|s| s.current_activity.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or({
            if is_terminal {
                "Session finished"
            } else {
                "Waiting for output"
            }
        });
    let frame = if is_terminal {
        terminal_status_marker(status, options.use_nerd_font).to_string()
    } else {
        graph_node_frame(options.tick, options.use_nerd_font).to_string()
    };
    let phase = session
        .map(|s| phase_label(s.status, s.is_thinking, &s.current_activity))
        .unwrap_or_else(|| status.label().to_ascii_lowercase());

    let block = graph_block(theme);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let visual_height = if options.use_nerd_font { 8 } else { 4 };
    let rows = UiLayout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(visual_height.min(inner.height)),
            Constraint::Length(3.min(inner.height)),
            Constraint::Min(0),
        ])
        .split(inner);

    let sprite = if options.use_nerd_font {
        let mut lines = vec![Line::styled(frame.to_string(), status_style)];
        lines.extend(sprite_lines(glyph_for_role(role), role_style));
        lines.push(Line::from(vec![
            Span::styled(node.label.clone(), role_style),
            Span::styled("  ", secondary),
            Span::styled(status.label(), status_style),
        ]));
        lines
    } else {
        vec![
            Line::styled(format!(" {frame} "), status_style),
            Line::styled(format!("[{}]", role_abbrev(role)), role_style),
            Line::from(vec![
                Span::styled(node.label.clone(), role_style),
                Span::styled("  ", secondary),
                Span::styled(status.label(), status_style),
            ]),
        ]
    };
    let sprite_para = Paragraph::new(sprite).alignment(Alignment::Center);
    f.render_widget(sprite_para, rows[0]);

    let status_label = if is_terminal { "Status" } else { "Phase" };
    let footer = if is_terminal {
        "Press [g] for panels or [q] to quit"
    } else {
        "Waiting for first file edit"
    };
    let phase_lines = vec![
        Line::from(vec![
            Span::styled(format!("{frame} "), status_style),
            Span::styled(format!("{status_label}: {phase}"), status_style),
            Span::styled("  ·  ", secondary),
            Span::styled(
                truncate_chars(activity, inner.width.saturating_sub(20) as usize),
                secondary,
            ),
        ]),
        Line::styled(footer, secondary),
    ];
    f.render_widget(
        Paragraph::new(phase_lines).alignment(Alignment::Center),
        rows[1],
    );

    let log_lines = activity_lines(
        session,
        rows[2].height as usize,
        inner.width as usize,
        theme,
    );
    let log_para = Paragraph::new(log_lines)
        .style(secondary)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    f.render_widget(log_para, rows[2]);
}

fn draw_no_agents(f: &mut Frame, area: Rect, theme: &Theme) {
    let para = Paragraph::new("No agents to display")
        .style(Style::default().fg(theme.text_secondary))
        .alignment(Alignment::Center)
        .block(graph_block(theme));
    f.render_widget(para, area);
}

fn sprite_lines(sprite: Sprite, style: Style) -> Vec<Line<'static>> {
    sprite
        .rows()
        .iter()
        .map(|row| Line::styled(row.iter().collect::<String>(), style))
        .collect()
}

fn phase_label(status: SessionStatus, is_thinking: bool, current_activity: &str) -> String {
    match animation_phase(status, is_thinking, current_activity) {
        AnimationPhase::Thinking => "thinking".into(),
        AnimationPhase::Spawning => "spawning".into(),
        AnimationPhase::ToolUse
            if current_activity.starts_with("$ ") || current_activity.starts_with("Bash:") =>
        {
            "command execution".into()
        }
        AnimationPhase::ToolUse
            if current_activity.starts_with("Read:")
                || current_activity.starts_with("Grep:")
                || current_activity.starts_with("Glob:") =>
        {
            "file discovery".into()
        }
        AnimationPhase::ToolUse => "tool use".into(),
        AnimationPhase::Idle if current_activity.trim().is_empty() => "waiting for output".into(),
        AnimationPhase::Idle => "running".into(),
        AnimationPhase::None => status.label().to_ascii_lowercase(),
    }
}

fn terminal_status_marker(status: SessionStatus, use_nerd_font: bool) -> &'static str {
    if use_nerd_font {
        status.nerd_symbol()
    } else {
        status.ascii_symbol()
    }
}

fn activity_lines(
    session: Option<&Session>,
    max_lines: usize,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if max_lines == 0 {
        return Vec::new();
    }

    let mut lines = vec![Line::styled(
        "Recent activity",
        Style::default()
            .fg(theme.title_accent)
            .add_modifier(Modifier::BOLD),
    )];
    let Some(session) = session else {
        lines.push(Line::styled(
            "Waiting for session data",
            Style::default().fg(theme.text_secondary),
        ));
        return lines;
    };

    let remaining = max_lines.saturating_sub(lines.len());
    let entries: Vec<_> = session.activity_log.iter().rev().take(remaining).collect();
    if entries.is_empty() {
        lines.push(Line::styled(
            "Waiting for first event from the agent",
            Style::default().fg(theme.text_secondary),
        ));
        return lines;
    }

    for entry in entries.into_iter().rev() {
        let time = entry.timestamp.format("%H:%M:%S");
        let message = truncate_chars(&entry.message, width.saturating_sub(13));
        lines.push(Line::from(vec![
            Span::styled(format!("{time} "), Style::default().fg(theme.text_muted)),
            Span::styled(message, Style::default().fg(theme.text_primary)),
        ]));
    }
    lines
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut chars = s.chars();
    let mut out: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        out.pop();
        out.push('…');
    }
    out
}
