use super::*;

#[test]
fn find_and_load_in_with_path_finds_nested_candidate() {
    let dir = tempfile::TempDir::new().unwrap();
    let nested = dir.path().join(".maestro");
    std::fs::create_dir_all(&nested).unwrap();
    let nested_toml = MINIMAL_TOML.replacen("owner/repo", "nested/repo", 1);
    std::fs::write(nested.join("config.toml"), &nested_toml).unwrap();

    let loaded = Config::find_and_load_in_with_path(dir.path()).expect("should find nested config");

    assert!(
        loaded.path.ends_with("config.toml"),
        "path must end with config.toml, got {:?}",
        loaded.path
    );
    assert_eq!(loaded.config.project.repo, "nested/repo");
}

#[test]
fn find_and_load_in_with_path_errors_when_absent() {
    let dir = tempfile::TempDir::new().unwrap();
    let result = Config::find_and_load_in_with_path(dir.path());
    assert!(result.is_err(), "should error when no config file present");
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("No maestro.toml"),
        "error message should mention 'No maestro.toml', got: {msg}"
    );
}

#[test]
fn find_and_load_shim_still_returns_config_only() {
    // Regression guard: the legacy API must keep returning Result<Config>, not LoadedConfig.
    let _: fn() -> anyhow::Result<Config> = Config::find_and_load;
}

#[test]
fn tui_mascot_style_serde_roundtrip() {
    use crate::mascot::MascotStyle;

    for variant in [MascotStyle::Sprite, MascotStyle::Ascii] {
        let cfg = TuiConfig {
            mascot_style: variant,
            ..Default::default()
        };
        let serialized = toml::to_string(&cfg).expect("serialize");
        let back: TuiConfig = toml::from_str(&serialized).expect("deserialize");
        assert_eq!(back.mascot_style, variant);
    }
}

#[test]
fn tui_mascot_style_emits_lowercase_on_disk() {
    use crate::mascot::MascotStyle;
    let cfg = TuiConfig {
        mascot_style: MascotStyle::Sprite,
        ..Default::default()
    };
    let serialized = toml::to_string(&cfg).unwrap();
    assert!(
        serialized.contains(r#"mascot_style = "sprite""#),
        "on-disk spelling must stay lowercase for human-edited TOML: {serialized}"
    );
}

#[test]
fn tui_mascot_style_defaults_to_sprite_when_absent() {
    use crate::mascot::MascotStyle;
    let parsed: TuiConfig = toml::from_str("").expect("empty TuiConfig parses");
    assert_eq!(parsed.mascot_style, MascotStyle::Sprite);
}

#[test]
fn mascot_style_rejects_unknown_value() {
    let err = toml::from_str::<TuiConfig>(r#"mascot_style = "foo""#)
        .expect_err("unknown variant must fail");
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("unknown")
            || msg.to_lowercase().contains("expected")
            || msg.contains("foo"),
        "error should reference the unknown value, got: {msg}"
    );
}

#[test]
fn notifications_config_desktop_false_survives_toml_round_trip() {
    let original = NotificationsConfig {
        desktop: false,
        slack: false,
        slack_webhook_url: None,
        slack_rate_limit_per_min: 10,
    };

    let serialized = toml::to_string(&original).expect("serialize");
    let deserialized: NotificationsConfig = toml::from_str(&serialized).expect("deserialize");

    assert!(!deserialized.desktop);
}

#[test]
fn notifications_config_missing_desktop_field_defaults_to_true() {
    let toml_str = r#"slack = false"#;

    let cfg: NotificationsConfig = toml::from_str(toml_str).expect("deserialize");

    assert!(cfg.desktop, "missing `desktop` key must default to true");
}

// --- Issue #525: ViewsConfig tests ---

#[test]
fn views_config_defaults_when_section_absent() {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), MINIMAL_TOML).unwrap();
    let cfg = Config::load(f.path()).expect("load failed");
    assert!(!cfg.views.agent_graph_enabled);
}

#[test]
fn views_config_parses_agent_graph_enabled_true() {
    let cfg: ViewsConfig = toml::from_str(r#"agent_graph_enabled = true"#).expect("parse failed");
    assert!(cfg.agent_graph_enabled);
}

#[test]
fn views_config_parses_agent_graph_enabled_false() {
    let cfg: ViewsConfig = toml::from_str(r#"agent_graph_enabled = false"#).expect("parse failed");
    assert!(!cfg.agent_graph_enabled);
}

#[test]
fn views_config_rejects_non_bool_agent_graph_enabled() {
    let err = toml::from_str::<ViewsConfig>(r#"agent_graph_enabled = "yes""#)
        .expect_err("string value must fail to parse as bool");
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("boolean")
            || msg.to_lowercase().contains("expected")
            || msg.contains("yes"),
        "error should reference the invalid value, got: {msg}"
    );
}

#[test]
fn views_config_round_trips_through_toml() {
    let original = ViewsConfig {
        agent_graph_enabled: true,
    };
    let serialized = toml::to_string(&original).expect("serialize");
    let deserialized: ViewsConfig = toml::from_str(&serialized).expect("deserialize");
    assert_eq!(original, deserialized);
}

#[test]
fn config_load_propagates_views_parse_error_with_file_path() {
    let f = tempfile::NamedTempFile::new().unwrap();
    let toml = format!("{MINIMAL_TOML}[views]\nagent_graph_enabled = \"yes\"\n");
    std::fs::write(f.path(), toml).unwrap();
    let err = Config::load(f.path()).expect_err("malformed views value must fail");
    let chain = format!("{err:#}");
    let path_str = f.path().to_string_lossy();
    assert!(
        chain.contains(path_str.as_ref()),
        "error chain must include the file path for debuggability, got: {chain}"
    );
}
