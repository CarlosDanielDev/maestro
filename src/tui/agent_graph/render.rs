//! Renderer for the agent-graph view.
//!
//! Consumes positions from `super::layout` and paints nodes + edges onto a
//! ratatui `Canvas`. See `docs/adr/001-agent-graph-viz.md` for the design
//! constraints (≥ 80×24 viewport, deterministic layout) and `super::animation`
//! for the per-tick animation rules added in #529.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout as UiLayout, Rect},
    style::{Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Wrap,
        canvas::{Canvas, Context, Line as CanvasLine, Rectangle},
    },
};

use super::animation::{SessionRenderInfo, edge_color, node_animation_style};
use super::label_placement::{CanvasPoint, place_file_label, place_label};
use super::layout::{ConcentricLayout, Layout};
use super::model::{GraphEdge, GraphNode, NodeId, NodeKind};
use super::personalities::{Sprite, glyph_for_role, role_abbrev, role_color};
use crate::session::types::{Session, SessionStatus};
use crate::tui::spinner::{AnimationPhase, animation_phase, graph_node_frame};
use crate::tui::theme::Theme;

/// Radial distance for the ASCII-mode `[ROLE] #NNN` label. The agent glyph
/// is a 1×1 cell rectangle, so a small offset suffices.
const LABEL_RADIUS_BLOCK: f64 = 0.10;

/// Number of cell rows of empty space between the sprite top/bottom and the
/// nerd-font `#NNN` label. Combined with the sprite's half-height
/// (2.5 cell rows) gives a label radius of `4.0 * cell_h`. Issue #576.
const SPRITE_LABEL_BUFFER_CELLS: f64 = 1.5;

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;

pub(crate) struct GraphRenderOptions<'a> {
    pub(crate) use_nerd_font: bool,
    pub(crate) tick: usize,
    pub(crate) sessions: &'a [&'a Session],
    pub(crate) theme: &'a Theme,
}

