use super::*;
use crate::state::types::{
    CURRENT_STATE_VERSION, IssueRunState, MaestroState, TeamRun, default_state_version,
};
use std::sync::Arc;

fn must<T, E: std::fmt::Display>(result: std::result::Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(e) => panic!("{context}: {e}"),
    }
}

fn make_store() -> (tempfile::TempDir, StateStore) {
    let dir = must(tempfile::tempdir(), "tempdir should be created");
    let path = dir.path().join("test-state.json");
    (dir, StateStore::new(path))
}

#[test]
fn load_returns_default_when_no_file() {
    let (_dir, store) = make_store();
    let state = must(store.load(), "missing state should load default");
    assert!(state.sessions.is_empty());
    assert_eq!(state.total_cost_usd, 0.0);
}

#[test]
fn save_then_load_round_trips() {
    let (_dir, store) = make_store();
    let state = MaestroState {
        total_cost_usd: 42.5,
        ..Default::default()
    };
    must(store.save(&state), "state should save");
    let loaded = must(store.load(), "state should load");
    assert_eq!(loaded.total_cost_usd, 42.5);
}

#[test]
fn load_corrupt_json_returns_error() {
    let (_dir, store) = make_store();
    must(
        std::fs::write(&store.path, b"{not valid json"),
        "corrupt state should be written",
    );

    let err = match store.load() {
        Ok(_) => panic!("corrupt state should return Err"),
        Err(e) => e,
    };

    assert!(err.to_string().contains("parsing state"));
}

#[test]
fn compact_none_then_save_is_lossless() {
    let (_dir, store) = make_store();
    let mut state = MaestroState::default();
    let mut s = crate::session::types::Session::new(
        "p".into(),
        "opus".into(),
        "orchestrator".into(),
        None,
        None,
    );
    for _ in 0..5 {
        s.activity_log.push(crate::session::types::ActivityEntry {
            timestamp: chrono::Utc::now(),
            message: "Tool: Bash".into(),
        });
    }
    state.sessions.push(s);
    let reports = state.compact(None);
    must(store.save(&state), "state should save");
    assert!(reports.is_empty());
    let loaded = must(store.load(), "state should load");
    assert_eq!(loaded.sessions[0].activity_log.len(), 5);
}

#[test]
fn compact_with_adapter_then_save_persists_collapsed_log() {
    use crate::turboquant::adapter::TurboQuantAdapter;

    let (_dir, store) = make_store();
    let mut state = MaestroState::default();
    let mut s = crate::session::types::Session::new(
        "p".into(),
        "opus".into(),
        "orchestrator".into(),
        None,
        None,
    );
    s.status = crate::session::types::SessionStatus::Running;
    for _ in 0..12 {
        s.activity_log.push(crate::session::types::ActivityEntry {
            timestamp: chrono::Utc::now(),
            message: "Tool: Bash".into(),
        });
    }
    state.sessions.push(s);

    let adapter = TurboQuantAdapter::new(4);
    let reports = state.compact(Some(&adapter));
    must(store.save(&state), "state should save");
    assert_eq!(reports.len(), 1);
    let loaded = must(store.load(), "state should load");
    assert_eq!(loaded.sessions[0].activity_log.len(), 1);
    assert!(loaded.sessions[0].activity_log[0].message.contains("x12"));
}

#[test]
fn load_legacy_state_without_handoff_fields_succeeds() {
    let (_dir, store) = make_store();
    let legacy_json = include_str!("../../tests/fixtures/state/v0.json");
    must(
        std::fs::write(&store.path, legacy_json),
        "legacy state should be written",
    );
    let loaded = must(store.load(), "legacy state should load");
    assert_eq!(loaded.sessions.len(), 1);
    assert!(loaded.sessions[0].tq_handoff_original_tokens.is_none());
    assert!(loaded.sessions[0].tq_handoff_compressed_tokens.is_none());
}

#[test]
fn concurrent_saves_do_not_corrupt() {
    let dir = must(tempfile::tempdir(), "tempdir should be created");
    let path = dir.path().join("concurrent-state.json");
    let store = Arc::new(StateStore::new(path));

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let store = Arc::clone(&store);
            std::thread::spawn(move || {
                let state = MaestroState {
                    total_cost_usd: i as f64,
                    ..Default::default()
                };
                must(store.save(&state), "concurrent save should succeed");
            })
        })
        .collect();

    for h in handles {
        if h.join().is_err() {
            panic!("save thread should not panic");
        }
    }

    let loaded = must(store.load(), "state should load after concurrent saves");
    assert!(loaded.total_cost_usd >= 0.0);
}

