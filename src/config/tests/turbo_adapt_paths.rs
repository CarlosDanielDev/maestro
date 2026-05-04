use super::*;

#[test]
fn tui_config_deserializes_color_override() {
    use crate::tui::theme::SerializableColor;
    use ratatui::style::Color;
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
[tui.theme.overrides]
text_primary = "cyan"
"#
    )
    .unwrap();
    let cfg = Config::load(f.path()).expect("load failed");
    assert_eq!(
        cfg.tui.theme.overrides.text_primary,
        Some(SerializableColor(Color::Cyan))
    );
}

// -- TurboQuantConfig --

#[test]
fn turboquant_config_defaults_are_correct() {
    let cfg = TurboQuantConfig::default();
    assert!(!cfg.enabled);
    assert_eq!(cfg.bit_width, 4);
    assert_eq!(cfg.strategy, QuantStrategy::TurboQuant);
    assert_eq!(cfg.apply_to, ApplyTarget::Both);
    assert!(!cfg.auto_on_overflow);
    assert_eq!(cfg.fork_handoff_budget, 4096);
    assert_eq!(cfg.system_prompt_budget, 2048);
    assert_eq!(cfg.knowledge_budget, 4096);
}

#[test]
fn turboquant_config_fork_handoff_budget_defaults_when_absent() {
    let toml_str = r#"
        enabled = true
        bit_width = 4
    "#;
    let cfg: TurboQuantConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.fork_handoff_budget, 4096);
    assert_eq!(cfg.system_prompt_budget, 2048);
    assert_eq!(cfg.knowledge_budget, 4096);
}

#[test]
fn turboquant_config_new_budgets_deserialize_from_toml() {
    let toml_str = r#"
        enabled = true
        fork_handoff_budget = 8192
        system_prompt_budget = 1024
        knowledge_budget = 16384
    "#;
    let cfg: TurboQuantConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.fork_handoff_budget, 8192);
    assert_eq!(cfg.system_prompt_budget, 1024);
    assert_eq!(cfg.knowledge_budget, 16384);
}

#[test]
fn turboquant_config_absent_section_uses_defaults() {
    let toml_str = r#"
        [project]
        repo = "owner/repo"
        base_branch = "main"
        [sessions]
        [budget]
        per_session_usd = 5.0
        total_usd = 50.0
        alert_threshold_pct = 80
        [notifications]
    "#;
    let cfg: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.turboquant, TurboQuantConfig::default());
}

#[test]
fn turboquant_config_serde_round_trip() {
    let cfg = TurboQuantConfig {
        enabled: true,
        bit_width: 6,
        strategy: QuantStrategy::PolarQuant,
        apply_to: ApplyTarget::Keys,
        auto_on_overflow: true,
        fork_handoff_budget: 8192,
        system_prompt_budget: 1024,
        knowledge_budget: 2048,
    };
    let serialized = toml::to_string(&cfg).unwrap();
    let deserialized: TurboQuantConfig = toml::from_str(&serialized).unwrap();
    assert_eq!(cfg, deserialized);
}

#[test]
fn turboquant_config_deserializes_from_toml() {
    let toml_str = r#"
        enabled = true
        bit_width = 2
        strategy = "qjl"
        apply_to = "values"
        auto_on_overflow = true
    "#;
    let cfg: TurboQuantConfig = toml::from_str(toml_str).unwrap();
    assert!(cfg.enabled);
    assert_eq!(cfg.bit_width, 2);
    assert_eq!(cfg.strategy, QuantStrategy::Qjl);
    assert_eq!(cfg.apply_to, ApplyTarget::Values);
    assert!(cfg.auto_on_overflow);
}

#[test]
fn turboquant_config_on_full_config() {
    let toml_str = r#"
        [project]
        repo = "owner/repo"
        base_branch = "main"
        [sessions]
        [budget]
        per_session_usd = 5.0
        total_usd = 50.0
        alert_threshold_pct = 80
        [notifications]
        [turboquant]
        enabled = true
        bit_width = 3
        strategy = "polarquant"
        apply_to = "both"
        auto_on_overflow = false
    "#;
    let cfg: Config = toml::from_str(toml_str).unwrap();
    assert!(cfg.turboquant.enabled);
    assert_eq!(cfg.turboquant.bit_width, 3);
    assert_eq!(cfg.turboquant.strategy, QuantStrategy::PolarQuant);
}

// -- AdaptSettings --

#[test]
fn adapt_settings_default_is_ai_naming() {
    let settings = AdaptSettings::default();
    assert_eq!(settings.milestone_naming, MilestoneNaming::Ai);
    assert!(settings.milestone_template.is_none());
}

#[test]
fn adapt_settings_parses_standard_naming() {
    let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 1
default_model = "opus"
default_mode = "orchestrator"
[budget]
max_cost_per_session = 1.0
max_cost_total = 10.0
[github]
[notifications]
[adapt]
milestone_naming = "standard"
"#;
    let cfg: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.adapt.milestone_naming, MilestoneNaming::Standard);
}

#[test]
fn adapt_settings_parses_custom_naming_with_template() {
    let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 1
default_model = "opus"
default_mode = "orchestrator"
[budget]
max_cost_per_session = 1.0
max_cost_total = 10.0
[github]
[notifications]
[adapt]
milestone_naming = "custom"
milestone_template = "v{n}.0.0 — {title}"
"#;
    let cfg: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.adapt.milestone_naming, MilestoneNaming::Custom);
    assert_eq!(
        cfg.adapt.milestone_template.as_deref(),
        Some("v{n}.0.0 — {title}")
    );
}

#[test]
fn adapt_settings_defaults_when_section_missing() {
    let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 1
default_model = "opus"
default_mode = "orchestrator"
[budget]
max_cost_per_session = 1.0
max_cost_total = 10.0
[github]
[notifications]
"#;
    let cfg: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.adapt.milestone_naming, MilestoneNaming::Ai);
}

// --- Issue #437: LoadedConfig path plumbing ---

#[test]
fn minimal_config_loads_with_domain_defaults() {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), MINIMAL_TOML).unwrap();

    let cfg = Config::load(f.path()).expect("minimal config should load");

    assert_eq!(cfg.project.repo, "owner/repo");
    assert_eq!(cfg.sessions.max_concurrent, 3);
    assert_eq!(cfg.sessions.hollow_retry, HollowRetryConfig::default());
    assert_eq!(cfg.budget.total_usd, 50.0);
    assert!(cfg.github.auto_pr);
    assert!(cfg.notifications.desktop);
    assert!(cfg.gates.enabled);
    assert!(!cfg.review.enabled);
    assert_eq!(cfg.concurrency.heavy_task_limit, 2);
    assert_eq!(cfg.monitoring.work_tick_interval_secs, 10);
    assert!(cfg.plugins.is_empty());
    assert!(cfg.modes.is_empty());
    assert_eq!(cfg.tui, TuiConfig::default());
    assert!(cfg.flags.entries.is_empty());
    assert_eq!(cfg.turboquant, TurboQuantConfig::default());
    assert_eq!(cfg.adapt, AdaptSettings::default());
    assert_eq!(cfg.views, ViewsConfig::default());
}

#[test]
fn find_and_load_in_with_path_returns_resolved_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let file_path = dir.path().join("maestro.toml");
    std::fs::write(&file_path, MINIMAL_TOML).unwrap();

    let loaded = Config::find_and_load_in_with_path(dir.path()).expect("should find config");

    assert!(
        loaded.path.ends_with("maestro.toml"),
        "path must end with maestro.toml, got {:?}",
        loaded.path
    );
    assert_eq!(loaded.config.project.repo, "owner/repo");
}
