//! #1153 test helper: the completion pipeline now dispatches its heavy
//! work (gates, git push, GitHub calls) to a background job and applies
//! outcomes on a later tick. Tests that assert post-completion state
//! drive one dispatch tick, wait for the job's outcome, then drive the
//! apply tick — mirroring what the event loop does across two draws.

use crate::tui::app::App;
use std::time::Duration;

/// Run `check_completions` until a pending completion has been fully
/// applied: one tick to dispatch the job, then ticks every 20 ms until
/// no job is left in flight (its outcome was applied). Bounded at 10 s.
pub(crate) async fn drive_completion_to_apply(app: &mut App) {
    app.check_completions()
        .await
        .expect("dispatch tick must not error");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !app.completions_in_flight.is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "completion job was not applied within 10s"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
        app.check_completions()
            .await
            .expect("apply tick must not error");
    }
}
