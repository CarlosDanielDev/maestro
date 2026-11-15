//! Byte-identity tests for `plan_v0_25_1_migration` (issue #718).
//!
//! Split from `migrate_tests.rs` to keep both files under the 400-line
//! file-size guardrail. These tests pin the `toml_edit`-based rewrite:
//! every original line must survive byte-identical; only new content is
//! appended.

use super::*;

fn assert_original_lines_untouched(before: &str, after: &str) {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    for original_line in &before_lines {
        assert!(
            after_lines.contains(original_line),
            "original line was mutated or removed.\n  missing: {original_line:?}\n--- before ---\n{before}\n--- after ---\n{after}"
        );
    }
}

#[test]
fn plan_preserves_comments_when_inserting_views_section() {
    let toml_in = concat!(
        "# maestro configuration file\n",
        "# managed by your team\n",
        "\n",
        "[project]\n",
        "repo = \"owner/repo\"\n",
        "\n",
        "# Session limits and defaults\n",
        "[sessions]\n",
        "default_model = \"opus\"\n",
    );
    let outcome = plan_v0_25_1_migration(toml_in).unwrap();
    let MigrationOutcome::Migrated {
        new_toml,
        added_keys,
    } = outcome
    else {
        panic!("expected Migrated when [views] absent, got AlreadyCurrent");
    };
    assert!(added_keys.iter().any(|k| k == "views.agent_graph_enabled"));
    assert_original_lines_untouched(toml_in, &new_toml);
    assert!(new_toml.contains("agent_graph_enabled = true"));
    assert!(toml::from_str::<toml::Value>(&new_toml).is_ok());
}

#[test]
fn plan_preserves_blank_lines_around_sections() {
    let toml_in = concat!(
        "[sessions]\n",
        "default_model = \"opus\"\n",
        "\n",
        "\n",
        "[budget]\n",
        "per_session_usd = 5.0\n",
        "\n",
    );
    let outcome = plan_v0_25_1_migration(toml_in).unwrap();
    let MigrationOutcome::Migrated { new_toml, .. } = outcome else {
        panic!("expected Migrated");
    };
    let blank_count_before = toml_in.lines().filter(|l| l.is_empty()).count();
    let blank_count_after = new_toml.lines().filter(|l| l.is_empty()).count();
    assert!(
        blank_count_after >= blank_count_before,
        "blank lines must be preserved (before={blank_count_before} after={blank_count_after}):\n{new_toml}"
    );
    assert_original_lines_untouched(toml_in, &new_toml);
}

#[test]
fn plan_preserves_unmodeled_sections() {
    let toml_in = concat!(
        "[sessions]\n",
        "default_model = \"opus\"\n",
        "\n",
        "[experimental]\n",
        "some_flag = true\n",
        "another_key = 42\n",
    );
    let outcome = plan_v0_25_1_migration(toml_in).unwrap();
    let MigrationOutcome::Migrated { new_toml, .. } = outcome else {
        panic!("expected Migrated");
    };
    assert!(new_toml.contains("[experimental]"));
    assert!(new_toml.contains("some_flag = true"));
    assert!(new_toml.contains("another_key = 42"));
    assert_original_lines_untouched(toml_in, &new_toml);
}

#[test]
fn plan_byte_identity_when_views_present_but_key_absent_preserves_existing_key() {
    let toml_in = concat!(
        "[sessions]\n",
        "default_model = \"opus\"\n",
        "\n",
        "[views]\n",
        "some_other_setting = 1\n",
    );
    let outcome = plan_v0_25_1_migration(toml_in).unwrap();
    let MigrationOutcome::Migrated { new_toml, .. } = outcome else {
        panic!("expected Migrated");
    };
    assert!(new_toml.contains("[views]"));
    assert!(new_toml.contains("some_other_setting = 1"));
    assert!(new_toml.contains("agent_graph_enabled = true"));
    assert_original_lines_untouched(toml_in, &new_toml);
}

#[test]
fn driver_preserves_comments_after_migration() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("maestro.toml");
    let initial = concat!(
        "# my project config\n",
        "# do not edit manually\n",
        "\n",
        "[project]\n",
        "repo = \"owner/repo\"\n",
        "\n",
        "# sessions block\n",
        "[sessions]\n",
        "default_model = \"sonnet\"\n",
    );
    std::fs::write(&path, initial).unwrap();
    let mut buf: Vec<u8> = Vec::new();

    run_startup_migration_with_writer(&path, &mut buf);

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_original_lines_untouched(initial, &on_disk);
    assert!(on_disk.contains("agent_graph_enabled = true"));
    assert!(on_disk.contains("# my project config"));
    assert!(on_disk.contains("# sessions block"));

    let stderr = String::from_utf8(buf).unwrap();
    assert!(
        stderr.contains("[maestro] config migrated: added views.agent_graph_enabled = true"),
        "migration notice must be emitted: {stderr:?}"
    );
}
