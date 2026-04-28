use crate::updater::UpgradeState;
use crate::updater::checker::UpdateChecker;
use crate::updater::checker::test_support::{MockCheckerResponse, MockUpdateChecker};
use crate::updater::installer::Installer;
use crate::updater::restart::RestartBuilder;
use tempfile::tempdir;
use tokio::fs;

#[tokio::test]
async fn upgrade_flow_check_to_trigger_full_pipeline() {
    let checker = MockUpdateChecker {
        response: MockCheckerResponse::NewVersion("0.6.0".into()),
    };

    let dir = tempdir().unwrap();
    let dest = dir.path().join("maestro");
    fs::write(&dest, b"old binary v0.5.0").await.unwrap();
    let installer = Installer::new(dest.clone());
    let restart_builder = RestartBuilder::new(dest.clone(), vec!["--once".to_string()]);

    let result = checker.check_for_update().await.unwrap();
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.version, "0.6.0");

    let state = UpgradeState::Available(info);
    assert!(state.is_visible());

    let new_bytes = b"new binary v0.6.0".to_vec();
    installer
        .install_with_backup(new_bytes.clone())
        .await
        .unwrap();
    let installed = fs::read(&dest).await.unwrap();
    assert_eq!(installed, new_bytes);

    let cmd = restart_builder.build_command();
    assert!(cmd.program.to_string_lossy().contains("maestro"));
    assert_eq!(cmd.args, vec!["--once".to_string()]);
}

#[tokio::test]
async fn upgrade_flow_network_error_leaves_state_hidden() {
    let checker = MockUpdateChecker {
        response: MockCheckerResponse::NetworkError,
    };

    let mut state = UpgradeState::Hidden;
    if let Ok(Some(info)) = checker.check_for_update().await {
        state = UpgradeState::Available(info);
    }
    assert_eq!(state, UpgradeState::Hidden);
}

#[tokio::test]
async fn upgrade_flow_same_version_leaves_state_hidden() {
    let checker = MockUpdateChecker {
        response: MockCheckerResponse::SameVersion,
    };

    let mut state = UpgradeState::Hidden;
    if let Ok(Some(info)) = checker.check_for_update().await {
        state = UpgradeState::Available(info);
    }
    assert_eq!(state, UpgradeState::Hidden);
}

#[tokio::test]
async fn upgrade_flow_malformed_version_leaves_state_hidden() {
    let checker = MockUpdateChecker {
        response: MockCheckerResponse::MalformedResponse,
    };

    let mut state = UpgradeState::Hidden;
    if let Ok(Some(info)) = checker.check_for_update().await {
        state = UpgradeState::Available(info);
    }
    assert_eq!(state, UpgradeState::Hidden);
}
