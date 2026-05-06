//! L3 cross-issue scheduler — builds launch plans and surfaces ready work.

#![allow(dead_code)]

use crate::orchestration::dag::{
    Edge, ExpandResult, IssueMeta, auto_expand, classify_edges, topo_levels,
};
use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::TeamInput;
use crate::state::types::{IssueNumber, IssueRunState, TeamRun};
use anyhow::{Result, anyhow};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Scheduler {
    pub team: ResolvedTeam,
    pub run: TeamRun,
    pub edges: HashMap<IssueNumber, Vec<Edge>>,
    pub max_parallel: usize,
    pub auto_added: Vec<IssueNumber>,
}

impl Scheduler {
    pub fn from_input(
        team: ResolvedTeam,
        input: TeamInput,
        metas: HashMap<IssueNumber, IssueMeta>,
        max_parallel: usize,
    ) -> Result<Self> {
        let (selected, primary_ms) = match input {
            TeamInput::Issue { number } => (HashSet::from([number]), None),
            TeamInput::IssueSet {
                primary_milestone,
                issues,
            } => (issues.into_iter().collect(), primary_milestone),
            TeamInput::IdeaInbox => (HashSet::new(), None),
        };

        let initial_edges = classify_edges(&selected, primary_ms, &metas);
        let (selected, auto_added) = match auto_expand(selected, &initial_edges) {
            ExpandResult::NoChange { selected } => (selected, Vec::new()),
            ExpandResult::Expanded { selected, added } => (selected, added),
            ExpandResult::TooLarge {
                original,
                would_be,
                added,
            } => {
                return Err(anyhow!(
                    "auto-expansion refused: original {original} would become {would_be} ({added:?})"
                ));
            }
        };

        let edges = classify_edges(&selected, primary_ms, &metas);
        let plan = topo_levels(&selected, &edges)?;

        let mut state = HashMap::new();
        for &n in &selected {
            state.insert(n, IssueRunState::Queued);
        }

        let run = TeamRun {
            id: Uuid::new_v4(),
            team_name: team.name.clone(),
            started_at: Utc::now(),
            plan,
            state,
        };

        Ok(Self {
            team,
            run,
            edges,
            max_parallel,
            auto_added,
        })
    }

    /// Issues eligible to spawn now: queued, deps succeeded, slots available.
    pub fn next_ready(&self) -> Vec<IssueNumber> {
        let in_flight = self
            .run
            .state
            .values()
            .filter(|s| matches!(s, IssueRunState::InFlight { .. }))
            .count();
        let slots = self.max_parallel.saturating_sub(in_flight);
        if slots == 0 {
            return Vec::new();
        }

        let mut ready = Vec::new();
        for level in &self.run.plan {
            let mut level_open = false;
            for &issue in level {
                match self.run.state.get(&issue) {
                    Some(IssueRunState::Queued) => {
                        level_open = true;
                        let deps_succeeded = self
                            .edges
                            .get(&issue)
                            .into_iter()
                            .flatten()
                            .filter_map(|e| match e {
                                Edge::InSlice(dep) => Some(dep),
                                _ => None,
                            })
                            .all(|dep| {
                                matches!(
                                    self.run.state.get(dep),
                                    Some(IssueRunState::Succeeded { .. })
                                )
                            });
                        if deps_succeeded {
                            ready.push(issue);
                        }
                    }
                    Some(IssueRunState::InFlight { .. }) => level_open = true,
                    Some(IssueRunState::Failed { .. }) | Some(IssueRunState::Blocked { .. }) => {
                        level_open = true
                    }
                    Some(IssueRunState::Succeeded { .. }) => {}
                    None => {}
                }
            }
            if level_open {
                break;
            }
        }

        if ready.len() > slots {
            ready.truncate(slots);
        }
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::dag::IssueState;
    use crate::orchestration::team::{RoleBinding, SourceTier};
    use crate::orchestration::types::{Primitive, TeamOutput, TeamRole};

    fn solo_team() -> ResolvedTeam {
        let mut bindings = HashMap::new();
        bindings.insert(
            TeamRole::Reviewer,
            RoleBinding {
                agent: "claude".into(),
                mode: None,
                model_override: None,
                prompt_addendum: None,
                fallback_agent: None,
            },
        );
        ResolvedTeam {
            name: "default-reviewer".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::BuiltIn,
        }
    }

    fn meta(
        n: IssueNumber,
        state: IssueState,
        milestone: Option<u64>,
        blocked_by: Vec<IssueNumber>,
    ) -> IssueMeta {
        IssueMeta {
            number: n,
            state,
            milestone,
            blocked_by,
        }
    }

    #[test]
    fn single_issue_ready_immediately() {
        let mut metas = HashMap::new();
        metas.insert(547, meta(547, IssueState::Open, Some(49), vec![]));
        let sched =
            Scheduler::from_input(solo_team(), TeamInput::Issue { number: 547 }, metas, 3).unwrap();
        assert_eq!(sched.next_ready(), vec![547]);
    }

    #[test]
    fn second_level_waits_for_first_to_succeed() {
        let mut metas = HashMap::new();
        metas.insert(547, meta(547, IssueState::Open, Some(49), vec![]));
        metas.insert(549, meta(549, IssueState::Open, Some(49), vec![547]));
        let mut sched = Scheduler::from_input(
            solo_team(),
            TeamInput::IssueSet {
                primary_milestone: Some(49),
                issues: vec![547, 549],
            },
            metas,
            3,
        )
        .unwrap();
        assert_eq!(sched.next_ready(), vec![547]);

        sched.run.state.insert(
            547,
            IssueRunState::Succeeded {
                output: TeamOutput::Pr {
                    number: 1,
                    branch: "x".into(),
                },
            },
        );
        let ready = sched.next_ready();
        assert_eq!(ready, vec![549]);
    }

    #[test]
    fn auto_expand_includes_same_milestone_open_external() {
        let mut metas = HashMap::new();
        metas.insert(1, meta(1, IssueState::Open, Some(1), vec![2]));
        metas.insert(2, meta(2, IssueState::Open, Some(1), vec![]));
        let sched = Scheduler::from_input(
            solo_team(),
            TeamInput::IssueSet {
                primary_milestone: Some(1),
                issues: vec![1],
            },
            metas,
            3,
        )
        .unwrap();
        assert!(sched.run.state.contains_key(&2));
        assert_eq!(sched.auto_added, vec![2]);
    }
}
