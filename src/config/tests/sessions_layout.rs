use super::*;

// --- Issue #275: configurable hollow retry policy ---
// Group A: merge_legacy_hollow pure function.

#[test]
fn merge_both_none_returns_default() {
    let result = merge_legacy_hollow(None, None);
    assert_eq!(result, HollowRetryConfig::default());
}

#[test]
fn merge_legacy_only_maps_to_work_max_retries() {
    let result = merge_legacy_hollow(None, Some(3));
    assert_eq!(result.policy, HollowRetryPolicy::IntentAware);
    assert_eq!(result.work_max_retries, 3);
    assert_eq!(result.consultation_max_retries, 0);
}

#[test]
fn merge_new_section_only_passes_through() {
    let cfg = HollowRetryConfig {
        policy: HollowRetryPolicy::Always,
        work_max_retries: 7,
        consultation_max_retries: 1,
    };
    let result = merge_legacy_hollow(Some(cfg.clone()), None);
    assert_eq!(result, cfg);
}

#[test]
fn merge_both_new_wins() {
    let cfg = HollowRetryConfig {
        policy: HollowRetryPolicy::Always,
        work_max_retries: 7,
        consultation_max_retries: 1,
    };
    let result = merge_legacy_hollow(Some(cfg.clone()), Some(99));
    assert_eq!(result, cfg);
    assert_ne!(result.work_max_retries, 99);
}

#[test]
fn merge_legacy_zero_is_respected() {
    let result = merge_legacy_hollow(None, Some(0));
    assert_eq!(result.work_max_retries, 0);
    assert_eq!(result.consultation_max_retries, 0);
}

// Group B: HollowRetryPolicy enum.

#[test]
fn hollow_retry_policy_defaults_to_intent_aware() {
    assert_eq!(HollowRetryPolicy::default(), HollowRetryPolicy::IntentAware);
}

#[test]
fn hollow_retry_policy_serializes_as_kebab_case() {
    assert_eq!(
        serde_json::to_string(&HollowRetryPolicy::IntentAware).unwrap(),
        r#""intent-aware""#
    );
    assert_eq!(
        serde_json::to_string(&HollowRetryPolicy::Always).unwrap(),
        r#""always""#
    );
    assert_eq!(
        serde_json::to_string(&HollowRetryPolicy::Never).unwrap(),
        r#""never""#
    );
}

#[test]
fn hollow_retry_policy_deserializes_from_kebab_case() {
    let p: HollowRetryPolicy = serde_json::from_str(r#""intent-aware""#).unwrap();
    assert_eq!(p, HollowRetryPolicy::IntentAware);
    let p: HollowRetryPolicy = serde_json::from_str(r#""never""#).unwrap();
    assert_eq!(p, HollowRetryPolicy::Never);
    let p: HollowRetryPolicy = serde_json::from_str(r#""always""#).unwrap();
    assert_eq!(p, HollowRetryPolicy::Always);
}

// Group C: HollowRetryConfig defaults + serde.

#[test]
fn hollow_retry_config_default_is_intent_aware_with_expected_limits() {
    let cfg = HollowRetryConfig::default();
    assert_eq!(cfg.policy, HollowRetryPolicy::IntentAware);
    assert_eq!(cfg.work_max_retries, 2);
    assert_eq!(cfg.consultation_max_retries, 0);
}

#[test]
fn hollow_retry_config_round_trips_via_serde() {
    let cfg = HollowRetryConfig {
        policy: HollowRetryPolicy::Never,
        work_max_retries: 5,
        consultation_max_retries: 1,
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let rt: HollowRetryConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(rt, cfg);
}

// Group D: SessionsConfig TOML parsing.

#[test]
fn sessions_config_parses_new_hollow_retry_section() {
    let toml_str = r#"
[hollow_retry]
policy = "never"
work_max_retries = 4
consultation_max_retries = 1
"#;
    let cfg: SessionsConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.hollow_retry.policy, HollowRetryPolicy::Never);
    assert_eq!(cfg.hollow_retry.work_max_retries, 4);
    assert_eq!(cfg.hollow_retry.consultation_max_retries, 1);
}

#[test]
fn sessions_config_parses_legacy_hollow_max_retries() {
    let toml_str = "hollow_max_retries = 3";
    let cfg: SessionsConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.hollow_retry.work_max_retries, 3);
    assert_eq!(cfg.hollow_retry.policy, HollowRetryPolicy::IntentAware);
    assert_eq!(cfg.hollow_retry.consultation_max_retries, 0);
}

#[test]
fn sessions_config_new_section_wins_over_legacy() {
    let toml_str = r#"
hollow_max_retries = 99
[hollow_retry]
work_max_retries = 5
"#;
    let cfg: SessionsConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.hollow_retry.work_max_retries, 5);
}

#[test]
fn sessions_config_empty_sessions_uses_default_hollow_retry() {
    let cfg: SessionsConfig = toml::from_str("").expect("parse failed");
    assert_eq!(cfg.hollow_retry, HollowRetryConfig::default());
}

#[test]
fn sessions_config_round_trips() {
    let original: SessionsConfig = toml::from_str(
        r#"
[hollow_retry]
policy = "never"
work_max_retries = 4
consultation_max_retries = 2
"#,
    )
    .expect("parse failed");
    let serialized = toml::to_string_pretty(&original).expect("serialize failed");
    let rt: SessionsConfig = toml::from_str(&serialized).expect("reparse failed");
    assert_eq!(rt.hollow_retry, original.hollow_retry);
}

#[test]
fn max_prompt_history_defaults_to_100() {
    let cfg: SessionsConfig = toml::from_str("").expect("parse failed");
    assert_eq!(cfg.max_prompt_history, 100);
}

#[test]
fn max_prompt_history_deserializes_from_toml() {
    let toml_str = r#"max_prompt_history = 50"#;
    let cfg: SessionsConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.max_prompt_history, 50);
}

#[test]
fn full_config_hollow_retry_defaults_when_absent() {
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
    assert_eq!(cfg.sessions.hollow_retry, HollowRetryConfig::default());
}

// --- Issue #121: LayoutConfig tests ---

#[test]
fn layout_config_defaults() {
    let cfg = LayoutConfig::default();
    assert_eq!(cfg.mode, LayoutMode::Vertical);
    assert_eq!(cfg.density, Density::Default);
    assert_eq!(cfg.preview_ratio, 50);
    assert_eq!(cfg.activity_log_height, 25);
}

#[test]
fn layout_config_deserializes_from_toml() {
    let toml_str = r#"
mode = "horizontal"
density = "compact"
preview_ratio = 60
activity_log_height = 30
"#;
    let cfg: LayoutConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.mode, LayoutMode::Horizontal);
    assert_eq!(cfg.density, Density::Compact);
    assert_eq!(cfg.preview_ratio, 60);
    assert_eq!(cfg.activity_log_height, 30);
}

#[test]
fn layout_config_partial_deserializes() {
    let toml_str = r#"mode = "horizontal""#;
    let cfg: LayoutConfig = toml::from_str(toml_str).expect("parse failed");
    assert_eq!(cfg.mode, LayoutMode::Horizontal);
    assert_eq!(cfg.density, Density::Default);
    assert_eq!(cfg.preview_ratio, 50);
}

#[test]
fn layout_config_round_trips() {
    let cfg = LayoutConfig {
        mode: LayoutMode::Horizontal,
        density: Density::Comfortable,
        preview_ratio: 40,
        activity_log_height: 20,
    };
    let toml_str = toml::to_string_pretty(&cfg).unwrap();
    let reloaded: LayoutConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(cfg, reloaded);
}
