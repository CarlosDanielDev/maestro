//! Data of the Files lens of Dashboard Deps (§02.20 of
//! `docs/design-system/index.html`, #1292): one row per agent, one per
//! repo-relative file, an `Edit` or `Read` edge per (agent, file).
//!
//! Edits come from the file claims (Write/Edit tool calls, already keyed
//! repo-relative by `claim_conflicts`) and the recorded conflicts; reads
//! are the rest of `Session::files_touched`. Each session writes inside
//! its own worktree, so raw paths are never compared (#1156).

use uuid::Uuid;

use crate::session::types::Session;
use crate::state::file_claims::{FileClaimManager, repo_relative_key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EdgeKind {
    Edit,
    Read,
}

/// One file of the lens, keyed by its repo-relative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileRow {
    pub(crate) path: String,
    /// Agents that edited it, in agent order.
    pub(crate) editors: Vec<Uuid>,
    /// Agents that only read it.
    pub(crate) readers: Vec<Uuid>,
    /// The session that holds the claim, when one does.
    pub(crate) holder: Option<Uuid>,
    /// A conflict on this file was recorded (`conflict_history`).
    pub(crate) conflict: bool,
}

impl FileRow {
    /// Two or more agents edit this file.
    pub(crate) fn contested(&self) -> bool {
        self.editors.len() >= 2
    }

    pub(crate) fn edge(&self, agent: Uuid) -> Option<EdgeKind> {
        if self.editors.contains(&agent) {
            Some(EdgeKind::Edit)
        } else if self.readers.contains(&agent) {
            Some(EdgeKind::Read)
        } else {
            None
        }
    }
}

/// Agents (session order) and files: CONTESTED first, then EDITED, then
/// READ ONLY; inside a section by the mean row of their agents
/// (barycenter), so edges stay short, then by path.
#[derive(Debug, Clone, Default)]
pub(crate) struct FilesGraph {
    pub(crate) agents: Vec<Uuid>,
    pub(crate) files: Vec<FileRow>,
}

impl FilesGraph {
    pub(crate) fn contested(&self) -> usize {
        self.files.iter().filter(|f| f.contested()).count()
    }

    pub(crate) fn edited(&self) -> usize {
        self.files.iter().filter(|f| !f.editors.is_empty()).count()
    }

    /// Files `agent` edits.
    pub(crate) fn edits_of(&self, agent: Uuid) -> usize {
        self.files
            .iter()
            .filter(|f| f.editors.contains(&agent))
            .count()
    }
}

/// Build the lens from the pool's sessions and its file claims.
pub(crate) fn build_graph(sessions: &[&Session], claims: &FileClaimManager) -> FilesGraph {
    let agents: Vec<Uuid> = sessions.iter().map(|s| s.id).collect();
    let mut files: Vec<FileRow> = Vec::new();
    let mut touch = |path: &str, agent: Uuid, kind: EdgeKind| {
        let at = match files.iter().position(|f| f.path == path) {
            Some(at) => at,
            None => {
                files.push(FileRow {
                    path: path.to_string(),
                    editors: Vec::new(),
                    readers: Vec::new(),
                    holder: claims.owner_of(path),
                    conflict: false,
                });
                files.len() - 1
            }
        };
        let row = &mut files[at];
        match kind {
            EdgeKind::Edit if !row.editors.contains(&agent) => {
                row.readers.retain(|r| *r != agent);
                row.editors.push(agent);
            }
            EdgeKind::Read if row.edge(agent).is_none() => row.readers.push(agent),
            _ => {}
        }
    };
    for s in sessions {
        for path in claims.files_for_session(s.id) {
            touch(path, s.id, EdgeKind::Edit);
        }
    }
    for c in claims.conflict_history() {
        for agent in [c.owner_session_id, c.offender_session_id] {
            if agents.contains(&agent) {
                touch(&c.file_path, agent, EdgeKind::Edit);
            }
        }
    }
    for s in sessions {
        for raw in &s.files_touched {
            touch(
                &repo_relative_key(raw, s.worktree_path.as_deref()),
                s.id,
                EdgeKind::Read,
            );
        }
    }
    for c in claims.conflict_history() {
        if let Some(row) = files.iter_mut().find(|f| f.path == c.file_path) {
            row.conflict = true;
        }
    }
    let bary = |f: &FileRow| {
        let rows: Vec<usize> = (f.editors.iter().chain(&f.readers))
            .filter_map(|a| agents.iter().position(|x| x == a))
            .collect();
        rows.iter().sum::<usize>() * 1000 / rows.len().max(1)
    };
    files.sort_by(|a, b| {
        let section = |f: &FileRow| match (f.contested(), f.editors.is_empty()) {
            (true, _) => 0,
            (false, false) => 1,
            (false, true) => 2,
        };
        (section(a), bary(a), &a.path).cmp(&(section(b), bary(b), &b.path))
    });
    FilesGraph { agents, files }
}

/// `#412`, or `S-` and the first 4 uuid characters with no issue.
pub(crate) fn agent_label(s: &Session) -> String {
    match s.issue_number {
        Some(n) => format!("#{n}"),
        None => format!("S-{}", &s.id.to_string()[..4]),
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
