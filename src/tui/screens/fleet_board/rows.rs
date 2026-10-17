//! Row model for the Fleet board (#1067): pool sessions and discovered
//! transcripts merged into one list. Pure data: no ratatui, no I/O.

use crate::session::discovery::DiscoveredSession;
use crate::session::types::Session;
use crate::tui::screens::adopt_picker::repo_name;
use std::cmp::Reverse;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Where a row came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FleetKind {
    /// A maestro pool session that was not adopted (spawned by maestro).
    Managed,
    /// An external transcript maestro has not adopted.
    External,
    /// A pool session adopted from an external transcript (`adopted_cwd`
    /// is set). Its transcript, if still on disk, is shown once, here.
    Adopted,
}

/// What the row opens or adopts. Pool rows carry the pool session id;
/// external rows carry the transcript's session id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FleetSource {
    Pool { session_id: Uuid },
    Transcript { agent_session_id: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FleetRow {
    pub kind: FleetKind,
    pub source: FleetSource,
    pub repo: String,
    pub title: String,
    pub status: String,
    pub cwd: Option<PathBuf>,
    /// `None` for a pool session that never started. It sorts as the
    /// oldest activity, so it is the most neglected.
    pub last_active: Option<SystemTime>,
}

/// Sort order of the board. `ActivityDesc` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FleetSort {
    #[default]
    ActivityDesc,
    StaleFirst,
}

impl FleetSort {
    pub fn toggled(self) -> Self {
        match self {
            Self::ActivityDesc => Self::StaleFirst,
            Self::StaleFirst => Self::ActivityDesc,
        }
    }
}

/// Merge pool sessions (`repo` is the current repo) with machine-wide
/// transcripts.
///
/// Kind comes from the session alone: `Adopted` iff it was adopted
/// (`adopted_cwd` is set, #1053), `Managed` otherwise. A maestro-spawned
/// session also binds its transcript id, so the id match never decides the
/// kind. The match does two other jobs: a transcript claimed by a pool row
/// is not listed again as `External`, and it fills the row's repo, cwd and
/// last-active time.
pub fn build_rows(
    pool: &[&Session],
    repo: &str,
    transcripts: &[DiscoveredSession],
) -> Vec<FleetRow> {
    let mut rows = Vec::with_capacity(pool.len() + transcripts.len());
    let mut claimed: HashSet<&str> = HashSet::new();

    for session in pool {
        let bound = session
            .agent_session_id
            .as_deref()
            .and_then(|id| transcripts.iter().find(|t| t.session_id == id));
        if let Some(transcript) = bound {
            claimed.insert(transcript.session_id.as_str());
        }
        let kind = if session.adopted_cwd.is_some() {
            FleetKind::Adopted
        } else {
            FleetKind::Managed
        };
        let (repo_col, cwd, last_active) = match bound {
            Some(transcript) => (
                repo_name(&transcript.cwd),
                Some(transcript.cwd.clone()),
                Some(transcript.last_active),
            ),
            None => (
                session
                    .adopted_cwd
                    .as_deref()
                    .map_or_else(|| repo.to_string(), repo_name),
                session.adopted_cwd.clone(),
                session
                    .finished_at
                    .or(session.started_at)
                    .map(SystemTime::from),
            ),
        };
        let title = match session.issue_title.clone() {
            Some(title) if !title.is_empty() => title,
            _ => bound
                .map(|t| t.title.clone())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| session.prompt.clone()),
        };
        rows.push(FleetRow {
            kind,
            source: FleetSource::Pool {
                session_id: session.id,
            },
            repo: repo_col,
            title,
            status: session.status.label().to_string(),
            cwd,
            last_active,
        });
    }

    for transcript in transcripts {
        if claimed.contains(transcript.session_id.as_str()) {
            continue;
        }
        rows.push(FleetRow {
            kind: FleetKind::External,
            source: FleetSource::Transcript {
                agent_session_id: transcript.session_id.clone(),
            },
            repo: repo_name(&transcript.cwd),
            title: transcript.title.clone(),
            status: "external".to_string(),
            cwd: Some(transcript.cwd.clone()),
            last_active: Some(transcript.last_active),
        });
    }
    rows
}

/// Indices into `rows` that match `filter` (case-insensitive substring on
/// repo and title), in `sort` order. Ties keep input order.
pub fn sorted_indices(rows: &[FleetRow], filter: &str, sort: FleetSort) -> Vec<usize> {
    let needle = filter.to_lowercase();
    let mut indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            needle.is_empty()
                || r.repo.to_lowercase().contains(&needle)
                || r.title.to_lowercase().contains(&needle)
        })
        .map(|(i, _)| i)
        .collect();
    match sort {
        FleetSort::ActivityDesc => {
            indices.sort_by_key(|&i| (Reverse(activity_key(&rows[i])), tie_key(&rows[i])));
        }
        FleetSort::StaleFirst => {
            indices.sort_by_key(|&i| (activity_key(&rows[i]), tie_key(&rows[i])));
        }
    }
    indices
}

/// A row that never started sorts as the oldest activity.
fn activity_key(row: &FleetRow) -> SystemTime {
    row.last_active.unwrap_or(UNIX_EPOCH)
}

/// Breaks activity ties by cwd, then by source id. Input order comes from
/// directory reads, which differ per machine, so it must not decide ties.
fn tie_key(row: &FleetRow) -> (Option<PathBuf>, String) {
    let id = match &row.source {
        FleetSource::Pool { session_id } => session_id.to_string(),
        FleetSource::Transcript { agent_session_id } => agent_session_id.clone(),
    };
    (row.cwd.clone(), id)
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
