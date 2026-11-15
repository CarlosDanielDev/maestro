//! Tests for `migrate.rs`. Extracted into a sibling file via `#[path]` so
//! `migrate.rs` itself stays under the repo's 400-line file-size guardrail.

use super::*;

// --- Group A: pure unit tests for the planner --------------------------

#[test]
fn plan_returns_already_current_when_key_present_true() {
    let toml = "[views]\nagent_graph_enabled = true\n";
    let outcome = plan_v0_25_1_migration(toml).unwrap();
    assert!(matches!(outcome, MigrationOutcome::AlreadyCurrent));
}

#[test]
fn plan_returns_already_current_when_key_present_false() {
    let toml = "[views]\nagent_graph_enabled = false\n";
    let outcome = plan_v0_25_1_migration(toml).unwrap();
    assert!(
        matches!(outcome, MigrationOutcome::AlreadyCurrent),
        "opt-out (false) must be preserved without rewrite"
    );
}

#[test]
fn plan_returns_migrated_when_views_section_absent() {
    let toml = "[sessions]\ndefault_model = \"opus\"\n";
    let outcome = plan_v0_25_1_migration(toml).unwrap();
    let MigrationOutcome::Migrated {
        new_toml,
        added_keys,
    } = outcome
    else {
        panic!("expected Migrated, got AlreadyCurrent");
    };
    assert!(
        new_toml.contains("agent_graph_enabled = true"),
        "new_toml must contain key set to true: {new_toml}"
    );
    assert!(
        added_keys.iter().any(|k| k == "views.agent_graph_enabled"),
        "added_keys must list views.agent_graph_enabled: {added_keys:?}"
    );
    assert!(
        toml::from_str::<toml::Value>(&new_toml).is_ok(),
        "new_toml must reparse as valid TOML: {new_toml}"
    );
}

#[test]
fn plan_returns_migrated_when_views_section_present_but_key_absent() {
    let toml = "[views]\nsome_other_key = true\n";
    let outcome = plan_v0_25_1_migration(toml).unwrap();
    let MigrationOutcome::Migrated {
        new_toml,
        added_keys,
    } = outcome
    else {
        panic!("expected Migrated");
    };
    assert!(new_toml.contains("[views]"));
    assert!(new_toml.contains("some_other_key = true"));
    assert!(new_toml.contains("agent_graph_enabled = true"));
    assert!(added_keys.iter().any(|k| k == "views.agent_graph_enabled"));
    assert!(toml::from_str::<toml::Value>(&new_toml).is_ok());
}

#[test]
fn plan_returns_error_on_malformed_toml() {
    let toml = "[[[[not valid toml";
    let result = plan_v0_25_1_migration(toml);
    assert!(
        result.is_err(),
        "malformed TOML must propagate as Err so the driver can skip the write"
    );
}

#[test]
fn plan_migrated_new_toml_is_valid_roundtrip() {
    let toml = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n";
    let outcome = plan_v0_25_1_migration(toml).unwrap();
    let MigrationOutcome::Migrated { new_toml, .. } = outcome else {
        panic!("expected Migrated");
    };
    assert!(toml::from_str::<toml::Value>(&new_toml).is_ok());
    assert!(new_toml.contains("agent_graph_enabled = true"));
    assert!(
        new_toml.contains("\"owner/repo\""),
        "existing content must be preserved: {new_toml}"
    );
}

// --- Group B: filesystem integration via writer injection --------------

#[test]
fn driver_writes_key_and_emits_notice_when_key_absent() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("maestro.toml");
    let initial = "[sessions]\ndefault_model = \"opus\"\n";
    std::fs::write(&path, initial).unwrap();
    let mut buf: Vec<u8> = Vec::new();

    run_startup_migration_with_writer(&path, &mut buf);

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(
        on_disk.contains("agent_graph_enabled = true"),
        "file must have key after migration: {on_disk}"
    );
    let stderr_output = String::from_utf8(buf).unwrap();
    assert!(
        stderr_output.contains("[maestro] config migrated: added views.agent_graph_enabled = true"),
        "stderr must contain migration notice, got: {stderr_output:?}"
    );
}