pub(crate) fn draw_agent_graph(
    f: &mut Frame,
    area: Rect,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    options: GraphRenderOptions<'_>,
) {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(f, area, options.theme);
        return;
    }
    let agent_count = nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Agent { .. }))
        .count();
    let file_count = nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::File))
        .count();
    if agent_count == 0 {
        draw_no_agents(f, area, options.theme);
        return;
    }
    // One active agent with no files has no edges yet, but it should still
    // render as a live graph state: sprite, spinner, phase, and recent log.
    if agent_count == 1 && file_count == 0 {
        draw_single_agent_empty_state(f, area, nodes, options);
        return;
    }

    let layout = ConcentricLayout;
    let positions = layout.position(nodes, edges);

    let marker = if options.use_nerd_font {
        Marker::Braille
    } else {
        Marker::Block
    };

    let nodes_for_paint = nodes.to_vec();
    let edges_for_paint = edges.to_vec();
    let positions_for_paint = positions;
    let inner_cols = area.width.saturating_sub(2);
    let inner_rows = area.height.saturating_sub(2);
    let session_infos: Vec<SessionRenderInfo> = options
        .sessions
        .iter()
        .map(|s| SessionRenderInfo::from_session(s))
        .collect();
    let file_color = options.theme.accent_info;
    let graph_block = graph_block(options.theme);
    let tick = options.tick;
    let use_nerd_font = options.use_nerd_font;

    let canvas = Canvas::default()
        .block(graph_block)
        .marker(marker)
        .x_bounds([-1.0, 1.0])
        .y_bounds([-1.0, 1.0])
        .paint(move |ctx| {
            for e in &edges_for_paint {
                let from_idx = nodes_for_paint
                    .iter()
                    .position(|n| n.id == e.from)
                    .unwrap_or(0);
                let to_idx = nodes_for_paint
                    .iter()
                    .position(|n| n.id == e.to)
                    .unwrap_or(0);
                let p1 = positions_for_paint[from_idx];
                let p2 = positions_for_paint[to_idx];
                ctx.draw(&CanvasLine {
                    x1: p1.x,
                    y1: p1.y,
                    x2: p2.x,
                    y2: p2.y,
                    color: edge_color(e, &session_infos, tick),
                });
            }

            for (idx, node) in nodes_for_paint.iter().enumerate() {
                let p = positions_for_paint[idx];
                let session = find_session(&session_infos, &node.id);
                let label = label_for_node(node, session, tick, use_nerd_font);

                match &node.kind {
                    NodeKind::Agent { status } => {
                        let role = session.map(|s| s.role).unwrap_or_default();
                        let role_fg = role_color(role);
                        let status_mod = status_modifier(*status);
                        let (color, modifier) = match session {
                            Some(s) => node_animation_style(s, role_fg, status_mod),
                            None => (role_fg, status_mod),
                        };
                        let style = Style::default().fg(color).add_modifier(modifier);

                        let agent_pt = CanvasPoint { x: p.x, y: p.y };
                        let outbound = outbound_targets(
                            &node.id,
                            &edges_for_paint,
                            &nodes_for_paint,
                            &positions_for_paint,
                        );

                        if use_nerd_font {
                            draw_sprite_on_canvas(
                                ctx,
                                p.x,
                                p.y,
                                glyph_for_role(role),
                                style,
                                inner_cols,
                                inner_rows,
                            );
                            let cell_h = 2.0 / inner_rows.max(2).saturating_sub(1) as f64;
                            let label_radius = (2.5 + SPRITE_LABEL_BUFFER_CELLS) * cell_h;
                            let (lx, ly) = place_label(
                                agent_pt,
                                &outbound,
                                label_radius,
                                label.chars().count(),
                            );
                            ctx.print(lx, ly, Line::styled(label, style));
                        } else {
                            ctx.draw(&Rectangle {
                                x: p.x - 0.02,
                                y: p.y - 0.02,
                                width: 0.04,
                                height: 0.04,
                                color,
                            });
                            let labeled = format!("[{}] {}", role_abbrev(role), label);
                            let (lx, ly) = place_label(
                                agent_pt,
                                &outbound,
                                LABEL_RADIUS_BLOCK,
                                labeled.chars().count(),
                            );
                            ctx.print(lx, ly, Line::styled(labeled, style));
                        }
                    }
                    NodeKind::File => {
                        // Label anchored at the edge endpoint; any y-offset reintroduces the gap. See ADR #569.
                        let style = Style::default().fg(file_color);
                        let pt = CanvasPoint { x: p.x, y: p.y };
                        let (lx, rendered) = place_file_label(pt, &label, inner_cols);
                        ctx.print(lx, p.y, Line::styled(rendered, style));
                    }
                }
            }
        });

    f.render_widget(canvas, area);
}

/// Collect the canvas-space target points of edges that originate at
/// `agent_id`. Used to compute a label angle that avoids overlapping any
/// outbound edge — see `super::label_placement` and issue #567.
fn outbound_targets(
    agent_id: &NodeId,
    edges: &[GraphEdge],
    nodes: &[GraphNode],
    positions: &[super::layout::Positioned],
) -> Vec<CanvasPoint> {
    edges
        .iter()
        .filter(|e| &e.from == agent_id)
        .filter_map(|e| {
            let to_idx = nodes.iter().position(|n| n.id == e.to)?;
            let p = positions[to_idx];
            Some(CanvasPoint { x: p.x, y: p.y })
        })
        .collect()
}

fn label_for_node(
    node: &GraphNode,
    session: Option<&SessionRenderInfo>,
    tick: usize,
    use_nerd_font: bool,
) -> String {
    let Some(session) = session else {
        return node.label.clone();
    };
    if session.status != SessionStatus::Running {
        return node.label.clone();
    }
    format!("{} {}", graph_node_frame(tick, use_nerd_font), node.label)
}

fn find_session<'a>(
    sessions: &'a [SessionRenderInfo],
    node_id: &NodeId,
) -> Option<&'a SessionRenderInfo> {
    let NodeId::Agent(uuid) = node_id else {
        return None;
    };
    sessions.iter().find(|s| s.id == *uuid)
}

