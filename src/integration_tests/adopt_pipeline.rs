//! Adoption pipeline integration tests (#1053): an Interactive session
//! created from an external Claude Code conversation resumes that
//! conversation from its first turn, in the transcript's original cwd.
//!
//! In-process only per the integration-test charter: the provider is the
//! scripted in-process fake — no PTY child, no transcript IO. The
//! `HOME`-rooted transcript stat that backs the live-session guard is
//! unit-covered in `src/session/adopt.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
use crate::session::manager::SessionEvent;
use crate::session::pool::SessionPool;
use crate::session::worktree::MockWorktreeManager;

const EXTERNAL_ID: &str = "ext-conv-1";
const ORIGINAL_CWD: &str = env!("CARGO_MANIFEST_DIR");

fn make_pool() -> (SessionPool, Arc<ScriptedProvider>) {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut pool = SessionPool::new(4, Box::new(MockWorktreeManager::new()), tx);
    let provider = Arc::new(ScriptedProvider::new(vec![ScriptedTurn {
        events: vec![],
        end: ScriptedEnd::Ok {
            exit_code: Some(0),
            session_id: Some(EXTERNAL_ID),
        },
    }]));
    pool.set_provider(provider.clone());
    (pool, provider)
}

fn adopt(pool: &mut SessionPool) -> crate::session::adopt::AdoptOutcome {
    pool.create_adopted_session(
        EXTERNAL_ID,
        PathBuf::from(ORIGINAL_CWD),
        Some("Adopted conversation".to_string()),
        "opus".to_string(),
    )
    .expect("valid external id is adoptable")
}

/// #1053: the first turn of an adopted session must resume the external
/// conversation — `resume_session_id = <external id>` and
/// `cwd = <original cwd>` on the very first provider request.
#[tokio::test]
async fn first_turn_request_resumes_external_conversation_in_original_cwd() {
    let (mut pool, provider) = make_pool();
    let outcome = adopt(&mut pool);

    let (event_tx, _event_rx) = mpsc::unbounded_channel::<SessionEvent>();
    {
        let managed = pool
            .get_active_mut(outcome.session_id)
            .expect("adopted session is active");
        managed.spawn(event_tx).await.expect("spawn starts");
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    while provider.requests.lock().expect("requests lock").is_empty() {
        assert!(
            Instant::now() < deadline,
            "provider never received the first-turn request"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let request = &provider.requests.lock().expect("requests lock")[0];
    assert_eq!(
        request.resume_session_id.as_deref(),
        Some(EXTERNAL_ID),
        "turn 1 must carry the external conversation id"
    );
    assert_eq!(
        request.cwd.as_deref(),
        Some(Path::new(ORIGINAL_CWD)),
        "turn 1 must run in the transcript's original cwd"
    );
}

/// #1053: maestro's own state records the adoption — the persisted
/// `maestro-state.json` session carries the bound `agent_session_id` and
/// the original cwd, so `maestro logs` and the session switcher see it.
/// Mirrors the `App::sync_state` core: pool sessions → state → save.
#[test]
fn adopted_session_persists_to_state_store_with_id_and_cwd() {
    let (mut pool, _provider) = make_pool();
    let outcome = adopt(&mut pool);

    let dir = tempfile::tempdir().expect("tempdir");
    let store = crate::state::store::StateStore::new(dir.path().join("maestro-state.json"));

    let mut state = crate::state::types::MaestroState::default();
    state.sessions = pool.all_sessions().into_iter().cloned().collect();
    store.save(&state).expect("state saves");

    let loaded = store.load().expect("state loads back");
    let persisted = loaded
        .sessions
        .iter()
        .find(|s| s.id == outcome.session_id)
        .expect("adopted session must be in the persisted state");
    assert_eq!(persisted.agent_session_id.as_deref(), Some(EXTERNAL_ID));
    assert_eq!(
        persisted.adopted_cwd.as_deref(),
        Some(Path::new(ORIGINAL_CWD))
    );
}
