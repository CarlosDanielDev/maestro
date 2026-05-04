use crate::tui::app::*;

fn make_app() -> crate::tui::app::App {
    crate::tui::make_test_app("maestro-tui-app-test")
}

#[test]
fn app_queue_confirmation_screen_defaults_to_none() {
    let app = make_app();
    assert!(app.screen_state.queue_confirmation_screen.is_none());
}

// --- Issue #68: QueueExecutor state ---

#[test]
fn app_queue_executor_defaults_to_none() {
    let app = make_app();
    assert!(app.queue_executor.is_none());
}

#[test]
fn tui_mode_queue_execution_variant_exists() {
    let mode = TuiMode::QueueExecution;
    assert!(matches!(mode, TuiMode::QueueExecution));
}
