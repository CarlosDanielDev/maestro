use super::*;

#[test]
fn project_config_parses_pre_505_toml_unchanged() {
    let toml_str = "[project]\n\
                    repo = \"owner/repo\"\n\
                    [sessions]\n\
                    [budget]\n\
                    per_session_usd = 5.0\n\
                    total_usd = 50.0\n\
                    alert_threshold_pct = 80\n\
                    [github]\n\
                    [notifications]\n";
    let cfg: Config = toml::from_str(toml_str).expect("legacy toml must still parse");
    assert!(cfg.project.language.is_none());
    assert!(cfg.project.languages.is_none());
    assert!(cfg.project.build_command.is_none());
    assert!(cfg.project.test_command.is_none());
    assert!(cfg.project.run_command.is_none());
}

#[test]
fn project_config_parses_new_fields() {
    let toml_str = "[project]\n\
                    repo = \"owner/repo\"\n\
                    language = \"node\"\n\
                    languages = [\"node\", \"python\"]\n\
                    build_command = \"npm run build\"\n\
                    test_command = \"npm test\"\n\
                    run_command = \"npm start\"\n\
                    [sessions]\n\
                    [budget]\n\
                    per_session_usd = 5.0\n\
                    total_usd = 50.0\n\
                    alert_threshold_pct = 80\n\
                    [notifications]\n";
    let cfg: Config = toml::from_str(toml_str).expect("new fields must parse");
    assert_eq!(cfg.project.language.as_deref(), Some("node"));
    assert_eq!(
        cfg.project.languages,
        Some(vec!["node".to_string(), "python".to_string()])
    );
    assert_eq!(cfg.project.build_command.as_deref(), Some("npm run build"));
    assert_eq!(cfg.project.test_command.as_deref(), Some("npm test"));
    assert_eq!(cfg.project.run_command.as_deref(), Some("npm start"));
}

#[test]
fn context_overflow_config_defaults_are_correct() {
    let cfg = ContextOverflowConfig::default();
    assert_eq!(cfg.overflow_threshold_pct, 70);
    assert!(cfg.auto_fork);
    assert_eq!(cfg.commit_prompt_pct, 50);
    assert_eq!(cfg.max_fork_depth, 5);
}

#[test]
fn context_overflow_config_deserializes_from_toml() {
    let toml_str = r#"overflow_threshold_pct = 85"#;
    let cfg: ContextOverflowConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.overflow_threshold_pct, 85);
    assert!(cfg.auto_fork); // default untouched
}

#[test]
fn conflict_policy_default_is_warn() {
    let cfg = ConflictConfig::default();
    assert!(cfg.enabled);
    assert_eq!(cfg.policy, ConflictPolicy::Warn);
}

#[test]
fn conflict_policy_deserializes_pause() {
    let toml_str = r#"policy = "pause""#;
    let cfg: ConflictConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.policy, ConflictPolicy::Pause);
    assert!(cfg.enabled); // default untouched
}

#[test]
fn conflict_policy_deserializes_kill() {
    let toml_str = r#"policy = "kill""#;
    let cfg: ConflictConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.policy, ConflictPolicy::Kill);
}

#[test]
fn conflict_policy_label_round_trips() {
    assert_eq!(ConflictPolicy::Warn.label(), "warn");
    assert_eq!(ConflictPolicy::Pause.label(), "pause");
    assert_eq!(ConflictPolicy::Kill.label(), "kill");
}

#[test]
fn config_uses_context_overflow_defaults_when_section_absent() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    )
    .unwrap();
    let cfg = Config::load(f.path()).expect("load failed");
    assert_eq!(cfg.sessions.context_overflow.overflow_threshold_pct, 70);
}

#[test]
fn completion_gates_config_defaults_when_section_absent() {
    let cfg: CompletionGatesConfig = toml::from_str("").expect("parse failed");
    assert!(cfg.enabled);
    assert!(cfg.commands.is_empty());
}

#[test]
fn completion_gates_config_deserializes_full_entry() {
    let toml_str = r#"
enabled = true
[[commands]]
name = "fmt"
run = "cargo fmt --check"
required = false
"#;
    let cfg: CompletionGatesConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.commands.len(), 1);
    assert_eq!(cfg.commands[0].name, "fmt");
    assert_eq!(cfg.commands[0].run, "cargo fmt --check");
    assert!(!cfg.commands[0].required);
}

