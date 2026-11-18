//! #1246: a wave starts only when every session of the previous wave is
//! terminal. Child of `team_runner_tests` so it reuses `FakeLauncher`.

use super::*;

/// Await the runner, failing (not hanging) when it never finishes.
async fn finish(run: tokio::task::JoinHandle<TeamOutcome>) -> TeamOutcome {
    tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("run_team hung: a session it waits on never finished")
        .expect("run_team task")
}

/// Let the runner and its spawned tasks run until they block again.
async fn settle() {
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn run_team_waits_for_level_completion() {
    let (tx, rx) = watch::channel(StatusFeed::new());
    let launcher = Arc::new(FakeLauncher::new().with_statuses(rx));
    let run = tokio::spawn(run_team(
        Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
        two_level_scheduler(3),
        "app-default".to_string(),
    ));
    settle().await;
    assert_eq!(
        launcher.spawned(),
        vec![10],
        "wave 2 spawned before wave 1 ran"
    );

    // Running, paused and stalled sessions all hold wave 2 back.
    for status in [
        SessionStatus::Running,
        SessionStatus::Paused,
        SessionStatus::Stalled,
    ] {
        tx.send_modify(|feed| {
            feed.insert(10, (status, false));
        });
        settle().await;
        assert_eq!(
            launcher.spawned(),
            vec![10],
            "wave 2 spawned while #10 was {status:?}"
        );
    }

    tx.send_modify(|feed| {
        feed.insert(10, (SessionStatus::Completed, false));
    });
    settle().await;
    let mut spawned = launcher.spawned();
    spawned.sort_unstable();
    assert_eq!(spawned, vec![10, 11, 12]);
    assert_eq!(*launcher.waves.lock().unwrap(), vec![0, 1]);
    assert!(!run.is_finished(), "run ended before wave 2 finished");

    tx.send_modify(|feed| {
        feed.insert(11, (SessionStatus::Completed, false));
        feed.insert(12, (SessionStatus::NeedsReview, false));
    });
    let outcome = finish(run).await;
    assert_eq!(outcome.succeeded.len(), 3);
    assert!(outcome.failed.is_empty());
    assert!(outcome.into_apply_result().is_ok());
}

#[tokio::test]
async fn run_team_failed_level_skips_downstream() {
    for (status, hollow, why) in [
        (SessionStatus::Completed, true, "hollow"),
        (SessionStatus::Killed, false, "killed"),
        (SessionStatus::FailedGates, false, "gates"),
        (SessionStatus::Errored, false, "errored"),
    ] {
        let (tx, rx) = watch::channel(StatusFeed::new());
        let launcher = Arc::new(FakeLauncher::new().with_statuses(rx));
        let run = tokio::spawn(run_team(
            Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
            two_level_scheduler(3),
            "app-default".to_string(),
        ));
        settle().await;
        tx.send_modify(|feed| {
            feed.insert(10, (status, hollow));
        });
        let outcome = finish(run).await;

        assert_eq!(launcher.spawned(), vec![10], "{status:?}: wave 2 spawned");
        assert!(
            outcome.succeeded.is_empty(),
            "{status:?} counted as success"
        );
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].issue, 10);
        assert!(
            outcome.failed[0].reason.contains(why),
            "{status:?}: reason {:?} should name {why}",
            outcome.failed[0].reason
        );
        assert_eq!(outcome.skipped_due_to_upstream, vec![11, 12]);
        let reason = outcome.skip_reason.clone().expect("skip reason");
        assert!(
            reason.starts_with("wave 1 failed: #10"),
            "skip reason {reason:?}"
        );
        // The banner carries the wave reason, not only the issue (#1246).
        let banner = outcome.into_apply_result().expect_err("failed wave");
        assert_eq!(banner, format!("{reason}; 2 downstream issue(s) skipped"));
    }
}

#[test]
fn team_verdict_waits_on_live_states_and_fails_hollow() {
    use SessionStatus::*;
    for live in [
        Queued,
        Spawning,
        Running,
        Paused,
        Stalled,
        Retrying,
        GatesRunning,
        NeedsPr,
    ] {
        assert_eq!(
            team_verdict(live, false),
            None,
            "{live:?} must keep waiting"
        );
    }
    assert_eq!(team_verdict(Completed, false), Some(Ok(())));
    assert_eq!(team_verdict(NeedsReview, false), Some(Ok(())));
    assert!(matches!(team_verdict(Completed, true), Some(Err(_))));
    for failed in [Killed, FailedGates, Errored] {
        assert!(
            matches!(team_verdict(failed, false), Some(Err(_))),
            "{failed:?}"
        );
    }
}

#[tokio::test]
async fn run_team_cancel_starts_no_later_wave() {
    let (_tx, rx) = watch::channel(StatusFeed::new());
    let launcher = Arc::new(FakeLauncher::new().with_statuses(rx));
    let cancel = CancellationToken::new();
    let run = tokio::spawn(run_team_until(
        Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
        two_level_scheduler(3),
        "app-default".to_string(),
        cancel.clone(),
    ));
    settle().await;
    assert_eq!(launcher.spawned(), vec![10]);

    // #10 never finishes (a Paused or stuck NeedsPr session); the user
    // cancels instead.
    cancel.cancel();
    let outcome = finish(run).await;

    assert_eq!(launcher.spawned(), vec![10], "wave 2 spawned after cancel");
    assert_eq!(*launcher.waves.lock().unwrap(), vec![0]);
    assert!(outcome.succeeded.is_empty());
    assert!(
        outcome.failed.is_empty(),
        "a cancel is not a session failure"
    );
    assert_eq!(outcome.skipped_due_to_upstream, vec![11, 12]);
    assert_eq!(
        outcome.into_apply_result(),
        Err("cancelled by user while waiting on wave 1; 2 downstream issue(s) skipped".into())
    );
}

#[tokio::test]
async fn run_team_level_larger_than_max_parallel_runs_in_batches() {
    let (tx, rx) = watch::channel(StatusFeed::new());
    let launcher = Arc::new(FakeLauncher::new().with_statuses(rx));
    let run = tokio::spawn(run_team(
        Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
        two_level_scheduler(1),
        "app-default".to_string(),
    ));
    settle().await;
    tx.send_modify(|feed| {
        feed.insert(10, (SessionStatus::Completed, false));
    });
    settle().await;
    let spawned = launcher.spawned();
    assert_eq!(
        spawned.len(),
        2,
        "max_parallel 1: one wave-2 session at a time"
    );

    let first = spawned[1];
    tx.send_modify(|feed| {
        feed.insert(first, (SessionStatus::Completed, false));
    });
    settle().await;
    assert_eq!(launcher.spawned().len(), 3);
    let second = launcher.spawned()[2];
    tx.send_modify(|feed| {
        feed.insert(second, (SessionStatus::Completed, false));
    });
    let outcome = finish(run).await;
    assert_eq!(outcome.succeeded.len(), 3);
}
