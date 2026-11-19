//! #1266: the exit summary says how many agents were paused and how to
//! bring them back.

use super::exit_summary_text;
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::tui::make_test_app;

#[test]
fn exit_summary_paused() {
    // Hermetic icons: the status glyph is in the snapshot.
    crate::tui::icons::init_ignoring_env(false);
    let mut app = make_test_app("exit-summary-1266");
    let session = Session::new(
        "probe".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(1266),
        None,
    );
    let id = session.id;
    app.pool.enqueue(session);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(id).expect("promoted");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
        (SessionStatus::Paused, TransitionReason::UserQuit),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    managed.session.started_at = None;
    managed.session.cost_usd = 0.42;
    // A status that cannot pause is killed at quit and named.
    let ci_fix = Session::new(
        "ci".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(1267),
        None,
    );
    let ci_id = ci_fix.id;
    app.pool.enqueue(ci_fix);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(ci_id).expect("promoted");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
        (SessionStatus::CiFix, TransitionReason::CiFixStarted),
        (SessionStatus::Killed, TransitionReason::UserQuit),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    managed.session.started_at = None;

    let text = exit_summary_text(&app);

    assert!(
        text.contains("1 paused · resume with maestro resume"),
        "the exit summary must name the paused count and the resume hint:\n{text}"
    );
    assert!(
        text.contains("killed at quit (cannot pause): #1267"),
        "the exit summary must name what quit killed:\n{text}"
    );
    insta::assert_snapshot!("exit_summary_paused", text);
}