fn draw_too_small(f: &mut Frame, area: Rect, theme: &Theme) {
    let msg = format!(
        "Agent graph requires {MIN_WIDTH}×{MIN_HEIGHT} (current: {}×{}). Press [g] for panels.",
        area.width, area.height
    );
    let para = Paragraph::new(msg)
        .style(Style::default().fg(theme.text_secondary))
        .alignment(Alignment::Center)
        .block(graph_block(theme));
    f.render_widget(para, area);
}

fn draw_no_agents(f: &mut Frame, area: Rect, theme: &Theme) {
    let para = Paragraph::new("No agents to display")
        .style(Style::default().fg(theme.text_secondary))
        .alignment(Alignment::Center)
        .block(graph_block(theme));
    f.render_widget(para, area);
}

fn draw_single_agent_empty_state(
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
    let role = session.map(|s| s.role).unwrap_or_default();
    let status_style = Style::default()
        .fg(theme.status_color(status))
        .add_modifier(status_modifier(status));
    let role_style = Style::default().fg(role_color(role));
    let secondary = Style::default().fg(theme.text_secondary);
    let activity = session
        .map(|s| s.current_activity.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("Waiting for output");
    let frame = graph_node_frame(options.tick, options.use_nerd_font);
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

    let phase_lines = vec![
        Line::from(vec![
            Span::styled(format!("{frame} "), status_style),
            Span::styled(format!("Phase: {phase}"), status_style),
            Span::styled("  ·  ", secondary),
            Span::styled(
                truncate_chars(activity, inner.width.saturating_sub(20) as usize),
                secondary,
            ),
        ]),
        Line::styled("Waiting for first file edit", secondary),
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

fn graph_block(theme: &Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_inactive))
        .title(Line::styled(
            " agent graph ",
            Style::default().fg(theme.title_accent),
        ))
}

/// Status modifier layered on top of an agent's role color.
///
/// The role color identifies *who* the agent is; this modifier identifies
/// *what they are doing right now*. Composed via `Style::add_modifier` so the
/// role color is preserved — see `docs/adr/002-agent-personalities.md`
/// § Status Modifier Composition.
pub(super) fn status_modifier(status: SessionStatus) -> Modifier {
    use SessionStatus::*;
    match status {
        Running | GatesRunning | NeedsReview | NeedsPr | CiFix | ConflictFix => Modifier::BOLD,
        Errored | FailedGates => Modifier::DIM | Modifier::BOLD,
        Completed | Killed | Paused => Modifier::DIM,
        Stalled => Modifier::DIM | Modifier::REVERSED,
        Spawning | Queued | Retrying => Modifier::empty(),
    }
}

/// Paint a 6×6 sprite onto the canvas at `(cx, cy)` (canvas units).
///
/// `row_step` and `x_offset` are derived from ratatui's canvas-to-cell mapping
/// (`2.0 / (inner_rows - 1)`, `2.0 / (inner_cols - 1)` — see `Canvas::render`
/// in ratatui 0.29) so consecutive sprite rows land in adjacent terminal rows
/// on every viewport from 80×24 up to 200×60. Pre-#576 these were hard-coded
/// constants calibrated to 80×24, which left 1- to 3-row gaps between sprite
/// rows on larger viewports because `ctx.print` floors y to a cell index.
/// See `docs/adr/002-agent-personalities.md` § Viewport-Derived Sprite Sizing.
fn draw_sprite_on_canvas(
    ctx: &mut Context<'_>,
    cx: f64,
    cy: f64,
    sprite: Sprite,
    style: Style,
    inner_cols: u16,
    inner_rows: u16,
) {
    let row_step = 2.0 / inner_rows.max(2).saturating_sub(1) as f64;
    let cell_w = 2.0 / inner_cols.max(2).saturating_sub(1) as f64;
    let x_offset = -2.5 * cell_w;

    for (row_idx, row_chars) in sprite.rows().iter().enumerate() {
        let y = cy + (2.5 - row_idx as f64) * row_step;
        let s: String = row_chars.iter().collect();
        ctx.print(cx + x_offset, y, Line::styled(s, style));
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "render_label_placement_tests.rs"]
mod label_placement_tests;

#[cfg(test)]
#[path = "status_modifier_tests.rs"]
mod status_modifier_tests;

#[cfg(test)]
#[path = "render_sprite_tests.rs"]
mod sprite_tests;
