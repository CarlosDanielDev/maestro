use crate::github::types::PrReviewEvent;

/// State machine for the PR review screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrReviewStep {
    #[default]
    Loading,
    PrList,
    PrDetail,
    SubmitReview,
    Done,
}

impl PrReviewStep {
    #[allow(dead_code)] // Reason: used by PR review screen rendering
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }
}

/// Form state for composing a PR review submission.
#[derive(Debug, Clone)]
pub struct ReviewForm {
    pub event: PrReviewEvent,
    pub body: String,
}

impl Default for ReviewForm {
    fn default() -> Self {
        Self {
            event: PrReviewEvent::Comment,
            body: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_review_step_default_is_loading() {
        assert_eq!(PrReviewStep::default(), PrReviewStep::Loading);
    }

    #[test]
    fn pr_review_step_is_loading_only_for_loading() {
        assert!(PrReviewStep::Loading.is_loading());
        assert!(!PrReviewStep::PrList.is_loading());
        assert!(!PrReviewStep::PrDetail.is_loading());
        assert!(!PrReviewStep::SubmitReview.is_loading());
        assert!(!PrReviewStep::Done.is_loading());
    }

    #[test]
    fn pr_review_step_copy_semantics() {
        let a = PrReviewStep::PrDetail;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn review_form_default_event_is_comment() {
        let form = ReviewForm::default();
        assert_eq!(form.event, PrReviewEvent::Comment);
    }

    #[test]
    fn review_form_body_default_is_empty() {
        let form = ReviewForm::default();
        assert!(form.body.is_empty());
    }
}