#[test]
fn reconcile_promotes_inflight_to_failed_on_load() {
    let (_dir, store) = make_store();
    let mut state = MaestroState::default();

    let mut run_state = std::collections::HashMap::new();
    run_state.insert(
        1u64,
        IssueRunState::InFlight {
            session_id: uuid::Uuid::new_v4(),
            started_at: chrono::Utc::now(),
        },
    );
    run_state.insert(
        2u64,
        IssueRunState::Succeeded {
            output: crate::orchestration::types::TeamOutput::Pr {
                number: 1,
                branch: "x".into(),
            },
        },
    );

    state.team_runs.push(TeamRun {
        id: uuid::Uuid::new_v4(),
        team_name: "t".into(),
        started_at: chrono::Utc::now(),
        plan: vec![vec![1, 2]],
        state: run_state,
    });

    must(store.save(&state), "state with inflight run should save");
    let loaded = must(store.load(), "state should load with reconciliation");
    let run = &loaded.team_runs[0];
    assert!(matches!(
        run.state.get(&1),
        Some(IssueRunState::Failed { reason, attempts }) if reason.contains("process state lost") && *attempts == 0
    ));
    assert!(matches!(
        run.state.get(&2),
        Some(IssueRunState::Succeeded { .. })
    ));
}

// --- Issue #665: state-store version stamp + v0 migration ---

#[test]
fn default_state_version_constant_is_zero() {
    // Defensive: any change to the legacy serde default would silently
    // re-flag old state files as already-current, skipping migration.
    assert_eq!(default_state_version(), 0);
}

#[test]
fn maestro_state_default_has_current_version() {
    let state = MaestroState::default();
    assert_eq!(state.version, CURRENT_STATE_VERSION);
}

#[test]
fn legacy_state_without_version_key_deserializes_with_zero() {
    let json = r#"{"sessions":[],"total_cost_usd":0.0,"file_claims":{},"last_updated":null}"#;
    let state: MaestroState = serde_json::from_str(json).unwrap();
    assert_eq!(
        state.version, 0,
        "legacy file without version key must deserialize to 0 — migration is the bumper"
    );
}

#[test]
fn migrate_bumps_version_zero_to_current() {
    let mut state = MaestroState {
        version: 0,
        ..Default::default()
    };
    migrate(&mut state).unwrap();
    assert_eq!(state.version, CURRENT_STATE_VERSION);
}

#[test]
fn migrate_is_idempotent_on_current_version() {
    let mut state = MaestroState::default();
    let before = state.version;
    migrate(&mut state).unwrap();
    migrate(&mut state).unwrap();
    assert_eq!(state.version, before);
}

#[test]
fn migrate_rejects_state_from_newer_version() {
    let mut state = MaestroState {
        version: CURRENT_STATE_VERSION + 1,
        ..Default::default()
    };
    let err = migrate(&mut state).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("newer maestro version"));
    assert!(msg.contains(&format!("v{}", CURRENT_STATE_VERSION + 1)));
}

#[test]
fn store_load_rejects_state_from_newer_version() {
    let (_dir, store) = make_store();
    let future_json = format!(
        r#"{{"version":{},"sessions":[],"total_cost_usd":0.0,"file_claims":{{}},"last_updated":null}}"#,
        CURRENT_STATE_VERSION + 99
    );
    std::fs::write(&store.path, future_json).unwrap();
    let err = store.load().unwrap_err();
    assert!(format!("{err:#}").contains("newer maestro version"));
}

#[test]
fn store_load_migrates_legacy_state_to_current_version() {
    let (_dir, store) = make_store();
    let legacy_json = r#"{
        "sessions": [],
        "total_cost_usd": 0.0,
        "file_claims": {},
        "last_updated": null
    }"#;
    must(
        std::fs::write(&store.path, legacy_json),
        "legacy state should be written",
    );
    let loaded = must(store.load(), "legacy state should load");
    assert_eq!(loaded.version, CURRENT_STATE_VERSION);
}

#[test]
fn store_load_v0_fixture_round_trips_with_version_bump() {
    let (_dir, store) = make_store();
    let v0_json = include_str!("../../tests/fixtures/state/v0.json");
    must(
        std::fs::write(&store.path, v0_json),
        "v0 fixture should be written",
    );
    let loaded = must(store.load(), "v0 fixture should load");
    assert_eq!(loaded.version, CURRENT_STATE_VERSION);
    assert_eq!(loaded.sessions.len(), 1);
    assert_eq!(loaded.team_runs.len(), 0);

    must(store.save(&loaded), "migrated state should save");
    let reloaded = must(store.load(), "migrated state should reload");
    assert_eq!(reloaded.version, CURRENT_STATE_VERSION);
    let serialized = std::fs::read_to_string(&store.path).unwrap();
    assert!(
        serialized.contains("\"version\""),
        "saved state must include the version key"
    );
}
