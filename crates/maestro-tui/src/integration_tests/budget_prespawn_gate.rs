//! Pre-spawn budget gate integration tests (#776/#850).
//!
//! Verifies the gate fires in `App::add_session` when the projection
//! crosses `alert_threshold_pct` (Warn) or `total_usd` (Block), and that
//! the session lands in `TuiMode::BudgetPreSpawn { session_id, .. }`
//! without transitioning to Spawning/Running.

use crate::budget::BudgetEnforcer;
use crate::budget::projector::BudgetProjector;
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::types::TuiMode;

struct FakeBudgetProjector {
    projected: f64,
}

impl FakeBudgetProjector {
    fn returning(projected: f64) -> Self {
        Self { projected }
    }
}

impl BudgetProjector for FakeBudgetProjector {
    fn projected_turn_cost(&self, _session: &Session) -> f64 {
        self.projected
    }
}

fn make_test_session() -> Session {
    let mut s = Session::new(
        "test".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(848),
        None,
    );
    s.agent_id = Some("claude".to_string());
    s
}

#[tokio::test]
async fn pre_spawn_gate_blocks_when_projected_exceeds_limit() {
    let mut app = crate::tui::make_test_app("budget-gate-block")
        .with_budget_projector(Box::new(FakeBudgetProjector::returning(8.0)));
    app.budget_enforcer = Some(BudgetEnforcer::new(5.0, 10.0, 80));
    app.total_cost = 5.0; // 5 + 8 = 13 > 10

    let session = make_test_session();
    app.add_session(session).await.unwrap();

    assert!(
        matches!(app.tui_mode, TuiMode::BudgetPreSpawn { .. }),
        "tui_mode must transition to BudgetPreSpawn on Block; got {:?}",
        app.tui_mode
    );
    assert!(
        app.pool
            .all_sessions()
            .iter()
            .all(|s| !matches!(s.status, SessionStatus::Running)),
        "no session must be Running after gate Blocks"
    );
}

#[tokio::test]
async fn pre_spawn_gate_warns_when_projected_crosses_threshold() {
    let mut app = crate::tui::make_test_app("budget-gate-warn")
        .with_budget_projector(Box::new(FakeBudgetProjector::returning(1.5)));
    app.budget_enforcer = Some(BudgetEnforcer::new(5.0, 10.0, 80));
    app.total_cost = 7.0; // 7 + 1.5 = 8.5; 85% > 80%

    let session = make_test_session();
    app.add_session(session).await.unwrap();

    assert!(
        matches!(app.tui_mode, TuiMode::BudgetPreSpawn { .. }),
        "tui_mode must transition to BudgetPreSpawn on Warn; got {:?}",
        app.tui_mode
    );
}

#[tokio::test]
async fn pre_spawn_gate_allows_when_below_threshold() {
    let mut app = crate::tui::make_test_app("budget-gate-allow")
        .with_budget_projector(Box::new(FakeBudgetProjector::returning(0.5)));
    app.budget_enforcer = Some(BudgetEnforcer::new(5.0, 10.0, 80));
    app.total_cost = 1.0; // 1 + 0.5 = 1.5; 15% < 80%

    let session = make_test_session();
    app.add_session(session).await.unwrap();

    assert!(
        !matches!(app.tui_mode, TuiMode::BudgetPreSpawn { .. }),
        "tui_mode must NOT transition to BudgetPreSpawn when under threshold; got {:?}",
        app.tui_mode
    );
}

#[tokio::test]
async fn pre_spawn_gate_no_enforcer_allows_always() {
    let mut app = crate::tui::make_test_app("budget-gate-no-enforcer")
        .with_budget_projector(Box::new(FakeBudgetProjector::returning(999.0)));
    // budget_enforcer left at None — gate must be Allow despite projected > any limit.
    assert!(app.budget_enforcer.is_none());

    let session = make_test_session();
    app.add_session(session).await.unwrap();

    assert!(
        !matches!(app.tui_mode, TuiMode::BudgetPreSpawn { .. }),
        "tui_mode must NOT transition to BudgetPreSpawn when enforcer is None"
    );
}

#[tokio::test]
async fn pre_spawn_gate_stores_session_id_in_mode() {
    let mut app = crate::tui::make_test_app("budget-gate-session-id")
        .with_budget_projector(Box::new(FakeBudgetProjector::returning(99.0)));
    app.budget_enforcer = Some(BudgetEnforcer::new(5.0, 10.0, 80));

    let session = make_test_session();
    let session_id = session.id;
    app.add_session(session).await.unwrap();

    match app.tui_mode {
        TuiMode::BudgetPreSpawn { session_id: stored } => {
            assert_eq!(
                stored, session_id,
                "BudgetPreSpawn must carry the parked session's id"
            );
        }
        other => panic!(
            "expected TuiMode::BudgetPreSpawn after gate fires; got {:?}",
            other
        ),
    }
}
