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
    /// A maestro pool session with no matching external transcript.
    Managed,
    /// An external transcript maestro has not adopted.
    External,
    /// A pool session bound to a discovered transcript. Shown once, here.
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
/// transcripts. A transcript whose id a pool session is bound to becomes
/// that pool row, kind `Adopted`, and is not listed twice.
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
        let title = session
            .issue_title
            .clone()
            .unwrap_or_else(|| session.prompt.clone());
        let status = session.status.label().to_string();
        let row = match bound {
            Some(transcript) => {
                claimed.insert(transcript.session_id.as_str());
                FleetRow {
                    kind: FleetKind::Adopted,
                    source: FleetSource::Pool {
                        session_id: session.id,
                    },
                    repo: repo_name(&transcript.cwd),
                    title: if title.is_empty() {
                        transcript.title.clone()
                    } else {
                        title
                    },
                    status,
                    cwd: Some(transcript.cwd.clone()),
                    last_active: Some(transcript.last_active),
                }
            }
            None => FleetRow {
                kind: FleetKind::Managed,
                source: FleetSource::Pool {
                    session_id: session.id,
                },
                repo: session
                    .adopted_cwd
                    .as_deref()
                    .map_or_else(|| repo.to_string(), repo_name),
                title,
                status,
                cwd: session.adopted_cwd.clone(),
                last_active: session
                    .finished_at
                    .or(session.started_at)
                    .map(SystemTime::from),
            },
        };
        rows.push(row);
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
        FleetSort::ActivityDesc => indices.sort_by_key(|&i| Reverse(activity_key(&rows[i]))),
        FleetSort::StaleFirst => indices.sort_by_key(|&i| activity_key(&rows[i])),
    }
    indices
}

/// A row that never started sorts as the oldest activity.
fn activity_key(row: &FleetRow) -> SystemTime {
    row.last_active.unwrap_or(UNIX_EPOCH)
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
