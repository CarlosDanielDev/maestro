//! PRD data model (#321).
//!
//! Live document for the project. Sections fixed by spec: Vision, Goals,
//! Non-Goals, Current State, Stakeholders, Timeline. The `Current State`
//! section is auto-populated from GitHub data (open/closed milestones and
//! issues) and the rest is editable from the TUI.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #321. The PRD TUI screen + GitHub syncer
// land in Phase 2; serde + state-machine methods are tests-only today.
#![allow(dead_code)]

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GoalId(pub Uuid);

impl GoalId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for GoalId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub id: GoalId,
    pub text: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_milestone: Option<String>,
}

impl Goal {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: GoalId::new(),
            text: text.into(),
            done: false,
            linked_milestone: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stakeholder {
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimelineStatus {
    Planned,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineMilestone {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_date: Option<NaiveDate>,
    pub status: TimelineStatus,
    #[serde(default = "TimelineMilestone::default_progress")]
    pub progress: f32,
}

impl TimelineMilestone {
    fn default_progress() -> f32 {
        0.0
    }

    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            target_date: None,
            status: TimelineStatus::Planned,
            progress: 0.0,
        }
    }
}

/// Auto-populated state derived from GitHub data. Never edited by hand.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentState {
    pub open_issues: u32,
    pub closed_issues: u32,
    pub open_milestones: u32,
    pub closed_milestones: u32,
    /// Top blockers (issue numbers) from open issues' `Blocked By` parsing.
    #[serde(default)]
    pub top_blockers: Vec<u64>,
}

impl CurrentState {
    pub fn total_issues(&self) -> u32 {
        self.open_issues + self.closed_issues
    }

    pub fn completion_ratio(&self) -> f32 {
        let total = self.total_issues();
        if total == 0 {
            0.0
        } else {
            self.closed_issues as f32 / total as f32
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prd {
    /// Schema version. Bumped on breaking changes.
    pub version: u8,
    pub vision: String,
    #[serde(default)]
    pub goals: Vec<Goal>,
    #[serde(default)]
    pub non_goals: Vec<String>,
    #[serde(default)]
    pub current_state: CurrentState,
    #[serde(default)]
    pub stakeholders: Vec<Stakeholder>,
    #[serde(default)]
    pub timeline: Vec<TimelineMilestone>,
}

impl Prd {
    pub const SCHEMA_VERSION: u8 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::SCHEMA_VERSION,
            vision: String::new(),
            goals: Vec::new(),
            non_goals: Vec::new(),
            current_state: CurrentState::default(),
            stakeholders: Vec::new(),
            timeline: Vec::new(),
        }
    }

    pub fn add_goal(&mut self, text: impl Into<String>) -> GoalId {
        let g = Goal::new(text);
        let id = g.id;
        self.goals.push(g);
        id
    }

    /// Returns true if the goal was found and removed.
    pub fn remove_goal(&mut self, id: GoalId) -> bool {
        let len = self.goals.len();
        self.goals.retain(|g| g.id != id);
        self.goals.len() != len
    }
}

impl Default for Prd {
    fn default() -> Self {
        Self::new()
    }
}

/// Counts of what `Prd::merge_ingested` actually applied — used by the
/// caller to format an activity-log entry without re-running the merge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub filled_vision: bool,
    pub added_goals: usize,
    pub added_non_goals: usize,
    pub added_stakeholders: usize,
}

impl MergeReport {
    pub fn is_no_op(&self) -> bool {
        !self.filled_vision
            && self.added_goals == 0
            && self.added_non_goals == 0
            && self.added_stakeholders == 0
    }
}

