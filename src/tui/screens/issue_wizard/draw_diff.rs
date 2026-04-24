//! Diff rendering for the Issue Wizard's AI improve step (#450). Pure
//! formatting helpers kept separate from the main `draw.rs` so the draw
//! surface stays within the 400-LOC project guardrail.

use super::IssueCreationPayload;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Canonical 8-field diff tuple: `(label, before, after)` in display order.
/// Callers render `before == after` as a single dim line and render the
/// rest with red-strikethrough old + green-prefixed new.
fn diff_fields<'a>(
    original: &'a IssueCreationPayload,
    candidate: &'a IssueCreationPayload,
) -> [(&'static str, &'a str, &'a str); 8] {
    [
        ("Title", &original.title, &candidate.title),
        ("Overview", &original.overview, &candidate.overview),
        (
            "Expected Behavior",
            &original.expected_behavior,
            &candidate.expected_behavior,
        ),
        (
            "Current Behavior",
            &original.current_behavior,
            &candidate.current_behavior,
        ),
        (
            "Steps to Reproduce",
            &original.steps_to_reproduce,
            &candidate.steps_to_reproduce,
        ),
        (
            "Acceptance Criteria",
            &original.acceptance_criteria,
            &candidate.acceptance_criteria,
        ),
        (
            "Files to Modify",
            &original.files_to_modify,
            &candidate.files_to_modify,
        ),
        ("Test Hints", &original.test_hints, &candidate.test_hints),
    ]
}

pub(super) fn build_diff_lines<'a>(
    original: &'a IssueCreationPayload,
    candidate: &'a IssueCreationPayload,
) -> Vec<Line<'a>> {
    let mut lines: Vec<Line<'a>> = Vec::new();
    for (label, before, after) in diff_fields(original, candidate) {
        if before == after {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}:", label),
                    Style::default().add_modifier(Modifier::DIM),
                ),
                Span::styled(" unchanged", Style::default().add_modifier(Modifier::DIM)),
            ]));
            continue;
        }
        lines.push(Line::from(Span::styled(
            format!("▼ {}", label),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for l in before.lines() {
            lines.push(Line::from(Span::styled(
                format!("  - {}", l),
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::CROSSED_OUT),
            )));
        }
        if before.is_empty() {
            lines.push(Line::from(Span::styled(
                "  - (empty)",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::CROSSED_OUT | Modifier::DIM),
            )));
        }
        for l in after.lines() {
            lines.push(Line::from(Span::styled(
                format!("  + {}", l),
                Style::default().fg(Color::Green),
            )));
        }
        if after.is_empty() {
            lines.push(Line::from(Span::styled(
                "  + (empty)",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::DIM),
            )));
        }
        lines.push(Line::from(""));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::super::{IssueCreationPayload, IssueType};
    use super::*;

    fn baseline() -> IssueCreationPayload {
        IssueCreationPayload {
            issue_type: IssueType::Feature,
            title: "Title".into(),
            overview: "Overview".into(),
            expected_behavior: "EB".into(),
            current_behavior: String::new(),
            steps_to_reproduce: String::new(),
            acceptance_criteria: "AC".into(),
            files_to_modify: "FTM".into(),
            test_hints: "TH".into(),
            blocked_by: vec![],
            milestone: None,
            image_paths: vec![],
        }
    }

    #[test]
    fn unchanged_field_renders_single_dim_line() {
        let p = baseline();
        let lines = build_diff_lines(&p, &p);
        // 8 unchanged fields → 8 lines, none of which contain `▼` or `+`.
        assert_eq!(lines.len(), 8);
        for line in &lines {
            let rendered: String = line
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<Vec<&str>>()
                .join("");
            assert!(rendered.contains("unchanged"), "got: {rendered:?}");
            assert!(!rendered.contains("▼"));
            assert!(!rendered.contains("+"));
        }
    }

    #[test]
    fn changed_field_renders_old_red_and_new_green() {
        let before = baseline();
        let mut after = before.clone();
        after.title = "New title".into();
        let lines = build_diff_lines(&before, &after);
        let flat: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<Vec<&str>>()
                    .join("")
            })
            .collect();
        let joined = flat.join("\n");
        assert!(joined.contains("▼ Title"), "got:\n{joined}");
        assert!(joined.contains("- Title"));
        assert!(joined.contains("+ New title"));
    }

    #[test]
    fn empty_to_filled_renders_empty_placeholder_and_new_content() {
        let before = baseline(); // current_behavior = ""
        let mut after = before.clone();
        after.current_behavior = "Now crashes".into();
        let lines = build_diff_lines(&before, &after);
        let flat: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<&str>>()
            .join("\n");
        assert!(flat.contains("- (empty)"));
        assert!(flat.contains("+ Now crashes"));
    }
}
