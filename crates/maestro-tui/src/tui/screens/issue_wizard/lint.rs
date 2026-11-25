//! #1282 Placeholder lint: the create gate's single source of truth. An AI
//! rewrite can leave `[TODO: …]` lines; while any is left in any of the 8
//! text fields, the issue cannot be created.

use super::{FieldId, IssueCreationPayload};

/// The marker the AI improve flow writes for a missing fact.
pub const MARKER: &str = "[TODO:";

/// One `[TODO:` left in a field, at a 0-based line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placeholder {
    pub field: FieldId,
    pub line: usize,
}

/// Every placeholder line, in `FieldId::ALL` order.
/// Only fields written for the issue type count: a `[TODO:` in a hidden
/// bug-only field of a Feature never reaches GitHub.
pub fn placeholders(p: &IssueCreationPayload) -> Vec<Placeholder> {
    FieldId::ALL
        .iter()
        .filter(|f| f.is_written_for(p.issue_type))
        .flat_map(|&field| {
            field
                .payload_ref(p)
                .lines()
                .enumerate()
                .filter(|(_, l)| l.contains(MARKER))
                .map(move |(line, _)| Placeholder { field, line })
        })
        .collect()
}

/// Every `[TODO:` in the payload's 8 text fields: the one count the gate,
/// the Draft checklist and the AI section rows all show.
pub fn total(p: &IssueCreationPayload) -> usize {
    FieldId::ALL
        .iter()
        .filter(|f| f.is_written_for(p.issue_type))
        .map(|f| count(f.payload_ref(p)))
        .sum()
}

/// Number of placeholders in one text.
pub fn count(text: &str) -> usize {
    text.matches(MARKER).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_placeholders_in_every_field_in_order() {
        let p = IssueCreationPayload {
            title: "feat: [TODO: name it]".into(),
            acceptance_criteria: "- ok\n- [TODO: limit]".into(),
            test_hints: "[TODO: x] and [TODO: y]".into(),
            ..Default::default()
        };
        assert_eq!(
            placeholders(&p),
            vec![
                Placeholder {
                    field: FieldId::Title,
                    line: 0
                },
                Placeholder {
                    field: FieldId::AcceptanceCriteria,
                    line: 1
                },
                Placeholder {
                    field: FieldId::TestHints,
                    line: 0
                },
            ]
        );
        assert_eq!(count(&p.test_hints), 2);
        assert_eq!(total(&p), 4, "occurrences, not lines");
    }

    #[test]
    fn hidden_bug_fields_of_a_feature_are_not_linted() {
        let mut p = IssueCreationPayload {
            current_behavior: "[TODO: what breaks]".into(),
            ..Default::default()
        };
        assert!(placeholders(&p).is_empty());
        assert_eq!(total(&p), 0);
        p.issue_type = crate::tui::screens::issue_wizard::IssueType::Bug;
        assert_eq!(total(&p), 1);
        assert_eq!(placeholders(&p)[0].field, FieldId::CurrentBehavior);
    }

    #[test]
    fn plain_todo_words_are_not_placeholders() {
        let p = IssueCreationPayload {
            overview: "TODO list app; [TODO] without a colon".into(),
            ..Default::default()
        };
        assert!(placeholders(&p).is_empty());
    }
}
