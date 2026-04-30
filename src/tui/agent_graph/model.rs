//! Data model for the agent-graph view. See `docs/adr/001-agent-graph-viz.md`.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::session::types::{Session, SessionStatus};

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum NodeId {
    Agent(Uuid),
    File(PathBuf),
}

#[derive(Clone)]
pub(crate) enum NodeKind {
    Agent { status: SessionStatus },
    File,
}

#[derive(Clone)]
pub(crate) struct GraphNode {
    pub(crate) id: NodeId,
    pub(crate) kind: NodeKind,
    pub(crate) label: String,
}

#[derive(Clone)]
pub(crate) struct GraphEdge {
    pub(crate) from: NodeId,
    pub(crate) to: NodeId,
}

/// Build a bipartite graph of agents and the files they touch.
///
/// One `GraphNode::Agent` per session. One `GraphNode::File` per unique path
/// across all `Session::files_touched`. One edge per (agent, file) pair the
/// agent has touched.
pub(crate) fn build_graph(sessions: &[&Session]) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes: Vec<GraphNode> = Vec::with_capacity(sessions.len() * 2);
    let mut edges: Vec<GraphEdge> = Vec::new();

    for &s in sessions {
        let agent_id = NodeId::Agent(s.id);
        nodes.push(GraphNode {
            id: agent_id.clone(),
            kind: NodeKind::Agent { status: s.status },
            label: agent_label(s),
        });

        for raw in &s.files_touched {
            let path = PathBuf::from(raw);
            let file_id = NodeId::File(path.clone());
            if !nodes.iter().any(|n| n.id == file_id) {
                nodes.push(GraphNode {
                    id: file_id.clone(),
                    kind: NodeKind::File,
                    label: file_label(&path),
                });
            }
            edges.push(GraphEdge {
                from: agent_id.clone(),
                to: file_id,
            });
        }
    }

    (nodes, edges)
}

fn agent_label(s: &Session) -> String {
    if let Some(n) = s.issue_number {
        return format!("#{n}");
    }
    let s = s.id.to_string();
    let head: String = s.chars().take(4).collect();
    format!("S-{head}")
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(status: SessionStatus, files: &[&str]) -> Session {
        let mut s = Session::new(String::new(), String::new(), String::new(), None, None);
        s.status = status;
        s.files_touched = files.iter().map(|s| (*s).to_string()).collect();
        s
    }

    #[test]
    fn build_graph_dedupes_files_across_sessions() {
        let s1 = fake(SessionStatus::Running, &["src/main.rs", "src/config.rs"]);
        let s2 = fake(SessionStatus::Running, &["src/config.rs", "Cargo.toml"]);
        let (nodes, edges) = build_graph(&[&s1, &s2]);

        let agent_count = nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Agent { .. }))
            .count();
        let file_count = nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::File))
            .count();

        assert_eq!(agent_count, 2);
        assert_eq!(file_count, 3, "config.rs must be deduped");
        assert_eq!(edges.len(), 4, "two files per session, no dedup of edges");
    }

    #[test]
    fn agent_label_prefers_issue_number() {
        let mut s = fake(SessionStatus::Running, &[]);
        s.issue_number = Some(513);
        assert_eq!(agent_label(&s), "#513");
    }

    #[test]
    fn file_label_uses_basename() {
        assert_eq!(file_label(Path::new("src/tui/ui.rs")), "ui.rs");
        assert_eq!(file_label(Path::new("Cargo.toml")), "Cargo.toml");
    }
}
