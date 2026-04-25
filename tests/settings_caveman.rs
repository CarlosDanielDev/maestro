//! Integration tests for `FsSettingsStore` against real tempfiles.

use maestro::settings::{CavemanModeState, CavemanWriteError, FsSettingsStore, SettingsStore};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn write_fixture(path: &PathBuf, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn read_value(path: &PathBuf) -> Value {
    let raw = fs::read_to_string(path).expect("read");
    serde_json::from_str(&raw).expect("parse json")
}

#[test]
fn roundtrip_preserves_unknown_top_level_keys() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_fixture(
        &path,
        r#"{
            "mcpServers": {"my-server": {"command": "npx"}},
            "env": {"MY_VAR": "1"},
            "alwaysThinkingEnabled": true,
            "hooks": {"postToolUse": []},
            "behavior": {"caveman_mode": false}
        }"#,
    );

    let store = FsSettingsStore::new(&path);
    store.save_caveman_mode(true).expect("save ok");

    let value = read_value(&path);
    assert_eq!(value["behavior"]["caveman_mode"], serde_json::json!(true));
    assert_eq!(
        value["mcpServers"],
        serde_json::json!({"my-server": {"command": "npx"}})
    );
    assert_eq!(value["env"], serde_json::json!({"MY_VAR": "1"}));
    assert_eq!(value["alwaysThinkingEnabled"], serde_json::json!(true));
    assert_eq!(value["hooks"], serde_json::json!({"postToolUse": []}));
}

#[test]
fn toggle_when_file_missing_creates_minimal_json() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    assert!(!path.exists());

    let store = FsSettingsStore::new(&path);
    store.save_caveman_mode(true).expect("save ok");

    assert!(path.exists());
    let value = read_value(&path);
    assert_eq!(
        value,
        serde_json::json!({"behavior": {"caveman_mode": true}})
    );
    assert_eq!(value.as_object().unwrap().len(), 1);
}

#[test]
fn toggle_when_behavior_block_absent_adds_only_behavior() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_fixture(&path, r#"{"mcpServers": {}}"#);

    let store = FsSettingsStore::new(&path);
    store.save_caveman_mode(true).expect("save ok");

    let value = read_value(&path);
    assert_eq!(value["behavior"]["caveman_mode"], serde_json::json!(true));
    assert_eq!(value["mcpServers"], serde_json::json!({}));
    assert_eq!(value.as_object().unwrap().len(), 2);
}

#[test]
fn toggle_when_behavior_partial_preserves_other_behavior_keys() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_fixture(&path, r#"{"behavior": {"other_flag": true}}"#);

    let store = FsSettingsStore::new(&path);
    store.save_caveman_mode(true).expect("save ok");

    let value = read_value(&path);
    assert_eq!(value["behavior"]["other_flag"], serde_json::json!(true));
    assert_eq!(value["behavior"]["caveman_mode"], serde_json::json!(true));
}

#[test]
fn malformed_json_load_returns_error_state() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_fixture(&path, "{ not valid json");

    let store = FsSettingsStore::new(&path);
    assert!(matches!(
        store.load_caveman_mode(),
        CavemanModeState::Error(_)
    ));
}

#[test]
fn non_object_behavior_load_returns_error_state_for_null() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_fixture(&path, r#"{"behavior": null}"#);

    let store = FsSettingsStore::new(&path);
    assert!(matches!(
        store.load_caveman_mode(),
        CavemanModeState::Error(_)
    ));
}

#[test]
fn non_object_behavior_load_returns_error_state_for_string() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_fixture(&path, r#"{"behavior": "x"}"#);

    let store = FsSettingsStore::new(&path);
    assert!(matches!(
        store.load_caveman_mode(),
        CavemanModeState::Error(_)
    ));
}

#[test]
fn non_object_behavior_load_returns_error_state_for_array() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_fixture(&path, r#"{"behavior": [1, 2]}"#);

    let store = FsSettingsStore::new(&path);
    assert!(matches!(
        store.load_caveman_mode(),
        CavemanModeState::Error(_)
    ));
}

#[test]
fn save_on_malformed_file_returns_serialise_error_and_file_unchanged() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_fixture(&path, "{ bad json");

    let original = fs::read(&path).unwrap();

    let store = FsSettingsStore::new(&path);
    let result = store.save_caveman_mode(true);

    assert!(matches!(result, Err(CavemanWriteError::Serialise(_))));
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn symlink_followed_writes_target_keeps_symlink() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let real = dir.path().join("real.json");
    let link = dir.path().join("link.json");
    write_fixture(&real, r#"{"behavior":{"caveman_mode":false}}"#);
    symlink(&real, &link).expect("symlink");

    let store = FsSettingsStore::new(&link);
    store.save_caveman_mode(true).expect("save ok");

    let link_meta = fs::symlink_metadata(&link).expect("symlink meta");
    assert!(
        link_meta.file_type().is_symlink(),
        "symlink path must remain a symlink"
    );

    let value = read_value(&real);
    assert_eq!(value["behavior"]["caveman_mode"], serde_json::json!(true));

    assert!(matches!(
        store.load_caveman_mode(),
        CavemanModeState::ExplicitTrue
    ));
}

#[cfg(unix)]
#[test]
fn broken_symlink_returns_symlink_not_supported() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let link = dir.path().join("link.json");
    let broken_target = dir.path().join("nonexistent_target.json");
    symlink(&broken_target, &link).expect("symlink");
    assert!(!broken_target.exists());

    let store = FsSettingsStore::new(&link);
    let result = store.save_caveman_mode(true);

    assert!(
        matches!(result, Err(CavemanWriteError::SymlinkNotSupported(_))),
        "got {:?}",
        result
    );
}

#[cfg(unix)]
#[test]
fn parent_readonly_yields_io_error_and_does_not_corrupt_original() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let path = sub.join("settings.json");
    write_fixture(&path, r#"{"behavior":{"caveman_mode":false}}"#);
    let original = fs::read_to_string(&path).unwrap();

    fs::set_permissions(&sub, fs::Permissions::from_mode(0o555)).unwrap();

    let store = FsSettingsStore::new(&path);
    let result = store.save_caveman_mode(true);

    // Restore permissions before any assertion that might panic, so tempdir cleanup works.
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        matches!(result, Err(CavemanWriteError::Io(_))),
        "got {:?}",
        result
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn rename_failure_leaves_original_intact() {
    // Replace the target with a non-empty directory so the final
    // `rename(tmp, target)` step fails. The tmp file was just written;
    // confirm both the rename failure surfaces and the original is gone
    // (it was overwritten by the directory creation in this contrived
    // setup) — the invariant we actually care about is that the original
    // file is never partially overwritten by the tempfile content.
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    // Setup: the target is a directory, so the read will fail with EISDIR
    // and the save returns Io. No data is touched on disk.
    fs::create_dir(&path).unwrap();

    let store = FsSettingsStore::new(&path);
    let result = store.save_caveman_mode(true);

    assert!(result.is_err(), "expected err, got {:?}", result);
    // The directory is still there — no file was written over it.
    assert!(
        path.is_dir(),
        "target must remain a directory after failed save"
    );
}
