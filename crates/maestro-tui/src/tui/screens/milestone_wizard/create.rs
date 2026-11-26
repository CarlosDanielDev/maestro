//! The Plan check and the Create gate of the milestone wizard (#1284,
//! §02.3): a selected issue whose blocker is deselected blocks advancing
//! (never dropped silently), and Create is an Armed-Enter write that states
//! the milestone and the issue count.

use super::types::stranded;
use super::{AiProposedIssue, MilestoneWizardScreen, MilestoneWizardStep, level_buckets};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::ds::armed::{ArmedState, EnterOutcome};
use std::time::Instant;

/// Shown on the Plan border while a selected issue needs a deselected one.
pub(super) const STRANDED_REASON: &str =
    "a selected issue needs a deselected one · select it with a, or drop this one with x";
/// Shown when selected issues block each other in a loop: no creation order.
pub(super) const CYCLE_REASON: &str = "selected issues block each other in a loop";
/// Shown after the repeated title when two selected issues have
/// equivalent titles (#1455): the provider reuses an issue by title, so one
/// of them would be lost. The border reads `"title" is planned twice · …`.
pub(super) const DUPLICATE_REASON: &str = "is planned twice · drop one with x";

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

    /// Why the Plan step cannot advance: a stranded issue, two selected
    /// issues with equivalent titles, or a selected issue `level_buckets`
    /// cannot place (a loop of blockers).
    pub(super) fn plan_block_reason(&self) -> Option<&'static str> {
        let plan = self.generated_plan()?;
        if !stranded(&plan.issues).is_empty() {
            return Some(STRANDED_REASON);
        }
        if !duplicates(&plan.issues).is_empty() {
            return Some(DUPLICATE_REASON);
        }
        let placed: usize = level_buckets(&plan.issues).iter().map(Vec::len).sum();
        let selected = plan.issues.iter().filter(|i| i.accepted).count();
        (placed != selected).then_some(CYCLE_REASON)
    }

    /// Selected issues that repeat the title of an earlier selected one,
    /// with that earlier issue (#1455). Empty without a plan.
    pub fn duplicate_issues(&self) -> Vec<(usize, usize)> {
        self.generated_plan()
            .map(|p| duplicates(&p.issues))
            .unwrap_or_default()
    }

    /// The Plan border text: the block reason, naming the repeated title
    /// when two selected issues share one.
    pub(super) fn plan_border_reason(&self) -> Option<String> {
        let reason = self.validation_error()?;
        if reason != DUPLICATE_REASON {
            return Some(reason.to_string());
        }
        let (_, first) = *self.duplicate_issues().first()?;
        let title = &self.generated_plan()?.issues[first].title;
        let mut short: String = title.trim().chars().take(30).collect();
        if title.trim().chars().count() > 30 {
            short.push('…');
        }
        Some(format!("\"{}\" {DUPLICATE_REASON}", sanitize_for_terminal(&short)))
    }

    /// Selected issues: what Create writes besides the milestone.
    pub fn selected_issue_count(&self) -> usize {
        self.generated_plan()
            .map(|p| p.issues.iter().filter(|i| i.accepted).count())
            .unwrap_or(0)
    }

    /// Selected issues an earlier partial create already wrote.
    fn selected_already_written(&self) -> usize {
        self.generated_plan()
            .and_then(|p| {
                let w = p.written.as_ref()?;
                Some(
                    p.issues
                        .iter()
                        .enumerate()
                        .filter(|(i, it)| it.accepted && w.issues.contains_key(i))
                        .count(),
                )
            })
            .unwrap_or(0)
    }

    /// The exact write, for the armed bar: `creates 1 milestone + 2 issues`,
    /// or after a partial create `creates 2 issues · reuses milestone #9 and
    /// 1 issue` (#1284).
    pub fn create_description(&self) -> String {
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        let n = self.selected_issue_count();
        let label = self.milestone_label_lowercase();
        match self.generated_plan().and_then(|p| p.written.as_ref()) {
            Some(w) => {
                let k = self.selected_already_written();
                let new = n - k;
                format!(
                    "creates {new} issue{} · reuses {label} #{} and {k} issue{}",
                    plural(new),
                    w.milestone_number,
                    plural(k)
                )
            }
            None => format!("creates 1 {label} + {n} issue{}", plural(n)),
        }
    }

    /// For the armed bar after a partial create: `1 of 3 issues already exist`.
    pub(super) fn already_written_note(&self) -> Option<String> {
        self.generated_plan()?.written.as_ref()?;
        Some(format!(
            "{} of {} issues already exist",
            self.selected_already_written(),
            self.selected_issue_count()
        ))
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
        let in_place = self.created_issue_numbers().len() + self.skipped_issue_numbers().len();
        let armed = matches!(self.create_arm_state(), ArmedState::Armed(_));
        let earlier = self.generated_plan().and_then(|p| p.written.as_ref());
        match self.step() {
            MilestoneWizardStep::Complete => format!("{in_place} issues written"),
            MilestoneWizardStep::Materializing => "writing…".to_string(),
            MilestoneWizardStep::Failed if self.created_milestone_number().is_some() => {
                let total = in_place + self.not_created.len();
                format!("{in_place} of {total} issues exist")
            }
            MilestoneWizardStep::Preview if earlier.is_some() => format!(
                "{}{} of {} issues exist",
                if armed { "armed · " } else { "" },
                self.selected_already_written(),
                self.selected_issue_count()
            ),
            MilestoneWizardStep::Preview if armed => "armed · nothing written yet".to_string(),
            _ => "nothing written yet".to_string(),
        }
    }
}

/// Selected issues whose title is equivalent (`util::titles_equivalent`,
/// the provider's own match) to an earlier selected issue, each with the
/// first such issue. Dropping these leaves one issue per title.
pub fn duplicates(issues: &[AiProposedIssue]) -> Vec<(usize, usize)> {
    let selected = || issues.iter().enumerate().filter(|(_, it)| it.accepted);
    selected()
        .filter_map(|(i, it)| {
            selected()
                .take_while(|(j, _)| *j < i)
                .find(|(_, earlier)| crate::util::titles_equivalent(&earlier.title, &it.title))
                .map(|(j, _)| (i, j))
        })
        .collect()
}

#[cfg(test)]
#[path = "create_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "retry_tests.rs"]
mod retry_tests;

#[cfg(test)]
#[path = "duplicate_tests.rs"]
mod duplicate_tests;
