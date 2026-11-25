//! The Plan check and the Create gate of the milestone wizard (#1284,
//! §02.3): a selected issue whose blocker is deselected blocks advancing
//! (never dropped silently), and Create is an Armed-Enter write that states
//! the milestone and the issue count.

use super::types::stranded;
use super::{MilestoneWizardScreen, MilestoneWizardStep, level_buckets};
use crate::tui::ds::armed::{ArmedState, EnterOutcome};
use std::time::Instant;

/// Shown on the Plan border while a selected issue needs a deselected one.
pub(super) const STRANDED_REASON: &str =
    "a selected issue needs a deselected one · select it with a, or drop this one with x";
/// Shown when selected issues block each other in a loop: no creation order.
pub(super) const CYCLE_REASON: &str = "selected issues block each other in a loop";

impl MilestoneWizardScreen {
    pub(super) fn now(&self) -> Instant {
        self.clock.unwrap_or_else(Instant::now)
    }

    #[cfg(test)]
    pub fn set_clock_for_test(&mut self, now: Instant) {
        self.clock = Some(now);
    }

    /// Selected issues whose blocker is deselected (or out of range), with
    /// those blockers. Empty without a plan.
    pub fn stranded_issues(&self) -> Vec<(usize, Vec<usize>)> {
        self.generated_plan()
            .map(|p| stranded(&p.issues))
            .unwrap_or_default()
    }

    /// Why the Plan step cannot advance: a stranded issue, or a selected
    /// issue `level_buckets` cannot place (a loop of blockers).
    pub(super) fn plan_block_reason(&self) -> Option<&'static str> {
        let plan = self.generated_plan()?;
        if !stranded(&plan.issues).is_empty() {
            return Some(STRANDED_REASON);
        }
        let placed: usize = level_buckets(&plan.issues).iter().map(Vec::len).sum();
        let selected = plan.issues.iter().filter(|i| i.accepted).count();
        (placed != selected).then_some(CYCLE_REASON)
    }

    /// Selected issues: what Create writes besides the milestone.
    pub fn selected_issue_count(&self) -> usize {
        self.generated_plan()
            .map(|p| p.issues.iter().filter(|i| i.accepted).count())
            .unwrap_or(0)
    }

    /// The exact write, for the armed bar: `creates 1 milestone + 2 issues`.
    pub fn create_description(&self) -> String {
        let n = self.selected_issue_count();
        format!(
            "creates 1 {} + {n} issue{}",
            self.milestone_label_lowercase(),
            if n == 1 { "" } else { "s" }
        )
    }

    pub fn create_arm_state(&self) -> &ArmedState {
        self.create_arm.state()
    }

    pub fn not_created(&self) -> &[String] {
        &self.not_created
    }

    /// ⏎ on Create: the first arms, a second within `ARM_WINDOW` writes.
    pub(super) fn on_create_enter(&mut self) {
        if self.step() != MilestoneWizardStep::Preview {
            return;
        }
        if self.create_arm.on_enter(self.now()) == EnterOutcome::Commit {
            self.begin_materialization();
        }
    }

    /// Right of the stepper: what has reached the provider so far.
    pub(super) fn writes_ledger(&self) -> String {
        let written = self.created_issue_numbers().len();
        match self.step() {
            MilestoneWizardStep::Complete => format!("{written} issues written"),
            MilestoneWizardStep::Materializing => "writing…".to_string(),
            MilestoneWizardStep::Failed if self.created_milestone_number().is_some() => {
                let total = written + self.not_created.len();
                format!("{written} of {total} issues written")
            }
            MilestoneWizardStep::Preview
                if matches!(self.create_arm_state(), ArmedState::Armed(_)) =>
            {
                "armed · nothing written yet".to_string()
            }
            _ => "nothing written yet".to_string(),
        }
    }
}

#[cfg(test)]
#[path = "create_tests.rs"]
mod tests;