#[test]
fn completion_gate_entry_required_defaults_to_true() {
    let toml_str = r#"
name = "fmt"
run = "cargo fmt --check"
"#;
    let entry: CompletionGateEntry = toml::from_str(toml_str).expect("parse failed");
    assert!(entry.required);
}

#[test]
fn completion_gates_config_multiple_entries_parse_in_order() {
    let toml_str = r#"
[[commands]]
name = "fmt"
run = "cargo fmt --check"
[[commands]]
name = "clippy"
run = "cargo clippy -- -D warnings"
"#;
    let cfg: CompletionGatesConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.commands[0].name, "fmt");
    assert_eq!(cfg.commands[1].name, "clippy");
}

#[test]
fn ci_auto_fix_config_defaults() {
    let cfg = CiAutoFixConfig::default();
    assert!(cfg.enabled);
    assert_eq!(cfg.max_retries, 3);
}

#[test]
fn ci_auto_fix_config_deserializes_from_toml() {
    let toml_str = r#"
enabled = false
max_retries = 5
"#;
    let cfg: CiAutoFixConfig = toml::from_str(toml_str).expect("parse failed");
    assert!(!cfg.enabled);
    assert_eq!(cfg.max_retries, 5);
}

#[test]
fn gates_config_ci_auto_fix_defaults_when_absent() {
    let cfg: GatesConfig = toml::from_str("").expect("parse failed");
    assert!(cfg.ci_auto_fix.enabled);
    assert_eq!(cfg.ci_auto_fix.max_retries, 3);
}

#[test]
fn full_config_load_propagates_ci_auto_fix() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
[gates.ci_auto_fix]
max_retries = 7
"#
    )
    .unwrap();
    let cfg = Config::load(f.path()).expect("load failed");
    assert_eq!(cfg.gates.ci_auto_fix.max_retries, 7);
}

#[test]
fn tui_config_defaults_when_section_absent() {
    use crate::tui::theme::ThemePreset;
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    )
    .unwrap();
    let cfg = Config::load(f.path()).expect("load failed");
    assert_eq!(cfg.tui.theme.preset, ThemePreset::Dark);
    assert!(cfg.tui.theme.overrides.text_primary.is_none());
}

#[test]
fn tui_config_deserializes_light_preset() {
    use crate::tui::theme::ThemePreset;
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
[tui.theme]
preset = "light"
"#
    )
    .unwrap();
    let cfg = Config::load(f.path()).expect("load failed");
    assert_eq!(cfg.tui.theme.preset, ThemePreset::Light);
}

// --- Issue #143: FlagsConfig tests ---

#[test]
fn flags_config_defaults_to_empty_hashmap() {
    let cfg = FlagsConfig::default();
    assert!(cfg.entries.is_empty());
}

#[test]
fn flags_config_deserializes_from_toml() {
    let toml_str = r#"
ci_auto_fix = true
review_council = false
"#;
    let cfg: FlagsConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.entries.get("ci_auto_fix"), Some(&true));
    assert_eq!(cfg.entries.get("review_council"), Some(&false));
}

#[test]
fn flags_config_deserializes_multiple_entries() {
    let toml_str = r#"
continuous_mode = false
ci_auto_fix = true
"#;
    let cfg: FlagsConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.entries.get("continuous_mode"), Some(&false));
    assert_eq!(cfg.entries.get("ci_auto_fix"), Some(&true));
    assert_eq!(cfg.entries.len(), 2);
}

#[test]
fn flags_config_handles_unknown_keys() {
    let toml_str = r#"
totally_unknown_flag = true
ci_auto_fix = false
"#;
    let cfg: FlagsConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.entries.len(), 2);
    assert_eq!(cfg.entries.get("totally_unknown_flag"), Some(&true));
}

#[test]
fn full_config_flags_defaults_when_section_absent() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    )
    .unwrap();
    let cfg = Config::load(f.path()).expect("load failed");
    assert!(cfg.flags.entries.is_empty());
}
#[test]
fn flags_config_non_boolean_value_is_rejected() {
    let toml_str = r#"continuous_mode = "yes""#;
    let result = toml::from_str::<FlagsConfig>(toml_str);
    assert!(
        result.is_err(),
        "non-bool flag value must fail to deserialize"
    );
}

#[test]
fn full_config_flags_parses_when_section_present() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
[flags]
ci_auto_fix = true
review_council = false
"#
    )
    .unwrap();
    let cfg = Config::load(f.path()).expect("load failed");
    assert_eq!(cfg.flags.entries.get("ci_auto_fix"), Some(&true));
    assert_eq!(cfg.flags.entries.get("review_council"), Some(&false));
}
