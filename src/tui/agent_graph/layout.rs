//! Deterministic concentric/radial bipartite layout for the agent graph.
//!
//! See `docs/adr/001-agent-graph-viz.md` for the design rationale and the
//! known bands of acceptable rendering quality.

use std::f64::consts::TAU;

use super::model::{GraphEdge, GraphNode, NodeId, NodeKind};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Positioned {
    pub(crate) id_idx: usize,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

pub(crate) trait Layout {
    /// Pure: no I/O, no terminal access. Returns positions in virtual [-1.0, 1.0].
    fn position(&self, nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<Positioned>;
}

const RING_AGENTS: f64 = 0.45;
const RING_FILES: f64 = 0.85;
/// Terminal cells are ~2:1 tall on common monospace fonts; scaling x by this
/// constant makes rings render as visual circles rather than vertical ellipses.
const CELL_ASPECT: f64 = 0.5;

pub(crate) struct ConcentricLayout;

impl Layout for ConcentricLayout {
    fn position(&self, nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<Positioned> {
        let agent_idxs: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.kind, NodeKind::Agent { .. }))
            .map(|(i, _)| i)
            .collect();
        let file_idxs: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.kind, NodeKind::File))
            .map(|(i, _)| i)
            .collect();

        let agent_angles: Vec<(usize, f64)> = place_evenly(&agent_idxs, /* phase = */ 0.0);
        let file_angles = place_files_by_barycenter(&file_idxs, &agent_angles, nodes, edges);

        let mut out: Vec<Positioned> = Vec::with_capacity(nodes.len());
        for (idx, theta) in &agent_angles {
            out.push(Positioned {
                id_idx: *idx,
                x: RING_AGENTS * theta.cos() * CELL_ASPECT,
                y: RING_AGENTS * theta.sin(),
            });
        }
        for (idx, theta) in &file_angles {
            out.push(Positioned {
                id_idx: *idx,
                x: RING_FILES * theta.cos() * CELL_ASPECT,
                y: RING_FILES * theta.sin(),
            });
        }
        out.sort_by_key(|p| p.id_idx);
        out
    }
}

fn place_evenly(idxs: &[usize], phase: f64) -> Vec<(usize, f64)> {
    let n = idxs.len();
    if n == 0 {
        return Vec::new();
    }
    let step = TAU / n as f64;
    idxs.iter()
        .enumerate()
        .map(|(i, idx)| (*idx, phase + i as f64 * step))
        .collect()
}

fn place_files_by_barycenter(
    file_idxs: &[usize],
    agent_angles: &[(usize, f64)],
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) -> Vec<(usize, f64)> {
    if file_idxs.is_empty() {
        return Vec::new();
    }
    let mut barycenters: Vec<(usize, f64)> = file_idxs
        .iter()
        .map(|fi| {
            let file_id = &nodes[*fi].id;
            let touching_angles: Vec<f64> = edges
                .iter()
                .filter(|e| matches!(&e.to, NodeId::File(_)) && e.to == *file_id)
                .filter_map(|e| match &e.from {
                    NodeId::Agent(_) => agent_angles
                        .iter()
                        .find(|(idx, _)| nodes[*idx].id == e.from)
                        .map(|(_, theta)| *theta),
                    NodeId::File(_) => None,
                })
                .collect();
            let theta = if touching_angles.is_empty() {
                0.0
            } else {
                circular_mean(&touching_angles)
            };
            (*fi, theta)
        })
        .collect();

    barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let n = barycenters.len();
    let step = TAU / n as f64;
    let phase = step / 2.0;
    barycenters
        .into_iter()
        .enumerate()
        .map(|(i, (idx, _))| (idx, phase + i as f64 * step))
        .collect()
}

fn circular_mean(angles: &[f64]) -> f64 {
    let (sx, sy): (f64, f64) = angles
        .iter()
        .fold((0.0, 0.0), |(sx, sy), t| (sx + t.cos(), sy + t.sin()));
    sy.atan2(sx)
}

#[cfg(test)]
mod tests {
    use super::super::model::{GraphEdge, GraphNode, NodeId, NodeKind};
    use super::*;
    use uuid::Uuid;

