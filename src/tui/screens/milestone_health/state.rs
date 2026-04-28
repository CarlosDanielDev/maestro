//! State machine for the milestone health-check wizard (#500).
//!
//! Pure reducer — no I/O, no rendering, no panics. The screen wraps this
//! and converts side effects into `TuiCommand`s.

use crossterm::event::KeyCode;

use crate::milestone_health::report::HealthReport;
use crate::milestone_health::{analyze, check_issues, generate_patch};
use crate::provider::github::types::{GhIssue, GhMilestone};
use crate::tui::screens::milestone_health::diff::{DiffLine, diff_lines};

#[derive(Debug, Clone)]
pub enum HealthStep {
    Picker {
        milestones: Vec<GhMilestone>,
        selected: usize,
    },
    Loading {
        label: String,
        /// Set when this Loading step is for an issue-fetch initiated from
        /// the picker — caches the user's selection so the dispatch layer
        /// can embed it in the `TuiCommand` instead of re-fetching the
        /// milestone list. `None` when loading the milestone list itself.
        milestone: Option<GhMilestone>,
    },
    Empty {
        milestone: GhMilestone,
    },
    Healthy {
        milestone: GhMilestone,
    },
    Report {
        milestone: GhMilestone,
        issues: Vec<GhIssue>,
    },
    Patch {
        milestone: GhMilestone,
        proposed: String,
        /// Precomputed before/after diff so `draw_patch` doesn't recompute
        /// the LCS on every render frame.
        diff: Vec<DiffLine>,
    },
    Confirm {
        milestone: GhMilestone,
        proposed: String,
    },
    Writing {
        milestone: GhMilestone,
        last_proposed: String,
    },
    Result {
        milestone: GhMilestone,
        outcome: PatchOutcome,
    },
    /// Terminal state for fetch-side errors (no milestone selected yet).
    /// Distinct from `Result { Error }` so the retry path is unambiguous.
    FetchError {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum PatchOutcome {
    Success,
    Error {
        message: String,
        retryable: bool,
        last_proposed: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct HealthScreenState {
    pub step: HealthStep,
    /// Last computed report — surfaced on Report / Healthy / Empty screens.
    pub report: Option<HealthReport>,
}

impl Default for HealthStep {
    fn default() -> Self {
        Self::Loading {
            label: "Loading milestones…".to_string(),
            milestone: None,
        }
    }
}

#[derive(Debug)]
pub enum HealthInput {
    Key(KeyCode),
    MilestonesLoaded(anyhow::Result<Vec<GhMilestone>>),
    DataFetched(anyhow::Result<(GhMilestone, Vec<GhIssue>)>),
    DataPatched(anyhow::Result<()>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthSideEffect {
    None,
    DispatchFetchMilestones,
    DispatchFetchIssues {
        milestone_number: u64,
        milestone_title: String,
    },
    DispatchPatch {
        milestone_number: u64,
        description: String,
    },
    Pop,
}

fn loading_milestones_step() -> HealthStep {
    HealthStep::Loading {
        label: "Loading milestones…".to_string(),
        milestone: None,
    }
}

impl HealthScreenState {
    pub fn new() -> Self {
        Self {
            step: loading_milestones_step(),
            report: None,
        }
    }

    /// Drive the state machine. Pure: no I/O, no panics. Returns the
    /// side effect the outer screen should enqueue.
    pub fn transition(&mut self, input: HealthInput) -> HealthSideEffect {
        match (input, &self.step) {
            // --- Loading-for-milestones-list ---
            (HealthInput::MilestonesLoaded(Ok(milestones)), HealthStep::Loading { .. }) => {
                self.step = HealthStep::Picker {
                    milestones,
                    selected: 0,
                };
                HealthSideEffect::None
            }
            (HealthInput::MilestonesLoaded(Err(e)), HealthStep::Loading { .. }) => {
                self.step = HealthStep::FetchError {
                    message: format!("{}", e),
                };
                HealthSideEffect::None
            }

            // --- Picker ---
            (HealthInput::Key(KeyCode::Up | KeyCode::Char('k')), HealthStep::Picker { .. }) => {
                if let HealthStep::Picker { selected, .. } = &mut self.step
                    && *selected > 0
                {
                    *selected -= 1;
                }
                HealthSideEffect::None
            }
            (HealthInput::Key(KeyCode::Down | KeyCode::Char('j')), HealthStep::Picker { .. }) => {
                if let HealthStep::Picker {
                    milestones,
                    selected,
                } = &mut self.step
                    && *selected + 1 < milestones.len()
                {
                    *selected += 1;
                }
                HealthSideEffect::None
            }
            (
                HealthInput::Key(KeyCode::Enter),
                HealthStep::Picker {
                    milestones,
                    selected,
                },
            ) => {
                if let Some(ms) = milestones.get(*selected) {
                    let number = ms.number;
                    let title = ms.title.clone();
                    self.step = HealthStep::Loading {
                        label: format!("Fetching issues for '{}'…", title),
                        milestone: Some(ms.clone()),
                    };
                    HealthSideEffect::DispatchFetchIssues {
                        milestone_number: number,
                        milestone_title: title,
                    }
                } else {
                    HealthSideEffect::None
                }
            }
            (HealthInput::Key(KeyCode::Char('r')), HealthStep::Picker { .. }) => {
                self.step = loading_milestones_step();
                HealthSideEffect::DispatchFetchMilestones
            }
            (HealthInput::Key(KeyCode::Esc | KeyCode::Char('q')), HealthStep::Picker { .. }) => {
                HealthSideEffect::Pop
            }

            // --- Loading-for-issues / generic Loading ---
            (HealthInput::DataFetched(Ok((milestone, issues))), HealthStep::Loading { .. }) => {
                if issues.is_empty() {
                    self.report = Some(HealthReport::default());
                    self.step = HealthStep::Empty { milestone };
                    return HealthSideEffect::None;
                }
                let dor = check_issues(&issues);
                let anomalies = analyze(&milestone.description, &issues);
                let report = HealthReport { dor, anomalies };
                let healthy = report.is_healthy();
                self.report = Some(report);
                self.step = if healthy {
                    HealthStep::Healthy { milestone }
                } else {
                    HealthStep::Report { milestone, issues }
                };
                HealthSideEffect::None
            }
            (HealthInput::DataFetched(Err(e)), HealthStep::Loading { .. }) => {
                self.step = HealthStep::FetchError {
                    message: format!("{}", e),
                };
                HealthSideEffect::None
            }
            (HealthInput::Key(KeyCode::Esc | KeyCode::Char('q')), HealthStep::Loading { .. }) => {
                self.step = loading_milestones_step();
                HealthSideEffect::DispatchFetchMilestones
            }

            // --- Healthy / Empty (terminal-ish; Esc → re-fetch picker) ---
            (HealthInput::Key(_), HealthStep::Healthy { .. } | HealthStep::Empty { .. }) => {
                self.step = loading_milestones_step();
                HealthSideEffect::DispatchFetchMilestones
            }

            // --- Report ---
            (HealthInput::Key(KeyCode::Enter), HealthStep::Report { milestone, issues }) => {
                let anomalies = self
                    .report
                    .as_ref()
                    .map(|r| r.anomalies.clone())
                    .unwrap_or_default();
                let proposed = generate_patch(milestone, issues, &anomalies);
                let diff = diff_lines(&milestone.description, &proposed);
                let milestone = milestone.clone();
                self.step = HealthStep::Patch {
                    milestone,
                    proposed,
                    diff,
                };
                HealthSideEffect::None
            }
            (HealthInput::Key(KeyCode::Esc | KeyCode::Char('q')), HealthStep::Report { .. }) => {
                self.step = loading_milestones_step();
                HealthSideEffect::DispatchFetchMilestones
            }

            // --- Patch ---
            (
                HealthInput::Key(KeyCode::Enter),
                HealthStep::Patch {
                    milestone,
                    proposed,
                    ..
                },
            ) => {
                let milestone = milestone.clone();
                let proposed = proposed.clone();
                self.step = HealthStep::Confirm {
                    milestone,
                    proposed,
                };
                HealthSideEffect::None
            }
            (HealthInput::Key(KeyCode::Esc | KeyCode::Char('q')), HealthStep::Patch { .. }) => {
                self.step = loading_milestones_step();
                HealthSideEffect::DispatchFetchMilestones
            }

            // --- Confirm — the single GitHub-write gate ---
            (
                HealthInput::Key(KeyCode::Enter),
                HealthStep::Confirm {
                    milestone,
                    proposed,
                },
            ) => {
                let milestone_number = milestone.number;
                let description = proposed.clone();
                let milestone = milestone.clone();
                let proposed = proposed.clone();
                self.step = HealthStep::Writing {
                    milestone,
                    last_proposed: proposed,
                };
                HealthSideEffect::DispatchPatch {
                    milestone_number,
                    description,
                }
            }
            (HealthInput::Key(KeyCode::Esc | KeyCode::Char('q')), HealthStep::Confirm { .. }) => {
                self.step = loading_milestones_step();
                HealthSideEffect::DispatchFetchMilestones
            }

            // --- Writing — Esc is intentionally ignored ---
            (HealthInput::Key(_), HealthStep::Writing { .. }) => HealthSideEffect::None,
            (HealthInput::DataPatched(Ok(())), HealthStep::Writing { milestone, .. }) => {
                self.step = HealthStep::Result {
                    milestone: milestone.clone(),
                    outcome: PatchOutcome::Success,
                };
                HealthSideEffect::None
            }
            (
                HealthInput::DataPatched(Err(e)),
                HealthStep::Writing {
                    milestone,
                    last_proposed,
                },
            ) => {
                let proposed = last_proposed.clone();
                self.step = HealthStep::Result {
                    milestone: milestone.clone(),
                    outcome: PatchOutcome::Error {
                        message: format!("{}", e),
                        retryable: true,
                        last_proposed: proposed,
                    },
                };
                HealthSideEffect::None
            }

            // --- Result ---
            (
                HealthInput::Key(KeyCode::Char('r')),
                HealthStep::Result {
                    milestone,
                    outcome:
                        PatchOutcome::Error {
                            last_proposed,
                            retryable: true,
                            ..
                        },
                },
            ) => {
                let milestone_number = milestone.number;
                let description = last_proposed.clone();
                let milestone = milestone.clone();
                self.step = HealthStep::Writing {
                    milestone,
                    last_proposed: description.clone(),
                };
                HealthSideEffect::DispatchPatch {
                    milestone_number,
                    description,
                }
            }
            (HealthInput::Key(_), HealthStep::Result { .. } | HealthStep::FetchError { .. }) => {
                self.step = loading_milestones_step();
                HealthSideEffect::DispatchFetchMilestones
            }

            // --- Catch-all: ignore stray data events while we're not
            //     in the matching loading state. Defensive — we've seen
            //     races on rapid Esc + retry sequences.
            _ => HealthSideEffect::None,
        }
    }
}

#[cfg(test)]
mod tests;
