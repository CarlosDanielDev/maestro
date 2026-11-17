use std::time::{Duration, Instant};

use super::*;

fn t0() -> Instant {
    Instant::now()
}

#[test]
fn info_expires_after_three_seconds() {
    let now = t0();
    let mut q: ToastQueue<()> = ToastQueue::default();
    q.push(Toast::new(Severity::Info, "hi"), now);
    assert!(q.visible(now + Duration::from_millis(2_999)).is_some());
    assert!(q.visible(now + Duration::from_secs(3)).is_none());
}

#[test]
fn warn_expires_after_six_seconds() {
    let now = t0();
    let mut q: ToastQueue<()> = ToastQueue::default();
    q.push(Toast::new(Severity::Warn, "hi"), now);
    assert!(q.visible(now + Duration::from_millis(5_999)).is_some());
    assert!(q.visible(now + Duration::from_secs(6)).is_none());
}

#[test]
fn error_is_sticky_until_dismissed() {
    let now = t0();
    let mut q: ToastQueue<()> = ToastQueue::default();
    q.push(Toast::new(Severity::Error, "boom"), now);
    let later = now + Duration::from_secs(3_600);
    assert!(q.visible(later).is_some());
    assert!(q.dismiss_error(later));
    assert!(q.visible(later).is_none());
    assert!(!q.dismiss_error(later), "nothing left to dismiss");
}

#[test]
fn newer_replaces_older_and_errors_queue() {
    let now = t0();
    let mut q: ToastQueue<()> = ToastQueue::default();
    q.push(Toast::new(Severity::Info, "first"), now);
    q.push(Toast::new(Severity::Warn, "second"), now);
    assert_eq!(q.visible(now).map(|v| v.message), Some("second"));

    q.push(Toast::new(Severity::Error, "e1"), now);
    q.push(Toast::new(Severity::Error, "e2"), now);
    // A live transient toast is newer news; errors wait behind it.
    assert_eq!(q.visible(now).map(|v| v.message), Some("second"));
    let after = now + Duration::from_secs(7);
    assert_eq!(q.visible(after).map(|v| v.message), Some("e1"));
    assert!(q.dismiss_error(after));
    assert_eq!(q.visible(after).map(|v| v.message), Some("e2"));
}

#[test]
fn dismiss_error_ignores_covering_transient() {
    let now = t0();
    let mut q: ToastQueue<()> = ToastQueue::default();
    q.push(Toast::new(Severity::Error, "e1"), now);
    q.push(Toast::new(Severity::Info, "covers"), now);
    assert!(
        !q.dismiss_error(now),
        "Esc must not drop an error nobody sees"
    );
}

#[test]
fn undo_is_taken_once_inside_window() {
    let now = t0();
    let mut q = ToastQueue::default();
    q.push(Toast::undoable("Paused #407", 7_u8), now);
    let view = q.visible(now + Duration::from_secs(1)).expect("visible");
    assert_eq!(view.undo_left, Some(UNDO_WINDOW - Duration::from_secs(1)));
    assert_eq!(q.take_undo(now + Duration::from_secs(1)), Some(7));
    assert_eq!(q.take_undo(now + Duration::from_secs(1)), None);
    assert!(q.visible(now).is_none(), "undo closes the toast");
}

#[test]
fn undo_after_window_is_none() {
    let now = t0();
    let mut q = ToastQueue::default();
    q.push(Toast::undoable("Paused #407", 7_u8), now);
    assert_eq!(q.take_undo(now + UNDO_WINDOW), None);
}

#[test]
fn undo_is_none_without_undo_toast() {
    let now = t0();
    let mut q: ToastQueue<u8> = ToastQueue::default();
    q.push(Toast::new(Severity::Info, "plain"), now);
    assert_eq!(q.take_undo(now), None);
    assert!(q.visible(now).is_some(), "plain toast stays");
}