    fn agent(label: &str) -> GraphNode {
        GraphNode {
            id: NodeId::Agent(Uuid::new_v4()),
            kind: NodeKind::Agent {
                status: crate::session::types::SessionStatus::Running,
            },
            label: label.into(),
        }
    }

    fn file(path: &str) -> GraphNode {
        GraphNode {
            id: NodeId::File(path.into()),
            kind: NodeKind::File,
            label: path.into(),
        }
    }

    fn edge(from: &NodeId, to: &NodeId) -> GraphEdge {
        GraphEdge {
            from: from.clone(),
            to: to.clone(),
        }
    }

    #[test]
    fn empty_input_returns_empty_output() {
        let out = ConcentricLayout.position(&[], &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn all_positions_within_unit_disc() {
        let a1 = agent("S-1");
        let a2 = agent("S-2");
        let f1 = file("main.rs");
        let nodes = vec![a1.clone(), a2.clone(), f1.clone()];
        let edges = vec![edge(&a1.id, &f1.id), edge(&a2.id, &f1.id)];
        let out = ConcentricLayout.position(&nodes, &edges);
        for p in &out {
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!(r <= 1.0, "node {} at radius {r} > 1.0", p.id_idx);
        }
    }

    #[test]
    fn files_placed_on_outer_ring_agents_on_inner() {
        let a = agent("S-1");
        let f = file("main.rs");
        let nodes = vec![a.clone(), f.clone()];
        let edges = vec![edge(&a.id, &f.id)];
        let out = ConcentricLayout.position(&nodes, &edges);
        let r0 = (out[0].x * out[0].x + out[0].y * out[0].y).sqrt();
        let r1 = (out[1].x * out[1].x + out[1].y * out[1].y).sqrt();
        assert!(r0 < r1, "agent ring ({r0}) must be inside file ring ({r1})");
    }

    #[test]
    fn output_is_indexed_by_input_position() {
        let a = agent("S-1");
        let f = file("main.rs");
        let nodes = vec![a, f];
        let out = ConcentricLayout.position(&nodes, &[]);
        assert_eq!(out[0].id_idx, 0);
        assert_eq!(out[1].id_idx, 1);
    }

    // --- Issue #526: phase offset and aspect-ratio fixes ---

    #[test]
    fn no_two_nodes_share_an_angle() {
        let a1 = agent("A1");
        let a2 = agent("A2");
        let f1 = file("shared.rs");
        let f2 = file("also_shared.rs");
        let nodes = vec![a1.clone(), a2.clone(), f1.clone(), f2.clone()];
        let edges = vec![
            edge(&a1.id, &f1.id),
            edge(&a2.id, &f1.id),
            edge(&a1.id, &f2.id),
            edge(&a2.id, &f2.id),
        ];
        let out = ConcentricLayout.position(&nodes, &edges);

        let angles: Vec<f64> = out.iter().map(|p| p.y.atan2(p.x)).collect();
        for i in 0..angles.len() {
            for j in (i + 1)..angles.len() {
                let diff = (angles[i] - angles[j]).abs();
                let dist = diff.min(TAU - diff);
                assert!(
                    dist > 1e-6,
                    "nodes {i} and {j} share angle {:.6} ≈ {:.6}",
                    angles[i],
                    angles[j]
                );
            }
        }
    }

    #[test]
    fn aspect_correction_squashes_x_axis() {
        let a = agent("A");
        let f1 = file("f1.rs");
        let f2 = file("f2.rs");
        let f3 = file("f3.rs");
        let f4 = file("f4.rs");
        let nodes = vec![a.clone(), f1.clone(), f2.clone(), f3.clone(), f4.clone()];
        let edges = vec![
            edge(&a.id, &f1.id),
            edge(&a.id, &f2.id),
            edge(&a.id, &f3.id),
            edge(&a.id, &f4.id),
        ];
        let out = ConcentricLayout.position(&nodes, &edges);

        let max_x = out.iter().map(|p| p.x.abs()).fold(0.0_f64, f64::max);
        let max_y = out.iter().map(|p| p.y.abs()).fold(0.0_f64, f64::max);
        assert!(
            max_x < max_y,
            "aspect correction must squash x: max_x={max_x:.4} max_y={max_y:.4}"
        );
    }
}
