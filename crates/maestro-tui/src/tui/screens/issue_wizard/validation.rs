//! #1282 Validation: every required-field error at once. It feeds the
//! field borders (touched fields only), the Draft checklist (every field),
//! the stepper's `✗` and `try_advance` (focus the first error).

use super::{FieldId, IssueWizardScreen, lint};
use crate::tui::ds::checklist::CheckState;
use std::borrow::Cow;

/// The required fields, in checklist order.
pub const REQUIRED: [FieldId; 4] = [
    FieldId::Title,
    FieldId::Overview,
    FieldId::ExpectedBehavior,
    FieldId::AcceptanceCriteria,
];

/// The checklist row for the placeholder lint.
pub const NO_PLACEHOLDERS: &str = "No [TODO] placeholders";

impl IssueWizardScreen {
    /// Every error of the draft, read from the live fields, in field order.
    pub fn validation_errors(&self) -> Vec<(FieldId, Cow<'static, str>)> {
        let mut out = Vec::new();
        let title = self.field_text(FieldId::Title);
        // The canonical validator from #452, also run at dispatch time.
        if crate::util::validate_title(&title, "title").is_err() {
            let msg = if title.trim().is_empty() {
                "required"
            } else {
                "too long or leading '-'"
            };
            out.push((FieldId::Title, Cow::Borrowed(msg)));
        }
        for field in &REQUIRED[1..] {
            if self.field_text(*field).trim().is_empty() {
                out.push((*field, Cow::Borrowed("required")));
            }
        }
        out
    }

    /// Errors of the fields on the current step.
    pub fn step_errors(&self) -> Vec<(FieldId, Cow<'static, str>)> {
        let mut errors = self.validation_errors();
        errors.retain(|(f, _)| f.step() == self.step);
        errors
    }

    /// The error to draw on `field`'s border: only once it is touched.
    pub fn field_error(&self, field: FieldId) -> Option<Cow<'static, str>> {
        if !self.touched.contains(&field) {
            return None;
        }
        self.validation_errors()
            .into_iter()
            .find(|(f, _)| *f == field)
            .map(|(_, e)| e)
    }

    /// Stepper indexes drawn `✗`: a step with an error the user has seen
    /// (touched) or already passed.
    pub fn stepper_errors(&self) -> Vec<usize> {
        let current = self.step.stepper_index();
        let mut out: Vec<usize> = self
            .validation_errors()
            .iter()
            .filter(|(f, _)| f.step().stepper_index() < current || self.touched.contains(f))
            .map(|(f, _)| f.step().stepper_index())
            .collect();
        out.dedup();
        out
    }

    /// Draft checklist rows `(label, state, note)`: each required field, then
    /// the placeholder lint. A field on a step not reached yet is `Pending`,
    /// never red.
    pub fn checklist_rows(&self) -> Vec<(&'static str, CheckState, Option<String>)> {
        let errors = self.validation_errors();
        let current = self.step.stepper_index();
        let mut rows: Vec<_> = REQUIRED
            .iter()
            .map(|field| {
                let err = errors.iter().find(|(f, _)| f == field);
                let state = match err {
                    None => CheckState::Ok,
                    Some(_) if field.step().stepper_index() > current => CheckState::Pending,
                    Some(_) => CheckState::Missing,
                };
                let note = err
                    .filter(|_| state == CheckState::Missing)
                    .map(|(_, e)| e.to_string());
                (field.label(), state, note)
            })
            .collect();
        let left: usize = FieldId::ALL
            .iter()
            .map(|f| lint::count(&self.field_text(*f)))
            .sum();
        rows.push(if left == 0 {
            (NO_PLACEHOLDERS, CheckState::Ok, None)
        } else {
            (
                NO_PLACEHOLDERS,
                CheckState::Missing,
                Some(format!("{left} left")),
            )
        });
        rows
    }
}
