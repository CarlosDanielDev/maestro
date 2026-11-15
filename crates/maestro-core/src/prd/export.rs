//! Markdown export for the PRD (#321). Pure function, snapshot-friendly.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #321. `to_markdown` is invoked by the
// PRD screen export action + `maestro prd --export` subcommand in Phase 2.
#![allow(dead_code)]

use crate::prd::model::{Prd, TimelineStatus};
use std::fmt::Write as _;

const PLACEHOLDER: &str = "_(not yet defined)_";

pub fn to_markdown(prd: &Prd) -> String {
    let mut out = String::new();
    section_vision(prd, &mut out);
    section_goals(prd, &mut out);
    section_non_goals(prd, &mut out);
    section_current_state(prd, &mut out);
    section_stakeholders(prd, &mut out);
    section_timeline(prd, &mut out);
    out
}

fn section_vision(prd: &Prd, out: &mut String) {
    out.push_str("# Product Requirements Document\n\n");
    out.push_str("## Vision & Purpose\n\n");
    if prd.vision.trim().is_empty() {
        out.push_str(PLACEHOLDER);
    } else {
        out.push_str(prd.vision.trim());
    }
    out.push_str("\n\n");
}

fn section_goals(prd: &Prd, out: &mut String) {
    out.push_str("## Goals\n\n");
    if prd.goals.is_empty() {
        out.push_str(PLACEHOLDER);
        out.push_str("\n\n");
        return;
    }
    for g in &prd.goals {
        let mark = if g.done { "x" } else { " " };
        let _ = writeln!(out, "- [{mark}] {}", g.text);
    }
    out.push('\n');
}

fn section_non_goals(prd: &Prd, out: &mut String) {
    out.push_str("## Non-Goals\n\n");
    if prd.non_goals.is_empty() {
        out.push_str(PLACEHOLDER);
        out.push_str("\n\n");
        return;
    }
    for ng in &prd.non_goals {
        let _ = writeln!(out, "- {ng}");
    }
    out.push('\n');
}

fn section_current_state(prd: &Prd, out: &mut String) {
    let cs = &prd.current_state;
    out.push_str("## Current State\n\n");
    let _ = writeln!(
        out,
        "- **Issues**: {} closed / {} total ({:.0}% complete)",
        cs.closed_issues,
        cs.total_issues(),
        cs.completion_ratio() * 100.0
    );
    let _ = writeln!(
        out,
        "- **Milestones**: {} closed / {} open",
        cs.closed_milestones, cs.open_milestones,
    );
    if !cs.top_blockers.is_empty() {
        out.push_str("- **Top blockers**: ");
        let mut first = true;
        for n in &cs.top_blockers {
            if !first {
                out.push_str(", ");
            }
            let _ = write!(out, "#{n}");
            first = false;
        }
        out.push('\n');
    }
    out.push('\n');
}

fn section_stakeholders(prd: &Prd, out: &mut String) {
    out.push_str("## Stakeholders\n\n");
    if prd.stakeholders.is_empty() {
        out.push_str(PLACEHOLDER);
        out.push_str("\n\n");
        return;
    }
    out.push_str("| Name | Role |\n|------|------|\n");
    for s in &prd.stakeholders {
        let _ = writeln!(out, "| {} | {} |", s.name, s.role);
    }
    out.push('\n');
}

fn section_timeline(prd: &Prd, out: &mut String) {
    out.push_str("## Timeline\n\n");
    if prd.timeline.is_empty() {
        out.push_str(PLACEHOLDER);
        out.push_str("\n\n");
        return;
    }
    for tm in &prd.timeline {
        let status = match tm.status {
            TimelineStatus::Planned => "planned",
            TimelineStatus::InProgress => "in-progress",
            TimelineStatus::Completed => "completed",
            TimelineStatus::Cancelled => "cancelled",
        };
        match tm.target_date {
            Some(d) => {
                let _ = writeln!(
                    out,
                    "- **{}** ({}, {status}) — {:.0}% complete",
                    tm.name,
                    d.format("%Y-%m-%d"),
                    tm.progress * 100.0
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "- **{}** (unscheduled, {status}) — {:.0}% complete",
                    tm.name,
                    tm.progress * 100.0
                );
            }
        }
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prd::model::{CurrentState, Goal, Stakeholder, TimelineMilestone, TimelineStatus};

    fn fully_populated() -> Prd {
        let mut p = Prd::new();
        p.vision = "Make maestro the standard for AI-assisted dev loops.".into();
        p.goals.push(Goal {
            text: "Stable TUI".into(),
            done: true,
            ..Goal::new("Stable TUI")
        });
        p.goals.push(Goal::new("PR review automation"));
        p.non_goals.push("Multi-tenant SaaS".into());
        p.current_state = CurrentState {
            open_issues: 4,
            closed_issues: 6,
            open_milestones: 1,
            closed_milestones: 5,
            top_blockers: vec![327, 328],
        };
        p.stakeholders.push(Stakeholder {
            name: "Carlos".into(),
            role: "Maintainer".into(),
        });
        let mut tm = TimelineMilestone::new("v0.16.0");
        tm.status = TimelineStatus::InProgress;
        tm.progress = 0.25;
        p.timeline.push(tm);
        p
    }

    #[test]
    fn export_includes_all_section_headers() {
        let md = to_markdown(&Prd::new());
        for header in [
            "# Product Requirements Document",
            "## Vision & Purpose",
            "## Goals",
            "## Non-Goals",
            "## Current State",
            "## Stakeholders",
            "## Timeline",
        ] {
            assert!(md.contains(header), "missing header: {header}\n{md}");
        }
    }

    #[test]
    fn empty_sections_render_placeholder() {
        let md = to_markdown(&Prd::new());
        assert!(md.contains(PLACEHOLDER));
    }

    #[test]
    fn populated_goal_renders_with_checkbox() {
        let prd = fully_populated();
        let md = to_markdown(&prd);
        assert!(md.contains("- [x] Stable TUI"));
        assert!(md.contains("- [ ] PR review automation"));
    }

    #[test]
    fn current_state_shows_completion_percentage() {
        let prd = fully_populated();
        let md = to_markdown(&prd);
        assert!(md.contains("60%"), "expected 60% complete; got:\n{md}");
    }

    #[test]
    fn current_state_lists_top_blockers_when_present() {
        let prd = fully_populated();
        let md = to_markdown(&prd);
        assert!(md.contains("**Top blockers**: #327, #328"));
    }

    #[test]
    fn stakeholders_render_as_table() {
        let prd = fully_populated();
        let md = to_markdown(&prd);
        assert!(md.contains("| Name | Role |"));
        assert!(md.contains("| Carlos | Maintainer |"));
    }

    #[test]
    fn timeline_includes_status_label_and_progress() {
        let prd = fully_populated();
        let md = to_markdown(&prd);
        assert!(md.contains("**v0.16.0**"));
        assert!(md.contains("in-progress"));
        assert!(md.contains("25% complete"));
    }

    #[test]
    fn fully_populated_prd_does_not_emit_placeholder() {
        let md = to_markdown(&fully_populated());
        assert!(
            !md.contains(PLACEHOLDER),
            "fully populated PRD should have no placeholders\n{md}"
        );
    }
}
