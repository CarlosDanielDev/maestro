use super::*;

#[test]
fn behavior_launch_default_produce_pr_true_interaction_false() {
    let defaults = LaunchBehaviorConfig::default();
    assert!(defaults.default_produce_pr);
    assert!(!defaults.default_interaction);
}

#[test]
fn config_without_behavior_table_uses_hard_coded_launch_defaults() {
    let cfg: Config = toml::from_str(MINIMAL_TOML).expect("minimal config must parse");
    assert_eq!(cfg.launch_defaults(), (true, false));
}

#[test]
fn config_behavior_launch_produce_pr_false_overrides_default() {
    let toml_str = format!(
        r#"{MINIMAL_TOML}
[behavior.launch]
default_produce_pr = false
"#
    );
    let cfg: Config = toml::from_str(&toml_str).expect("config must parse");
    assert_eq!(cfg.launch_defaults(), (false, false));
}

#[test]
fn config_behavior_launch_both_keys_explicit_echo_through() {
    let toml_str = format!(
        r#"{MINIMAL_TOML}
[behavior.launch]
default_produce_pr = false
default_interaction = true
"#
    );
    let cfg: Config = toml::from_str(&toml_str).expect("config must parse");
    assert_eq!(cfg.launch_defaults(), (false, true));
}

#[test]
fn config_behavior_launch_unknown_key_is_parse_error() {
    let toml_str = format!(
        r#"{MINIMAL_TOML}
[behavior.launch]
unknown_key = true
"#
    );
    let result = toml::from_str::<Config>(&toml_str);
    assert!(
        result.is_err(),
        "unknown key under [behavior.launch] must be rejected by deny_unknown_fields"
    );
}