#[test]
fn driver_is_idempotent_no_write_on_second_run() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("maestro.toml");
    let initial = "[sessions]\ndefault_model = \"opus\"\n";
    std::fs::write(&path, initial).unwrap();

    let mut buf1: Vec<u8> = Vec::new();
    run_startup_migration_with_writer(&path, &mut buf1);
    let content_after_first = std::fs::read_to_string(&path).unwrap();

    let mut buf2: Vec<u8> = Vec::new();
    run_startup_migration_with_writer(&path, &mut buf2);
    let content_after_second = std::fs::read_to_string(&path).unwrap();

    assert_eq!(
        content_after_first, content_after_second,
        "second run must be a no-op (byte-identical)"
    );
    let stderr_output = String::from_utf8(buf2).unwrap();
    assert!(
        !stderr_output.contains("config migrated"),
        "second run must emit no notice, got: {stderr_output:?}"
    );
}

#[test]
fn driver_preserves_opt_out_false_value() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("maestro.toml");
    let initial = "[views]\nagent_graph_enabled = false\n";
    std::fs::write(&path, initial).unwrap();
    let content_before = std::fs::read_to_string(&path).unwrap();
    let mut buf: Vec<u8> = Vec::new();

    run_startup_migration_with_writer(&path, &mut buf);

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        content_before,
        "opt-out file must be untouched"
    );
    assert!(String::from_utf8(buf).unwrap().is_empty());
}

#[test]
fn driver_does_nothing_when_file_absent() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("maestro.toml");
    let mut buf: Vec<u8> = Vec::new();

    run_startup_migration_with_writer(&path, &mut buf);

    assert!(!path.exists(), "no file should be created on fresh install");
    assert!(String::from_utf8(buf).unwrap().is_empty());
}

#[test]
fn driver_skips_write_and_continues_on_malformed_toml() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("maestro.toml");
    let garbage = "[[[[not valid";
    std::fs::write(&path, garbage).unwrap();
    let mut buf: Vec<u8> = Vec::new();

    run_startup_migration_with_writer(&path, &mut buf);

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        garbage,
        "malformed file must be left untouched so the load-time error is preserved"
    );
}

#[cfg(unix)]
#[test]
fn driver_skips_symlinked_config() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::TempDir::new().unwrap();
    let real = dir.path().join("real.toml");
    let link = dir.path().join("maestro.toml");
    std::fs::write(&real, "[sessions]\ndefault_model = \"opus\"\n").unwrap();
    symlink(&real, &link).unwrap();
    let before = std::fs::read_to_string(&real).unwrap();
    let mut buf: Vec<u8> = Vec::new();

    run_startup_migration_with_writer(&link, &mut buf);

    assert_eq!(
        std::fs::read_to_string(&real).unwrap(),
        before,
        "symlink target must not be migrated"
    );
    assert!(
        link.symlink_metadata().unwrap().file_type().is_symlink(),
        "symlink at maestro.toml must still be a symlink, not replaced by a regular file"
    );
}

#[test]
fn driver_skips_oversized_config() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("maestro.toml");
    // 2 MiB of bytes that happen to be a valid (but giant) TOML comment.
    let mut content = String::from("# pad\n");
    content.push_str(&"x".repeat(2 * 1024 * 1024));
    std::fs::write(&path, &content).unwrap();
    let mut buf: Vec<u8> = Vec::new();

    run_startup_migration_with_writer(&path, &mut buf);

    assert_eq!(
        std::fs::read_to_string(&path).unwrap().len(),
        content.len(),
        "oversized file must not be touched"
    );
}

#[cfg(unix)]
#[test]
fn driver_skips_non_regular_file() {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::net::UnixListener;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("maestro.toml");
    // A unix socket is a non-regular file. Creating one is portable across
    // Linux/macOS without needing mknod (FIFOs need it; sockets don't).
    let _listener = UnixListener::bind(&path).unwrap();
    let mut buf: Vec<u8> = Vec::new();

    run_startup_migration_with_writer(&path, &mut buf);

    // Path still exists as a socket; nothing was rewritten.
    assert!(
        path.symlink_metadata().unwrap().file_type().is_socket(),
        "non-regular file must not be replaced"
    );
}

#[cfg(unix)]
#[test]
fn driver_emits_warning_and_continues_on_write_failure() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("maestro.toml");
    let initial = "[sessions]\ndefault_model = \"opus\"\n";
    std::fs::write(&path, initial).unwrap();

    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

    let mut buf: Vec<u8> = Vec::new();
    run_startup_migration_with_writer(&path, &mut buf);

    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(
        !on_disk.contains("agent_graph_enabled = true"),
        "write must have failed; on-disk file is unchanged: {on_disk}"
    );
}
