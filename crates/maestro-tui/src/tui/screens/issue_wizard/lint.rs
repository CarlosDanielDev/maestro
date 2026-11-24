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
pub fn placeholders(p: &IssueCreationPayload) -> Vec<Placeholder> {
    FieldId::ALL
        .iter()
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
