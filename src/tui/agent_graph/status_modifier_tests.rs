//! Composition contract: status is a `Style::add_modifier` overlay on the
//! role color. These tests pin the table from
//! `docs/adr/002-agent-personalities.md` § Status Modifier Composition.

use ratatui::style::Modifier;

use crate::session::types::SessionStatus;

#[test]
fn status_modifier_running_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::Running),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_gates_running_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::GatesRunning),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_needs_review_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::NeedsReview),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_needs_pr_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::NeedsPr),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_ci_fix_is_bold() {
    assert_eq!(super::status_modifier(SessionStatus::CiFix), Modifier::BOLD);
}

#[test]
fn status_modifier_conflict_fix_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::ConflictFix),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_errored_is_dim_bold() {
    // Intentionally NOT Modifier::CROSSED_OUT — crossterm portability concern
    // flagged in the issue test hints. DIM | BOLD reads "agent stopped working"
    // on every terminal.
    assert_eq!(
        super::status_modifier(SessionStatus::Errored),
        Modifier::DIM | Modifier::BOLD
    );
}

#[test]
fn status_modifier_completed_is_dim() {
    assert_eq!(
        super::status_modifier(SessionStatus::Completed),
        Modifier::DIM
    );
}

#[test]
fn status_modifier_killed_is_dim() {
    assert_eq!(super::status_modifier(SessionStatus::Killed), Modifier::DIM);
}

#[test]
fn status_modifier_paused_is_dim() {
    assert_eq!(super::status_modifier(SessionStatus::Paused), Modifier::DIM);
}

#[test]
fn status_modifier_stalled_is_dim_reversed() {
    assert_eq!(
        super::status_modifier(SessionStatus::Stalled),
        Modifier::DIM | Modifier::REVERSED
    );
}

#[test]
fn status_modifier_spawning_is_empty() {
    assert_eq!(
        super::status_modifier(SessionStatus::Spawning),
        Modifier::empty()
    );
}

#[test]
fn status_modifier_queued_is_empty() {
    assert_eq!(
        super::status_modifier(SessionStatus::Queued),
        Modifier::empty()
    );
}

#[test]
fn status_modifier_retrying_is_empty() {
    assert_eq!(
        super::status_modifier(SessionStatus::Retrying),
        Modifier::empty()
    );
}
