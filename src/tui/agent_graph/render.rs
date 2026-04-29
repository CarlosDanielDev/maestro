//! Renderer for the agent-graph view.
//!
//! Consumes positions from `super::layout` and paints nodes + edges onto a
//! ratatui `Canvas`. See `docs/adr/001-agent-graph-viz.md` for the design
//! constraints (deterministic, no animation, ≥ 80×24 viewport).

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::Line,
    widgets::{
        Block, Borders, Paragraph,
        canvas::{Canvas, Line as CanvasLine, Rectangle},
    },
};

use super::layout::{ConcentricLayout, Layout};
use super::model::{GraphEdge, GraphNode, NodeKind};
use crate::session::types::SessionStatus;

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;

pub(crate) fn draw_agent_graph(
    f: &mut Frame,
    area: Rect,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    use_braille: bool,
) {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(f, area);
        return;
    }
    if nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Agent { .. }))
        .count()
        < 2
    {
        draw_single_agent_card(f, area, nodes);
        return;
    }

    let layout = ConcentricLayout;
    let positions = layout.position(nodes, edges);

    let marker = if use_braille {
        Marker::Braille
    } else {
        Marker::Block
    };

    let nodes_for_paint = nodes.to_vec();
    let edges_for_paint = edges.to_vec();
    let positions_for_paint = positions;

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" agent graph "),
        )
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
                    color: Color::DarkGray,
                });
            }

            for (idx, node) in nodes_for_paint.iter().enumerate() {
                let p = positions_for_paint[idx];
                let (color, modifier) = node_style(&node.kind);
                ctx.draw(&Rectangle {
                    x: p.x - 0.02,
                    y: p.y - 0.02,
                    width: 0.04,
                    height: 0.04,
                    color,
                });
                let style = Style::default().fg(color).add_modifier(modifier);
                ctx.print(p.x, p.y - 0.08, Line::styled(node.label.clone(), style));
            }
        });

    f.render_widget(canvas, area);
}

fn draw_too_small(f: &mut Frame, area: Rect) {
    let msg = format!(
        "Agent graph requires {MIN_WIDTH}×{MIN_HEIGHT} (current: {}×{}). Press [g] for panels.",
        area.width, area.height
    );
    let para = Paragraph::new(msg)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(para, area);
}

fn draw_single_agent_card(f: &mut Frame, area: Rect, nodes: &[GraphNode]) {
    let label = nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Agent { .. }))
        .map(|n| n.label.clone())
        .unwrap_or_else(|| "—".to_string());
    let files: Vec<String> = nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::File))
        .map(|n| n.label.clone())
        .collect();

    let body = vec![
        Line::from(format!("▶  {label}  RUNNING")),
        Line::from(format!("    Files: {}", files.join(", "))),
        Line::from(""),
        Line::from("1 agent active — graph view activates at 2+ agents"),
    ];

    let para = Paragraph::new(body).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" agent graph "),
    );
    f.render_widget(para, area);
}

fn node_style(kind: &NodeKind) -> (Color, Modifier) {
    match kind {
        NodeKind::Agent { status, .. } => match status {
            SessionStatus::Running => (Color::Green, Modifier::BOLD),
            SessionStatus::Errored | SessionStatus::Killed => (Color::Red, Modifier::BOLD),
            SessionStatus::Completed => (Color::Gray, Modifier::DIM),
            _ => (Color::Yellow, Modifier::empty()),
        },
        NodeKind::File => (Color::Cyan, Modifier::empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_style_distinguishes_running_vs_completed() {
        let running = node_style(&NodeKind::Agent {
            status: SessionStatus::Running,
            issue_number: None,
        });
        let completed = node_style(&NodeKind::Agent {
            status: SessionStatus::Completed,
            issue_number: None,
        });
        assert_ne!(running.0, completed.0);
    }

    #[test]
    fn file_style_is_neutral_color() {
        let (color, _) = node_style(&NodeKind::File);
        assert_eq!(color, Color::Cyan);
    }

    #[test]
    fn too_small_message_contains_dimensions() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut terminal = Terminal::new(TestBackend::new(79, 23)).unwrap();
        terminal
            .draw(|f| {
                draw_agent_graph(f, f.area(), &[], &[], false);
            })
            .unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("79"), "width not in message");
        assert!(rendered.contains("23"), "height not in message");
    }
}