impl Prd {
    /// Additive merge: take fields from `ingested` and apply them to
    /// `self` WITHOUT overwriting user-edited values.
    /// - Vision: filled only if `self.vision` is empty.
    /// - Goals/Non-Goals/Stakeholders: appended if not already present
    ///   (case-insensitive on text/name).
    pub fn merge_ingested(&mut self, ingested: &crate::prd::ingest::IngestedPrd) -> MergeReport {
        let mut report = MergeReport::default();

        if self.vision.trim().is_empty()
            && let Some(v) = ingested.vision.as_ref()
        {
            self.vision = v.clone();
            report.filled_vision = true;
        }
        for goal in &ingested.goals {
            if !self.goals.iter().any(|g| g.text.eq_ignore_ascii_case(goal)) {
                self.goals.push(Goal::new(goal));
                report.added_goals += 1;
            }
        }
        for ng in &ingested.non_goals {
            if !self.non_goals.iter().any(|x| x.eq_ignore_ascii_case(ng)) {
                self.non_goals.push(ng.clone());
                report.added_non_goals += 1;
            }
        }
        for (name, role) in &ingested.stakeholders {
            if !self
                .stakeholders
                .iter()
                .any(|s| s.name.eq_ignore_ascii_case(name))
            {
                self.stakeholders.push(Stakeholder {
                    name: name.clone(),
                    role: role.clone(),
                });
                report.added_stakeholders += 1;
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prd_new_pins_schema_version() {
        let p = Prd::new();
        assert_eq!(p.version, Prd::SCHEMA_VERSION);
    }

    #[test]
    fn prd_round_trip_serde_toml() {
        let mut p = Prd::new();
        p.vision = "Ship maestro v1.0".into();
        p.add_goal("Stable TUI");
        p.non_goals.push("Multi-tenant SaaS".into());
        p.stakeholders.push(Stakeholder {
            name: "Carlos".into(),
            role: "Maintainer".into(),
        });
        p.timeline.push(TimelineMilestone::new("v0.16.0"));

        let serialized = toml::to_string(&p).expect("toml serialize");
        let back: Prd = toml::from_str(&serialized).expect("toml deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn prd_round_trip_serde_json() {
        let p = Prd::new();
        let json = serde_json::to_string(&p).expect("json serialize");
        let back: Prd = serde_json::from_str(&json).expect("json deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn prd_rejects_unknown_field() {
        let json = r#"{"version":1,"vision":"x","unknown":true}"#;
        let result: Result<Prd, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn add_goal_returns_id_and_appends() {
        let mut p = Prd::new();
        let id = p.add_goal("Test");
        assert_eq!(p.goals.len(), 1);
        assert_eq!(p.goals[0].id, id);
        assert_eq!(p.goals[0].text, "Test");
        assert!(!p.goals[0].done);
    }

    #[test]
    fn remove_goal_returns_true_when_found() {
        let mut p = Prd::new();
        let id = p.add_goal("Test");
        assert!(p.remove_goal(id));
        assert!(p.goals.is_empty());
    }

    #[test]
    fn remove_goal_returns_false_when_missing() {
        let mut p = Prd::new();
        assert!(!p.remove_goal(GoalId::new()));
    }

    #[test]
    fn current_state_completion_ratio_zero_when_no_issues() {
        let cs = CurrentState::default();
        assert_eq!(cs.completion_ratio(), 0.0);
    }

    #[test]
    fn current_state_completion_ratio_half_when_balanced() {
        let cs = CurrentState {
            open_issues: 5,
            closed_issues: 5,
            ..Default::default()
        };
        assert_eq!(cs.total_issues(), 10);
        assert!((cs.completion_ratio() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn current_state_completion_ratio_full_when_all_closed() {
        let cs = CurrentState {
            open_issues: 0,
            closed_issues: 10,
            ..Default::default()
        };
        assert!((cs.completion_ratio() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn timeline_status_round_trip() {
        for status in [
            TimelineStatus::Planned,
            TimelineStatus::InProgress,
            TimelineStatus::Completed,
            TimelineStatus::Cancelled,
        ] {
            let json = serde_json::to_string(&status).expect("serialize");
            let back: TimelineStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(status, back);
        }
    }
}
