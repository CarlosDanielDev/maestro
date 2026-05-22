//! Tests for `src/config/schema.rs`.
//!
//! Loaded via `#[cfg(test)] #[path = "schema_tests.rs"] mod tests;` so the
//! parent module stays under the 400-LOC file-size cap (see
//! `docs/RUST-GUARDRAILS.md` §7). Mirrors the convention used by
//! `docs_render_tests.rs` and `migrate_tests.rs`.

use super::*;
use crate::config::Config;

const MINIMAL_TOML: &str = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[sessions.completion_gates]\nenabled = true\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\nslack_webhook_url = \"\"\n";

// Decremented from 53 → 51 after `default_model` and `permission_mode`
// were removed from SESSIONS_FIELDS in favor of per-provider configuration
// (live on `[agents.<id>]`). Bumped 51 → 93 in #788 after backfilling
// project/sessions/review/tui/turboquant/concurrency sections.
// Walker breakdown: +5 (project) +3 (sessions: allowed_tools, max_prompt_history,
// guardrail_prompt) +0 (review: only reviewers VecOfStruct added) +2 (tui:
// show_mascot, mascot_style) +0 (tui.theme top-level: overrides is NestedTable)
// +28 (tui.theme.overrides leaves) +3 (turboquant token budgets) +1
// (concurrency.team_max_parallel) = +42.
const EXPECTED_STATIC_NON_NESTED_FIELDS: usize = 93;
// Bumped 3 → 4 in #788 — `review.reviewers` registered as VecOfStruct.
const EXPECTED_DYNAMIC_CARDINALITY_SLOTS: usize = 4;

const EMPTY: &[FieldSchema] = &[];

const fn dummy_field(kind: FieldKind, default: DefaultValue) -> FieldSchema {
    FieldSchema {
        key: "k",
        label: "K",
        help: "k",
        default,
        kind,
        validator: None,
        presentation: None,
    }
}

fn default_config() -> Config {
    toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML fixture must parse")
}

fn config_as_toml_value(config: &Config) -> toml::Value {
    toml::Value::try_from(config).expect("Config must serialize to toml::Value")
}

fn resolve<'a>(root: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    path.split('.').try_fold(root, |node, seg| node.get(seg))
}

fn has_internal_sentence_boundary(s: &str) -> bool {
    s.as_bytes()
        .windows(3)
        .any(|w| matches!(w[0], b'.' | b'!' | b'?') && w[1] == b' ' && w[2].is_ascii_uppercase())
}

fn find_validator(dotted: &str) -> Option<Validator> {
    let (table_name, field_key) = dotted.rsplit_once('.')?;
    schema_for_config()
        .iter()
        .find(|t| t.name == table_name)?
        .fields
        .iter()
        .find(|f| f.key == field_key)?
        .validator
}

fn walk_table_paths(
    toml_val: &toml::Value,
    prefix: &str,
    fields: &'static [FieldSchema],
    walked: &mut usize,
    dynamic_slots: &mut usize,
) {
    for field in fields {
        match field.kind {
            FieldKind::NestedTable(inner) => {
                let next_prefix = format!("{prefix}.{}", field.key);
                walk_table_paths(toml_val, &next_prefix, inner, walked, dynamic_slots);
            }
            FieldKind::Map { .. }
            | FieldKind::FlattenedMap { .. }
            | FieldKind::VecOfStruct { .. } => {
                *dynamic_slots += 1;
            }
            _ => {
                let path = format!("{prefix}.{}", field.key);
                // Fields whose Rust type is `Option<T>` with
                // `#[serde(skip_serializing_if = "Option::is_none")]` are
                // omitted from the serialized default Config when their value
                // is `None`. The schema marks these "optional-unset" using
                // the sentinel defaults `DefaultValue::Str("")`,
                // `Int(0)`, or `StrList(&[])` (which the renderer prints as
                // `unset` / `\`0\`` / `\`[]\``). The walker must accept a
                // missing path for these sentinels.
                let is_optional_unset = matches!(
                    field.default,
                    DefaultValue::Str("") | DefaultValue::Int(0) | DefaultValue::StrList(&[])
                );
                assert!(
                    resolve(toml_val, &path).is_some() || is_optional_unset,
                    "schema path not found in serialized Config: {path}"
                );
                *walked += 1;
            }
        }
    }
}

#[test]
fn schema_all_dotted_static_paths_resolve_on_default_config() {
    let config = default_config();
    let toml_val = config_as_toml_value(&config);

    let mut walked = 0usize;
    let mut dynamic_slots = 0usize;
    for table in schema_for_config() {
        walk_table_paths(
            &toml_val,
            table.name,
            table.fields,
            &mut walked,
            &mut dynamic_slots,
        );
    }
    assert_eq!(
        walked, EXPECTED_STATIC_NON_NESTED_FIELDS,
        "expected {EXPECTED_STATIC_NON_NESTED_FIELDS} static non-nested fields, walked {walked}"
    );
    assert_eq!(
        dynamic_slots, EXPECTED_DYNAMIC_CARDINALITY_SLOTS,
        "expected {EXPECTED_DYNAMIC_CARDINALITY_SLOTS} dynamic-cardinality slots, got {dynamic_slots}"
    );
}

#[test]
fn schema_enum_defaults_are_within_allowed_variants() {
    let config = default_config();
    let toml_val = config_as_toml_value(&config);

    for table in schema_for_config() {
        for field in table.fields {
            let FieldKind::Enum(variants) = &field.kind else {
                continue;
            };
            let path = format!("{}.{}", table.name, field.key);
            let node = resolve(&toml_val, &path)
                .unwrap_or_else(|| panic!("enum field not found in Config TOML: {path}"));
            let actual = node
                .as_str()
                .unwrap_or_else(|| panic!("enum field {path} serializes as non-string: {node:?}"));
            assert!(
                variants.contains(&actual),
                "enum field {path}: default value {actual:?} not in allowed list {variants:?}"
            );
        }
    }
}

#[test]
fn schema_help_text_style() {
    for table in schema_for_config() {
        for field in table.fields {
            let path = format!("{}.{}", table.name, field.key);
            let h = field.help;
            assert!(!h.is_empty(), "{path}: help must not be empty");
            assert!(
                h.len() <= 120,
                "{path}: help length {} exceeds 120 chars: {h:?}",
                h.len()
            );
            assert!(
                !matches!(h.as_bytes().last(), Some(b'.' | b'!' | b'?')),
                "{path}: help must not end with sentence punctuation: {h:?}"
            );
            assert!(
                !has_internal_sentence_boundary(h),
                "{path}: help contains internal sentence boundary: {h:?}"
            );
        }
    }
}

#[test]
fn schema_default_value_variant_matches_field_kind() {
    for table in schema_for_config() {
        for field in table.fields {
            let path = format!("{}.{}", table.name, field.key);
            let ok = matches!(
                (&field.default, &field.kind),
                (DefaultValue::Bool(_), FieldKind::Bool)
                    | (DefaultValue::Int(_), FieldKind::Int { .. })
                    | (DefaultValue::Float(_), FieldKind::Float { .. })
                    | (DefaultValue::Str(_), FieldKind::String | FieldKind::Enum(_))
                    | (DefaultValue::StrList(_), FieldKind::StringList)
                    | (DefaultValue::Nested, FieldKind::NestedTable(_))
                    | (
                        DefaultValue::Empty,
                        FieldKind::Map { .. }
                            | FieldKind::FlattenedMap { .. }
                            | FieldKind::VecOfStruct { .. },
                    ),
            );
            assert!(
                ok,
                "{path}: DefaultValue {:?} incompatible with FieldKind {:?}",
                field.default, field.kind
            );
        }
    }
}

#[test]
fn schema_no_duplicate_dotted_paths() {
    let mut paths: Vec<String> = schema_for_config()
        .iter()
        .flat_map(|t| {
            t.fields
                .iter()
                .map(move |f| format!("{}.{}", t.name, f.key))
        })
        .collect();
    paths.sort();
    let total = paths.len();
    paths.dedup();
    assert_eq!(
        paths.len(),
        total,
        "duplicate dotted paths detected in schema registry"
    );
}

#[test]
fn schema_validators_are_pure_functions() {
    let url_validator = find_validator("notifications.slack_webhook_url")
        .expect("notifications.slack_webhook_url must have a validator");

    assert!(
        url_validator(&toml::Value::String(String::new())).is_ok(),
        "validate_url_or_empty: empty string must be Ok"
    );
    assert!(
        url_validator(&toml::Value::String(
            "https://hooks.slack.com/services/T/B/X".into()
        ))
        .is_ok(),
        "validate_url_or_empty: valid https URL must be Ok"
    );
    assert!(
        url_validator(&toml::Value::String("not-a-url".into())).is_err(),
        "validate_url_or_empty: bare word must be Err"
    );
    assert!(
        url_validator(&toml::Value::String("http://".into())).is_err(),
        "validate_url_or_empty: scheme-only string must be Err"
    );

    for field_path in ["review.command", "project.repo"] {
        let v = find_validator(field_path)
            .unwrap_or_else(|| panic!("{field_path} must have a validator"));
        assert!(
            v(&toml::Value::String("cargo test".into())).is_ok(),
            "{field_path}: non-empty string must be Ok"
        );
        let err = v(&toml::Value::String(String::new()));
        assert!(err.is_err(), "{field_path}: empty string must be Err");
        assert!(
            !err.unwrap_err().is_empty(),
            "{field_path}: error message must not be empty"
        );
    }
}

#[test]
fn presentation_default_resolves_to_subtabs_for_map() {
    let field = dummy_field(
        FieldKind::Map {
            entry_fields: EMPTY,
        },
        DefaultValue::Empty,
    );
    assert!(
        matches!(field.resolved_presentation(), Some(Presentation::Subtabs)),
        "Map with presentation:None must resolve to Subtabs, got {:?}",
        field.resolved_presentation()
    );
}

#[test]
fn presentation_default_resolves_to_rows_for_vecofstruct() {
    let field = dummy_field(
        FieldKind::VecOfStruct {
            entry_fields: EMPTY,
        },
        DefaultValue::Empty,
    );
    assert!(
        matches!(field.resolved_presentation(), Some(Presentation::Rows)),
        "VecOfStruct with presentation:None must resolve to Rows, got {:?}",
        field.resolved_presentation()
    );
}

#[test]
fn presentation_explicit_subtabs_on_vecofstruct_overrides_default() {
    let field = FieldSchema {
        key: "k",
        label: "K",
        help: "k",
        default: DefaultValue::Empty,
        kind: FieldKind::VecOfStruct {
            entry_fields: EMPTY,
        },
        validator: None,
        presentation: Some(Presentation::Subtabs),
    };
    assert!(
        matches!(field.resolved_presentation(), Some(Presentation::Subtabs)),
        "explicit Subtabs must override VecOfStruct default (Rows), got {:?}",
        field.resolved_presentation()
    );
}

#[test]
fn presentation_explicit_rows_on_map_overrides_default() {
    let field = FieldSchema {
        key: "k",
        label: "K",
        help: "k",
        default: DefaultValue::Empty,
        kind: FieldKind::Map {
            entry_fields: EMPTY,
        },
        validator: None,
        presentation: Some(Presentation::Rows),
    };
    assert!(
        matches!(field.resolved_presentation(), Some(Presentation::Rows)),
        "explicit Rows must override Map default (Subtabs), got {:?}",
        field.resolved_presentation()
    );
}

#[test]
fn presentation_returns_none_for_all_static_variants() {
    let static_fields: &[FieldSchema] = &[
        dummy_field(FieldKind::Bool, DefaultValue::Bool(false)),
        dummy_field(
            FieldKind::Int {
                min: 0,
                max: 10,
                step: 1,
            },
            DefaultValue::Int(0),
        ),
        dummy_field(
            FieldKind::Float {
                min: 0.0,
                max: 1.0,
                step: 0.1,
                display_scale: 10,
            },
            DefaultValue::Float(0.0),
        ),
        dummy_field(FieldKind::String, DefaultValue::Str("")),
        dummy_field(FieldKind::Enum(&["a", "b"]), DefaultValue::Str("a")),
        dummy_field(FieldKind::StringList, DefaultValue::StrList(&[])),
        dummy_field(FieldKind::NestedTable(EMPTY), DefaultValue::Nested),
    ];

    for field in static_fields {
        assert!(
            field.resolved_presentation().is_none(),
            "static variant {:?} must return None, got {:?}",
            field.kind,
            field.resolved_presentation()
        );
    }
}

#[test]
fn dynamic_field_kinds_have_stable_debug_shape() {
    const TINY: &[FieldSchema] = &[];

    let map_kind = FieldKind::Map { entry_fields: TINY };
    let flat_kind = FieldKind::FlattenedMap { entry_fields: TINY };
    let vec_kind = FieldKind::VecOfStruct { entry_fields: TINY };

    let map_debug_a = format!("{map_kind:?}");
    let map_debug_b = format!("{map_kind:?}");
    assert!(
        map_debug_a.starts_with("Map { entry_fields:"),
        "FieldKind::Map Debug must start with 'Map {{ entry_fields:', got: {map_debug_a:?}"
    );
    assert_eq!(
        map_debug_a, map_debug_b,
        "FieldKind::Map Debug must be idempotent"
    );

    let flat_debug = format!("{flat_kind:?}");
    assert!(
        flat_debug.starts_with("FlattenedMap { entry_fields:"),
        "FieldKind::FlattenedMap Debug must start with 'FlattenedMap {{ entry_fields:', got: {flat_debug:?}"
    );

    let vec_debug_a = format!("{vec_kind:?}");
    let vec_debug_b = format!("{vec_kind:?}");
    assert!(
        vec_debug_a.starts_with("VecOfStruct { entry_fields:"),
        "FieldKind::VecOfStruct Debug must start with 'VecOfStruct {{ entry_fields:', got: {vec_debug_a:?}"
    );
    assert_eq!(
        vec_debug_a, vec_debug_b,
        "FieldKind::VecOfStruct Debug must be idempotent"
    );
}

#[test]
fn dynamic_field_kinds_are_const_constructible() {
    const _MAP: FieldKind = FieldKind::Map {
        entry_fields: EMPTY,
    };
    const _FLAT: FieldKind = FieldKind::FlattenedMap {
        entry_fields: EMPTY,
    };
    const _VEC: FieldKind = FieldKind::VecOfStruct {
        entry_fields: EMPTY,
    };
    const _FIELD_MAP: FieldSchema = dummy_field(_MAP, DefaultValue::Empty);
    const _FIELD_FLAT: FieldSchema = dummy_field(_FLAT, DefaultValue::Empty);
    const _FIELD_VEC: FieldSchema = dummy_field(_VEC, DefaultValue::Empty);
    assert!(matches!(_FIELD_MAP.kind, FieldKind::Map { .. }));
    assert!(matches!(_FIELD_FLAT.kind, FieldKind::FlattenedMap { .. }));
    assert!(matches!(_FIELD_VEC.kind, FieldKind::VecOfStruct { .. }));
}

#[test]
fn flattened_map_default_resolves_to_subtabs() {
    let field = dummy_field(
        FieldKind::FlattenedMap {
            entry_fields: EMPTY,
        },
        DefaultValue::Empty,
    );
    assert!(
        matches!(field.resolved_presentation(), Some(Presentation::Subtabs)),
        "FlattenedMap with presentation:None must resolve to Subtabs, got {:?}",
        field.resolved_presentation()
    );
}

#[test]
fn agents_table_registered_with_flattened_map() {
    let schema = schema_for_config();
    let agents_table = schema
        .iter()
        .find(|t| t.name == "agents")
        .expect("agents TableSchema must be registered");
    assert_eq!(agents_table.label, "Agents");
    let flattened = agents_table
        .fields
        .iter()
        .find(|f| matches!(f.kind, FieldKind::FlattenedMap { .. }))
        .expect("agents table must expose a FlattenedMap field");
    let FieldKind::FlattenedMap { entry_fields } = flattened.kind else {
        panic!("expected FlattenedMap variant");
    };
    assert_eq!(
        entry_fields.len(),
        11,
        "AGENTS_ENTRY_FIELDS must lock at 11 scalar/list fields"
    );
}

#[test]
fn agents_entry_fields_use_actual_rust_field_names() {
    let schema = schema_for_config();
    let agents_table = schema.iter().find(|t| t.name == "agents").unwrap();
    let FieldKind::FlattenedMap { entry_fields } = agents_table
        .fields
        .iter()
        .find(|f| matches!(f.kind, FieldKind::FlattenedMap { .. }))
        .unwrap()
        .kind
    else {
        panic!();
    };
    let keys: Vec<&str> = entry_fields.iter().map(|f| f.key).collect();
    for required in [
        "kind",
        "enabled",
        "command",
        "base_url",
        "model",
        "extra_args",
        "permission_mode",
        "allowed_tools",
        "sandbox",
        "request_timeout_secs",
        "api_key_env",
    ] {
        assert!(
            keys.contains(&required),
            "AGENTS_ENTRY_FIELDS missing required key `{required}` — found {keys:?}"
        );
    }
}

#[test]
fn modes_table_registered_with_three_entry_fields() {
    let schema = schema_for_config();
    let modes_table = schema
        .iter()
        .find(|t| t.name == "modes")
        .expect("modes TableSchema must be registered");
    let FieldKind::FlattenedMap { entry_fields } = modes_table
        .fields
        .iter()
        .find(|f| matches!(f.kind, FieldKind::FlattenedMap { .. }))
        .unwrap()
        .kind
    else {
        panic!();
    };
    assert_eq!(entry_fields.len(), 3);
    let keys: Vec<&str> = entry_fields.iter().map(|f| f.key).collect();
    assert!(keys.contains(&"system_prompt"));
    assert!(keys.contains(&"allowed_tools"));
    assert!(keys.contains(&"permission_mode"));
}

#[test]
fn sessions_completion_gates_nested_table_has_commands_vec_of_struct() {
    let schema = schema_for_config();
    let sessions_table = schema.iter().find(|t| t.name == "sessions").unwrap();
    let gates_field = sessions_table
        .fields
        .iter()
        .find(|f| f.key == "completion_gates")
        .expect("sessions schema must include completion_gates");
    let FieldKind::NestedTable(inner) = gates_field.kind else {
        panic!("completion_gates must be NestedTable");
    };
    let commands_field = inner
        .iter()
        .find(|f| f.key == "commands")
        .expect("completion_gates must contain a commands field");
    let FieldKind::VecOfStruct { entry_fields } = commands_field.kind else {
        panic!("commands must be VecOfStruct");
    };
    assert_eq!(entry_fields.len(), 3);
    let keys: Vec<&str> = entry_fields.iter().map(|f| f.key).collect();
    assert_eq!(keys, &["name", "run", "required"]);
}

// ---------------------------------------------------------------------------
// Section coverage backfill (#788): every table below now has every field
// documented in `docs/configuration.md`. These tests lock the field set so
// future drift triggers a compile/runtime failure rather than silently
// re-adding entries to `SCHEMA_BACKFILL_PENDING`.
// ---------------------------------------------------------------------------

fn keys_of(table_name: &str) -> Vec<&'static str> {
    schema_for_config()
        .iter()
        .find(|t| t.name == table_name)
        .unwrap_or_else(|| panic!("table {table_name} not in schema_for_config()"))
        .fields
        .iter()
        .map(|f| f.key)
        .collect()
}

#[test]
fn project_table_has_init_detection_fields() {
    let keys = keys_of("project");
    for required in [
        "repo",
        "base_branch",
        "language",
        "languages",
        "build_command",
        "test_command",
        "run_command",
    ] {
        assert!(
            keys.contains(&required),
            "PROJECT_FIELDS missing `{required}` — found {keys:?}"
        );
    }
}

#[test]
fn sessions_table_has_top_level_runtime_fields() {
    let keys = keys_of("sessions");
    for required in ["allowed_tools", "max_prompt_history", "guardrail_prompt"] {
        assert!(
            keys.contains(&required),
            "SESSIONS_FIELDS missing `{required}` — found {keys:?}"
        );
    }
}

#[test]
fn review_table_has_reviewers_vec_of_struct() {
    let review_table = schema_for_config()
        .iter()
        .find(|t| t.name == "review")
        .expect("review table must be registered");
    let reviewers = review_table
        .fields
        .iter()
        .find(|f| f.key == "reviewers")
        .expect("review.reviewers must be present");
    let FieldKind::VecOfStruct { entry_fields } = reviewers.kind else {
        panic!(
            "review.reviewers must be VecOfStruct, got {:?}",
            reviewers.kind
        );
    };
    let entry_keys: Vec<&str> = entry_fields.iter().map(|f| f.key).collect();
    assert_eq!(entry_keys, &["name", "command", "required"]);
}

#[test]
fn tui_table_has_mascot_fields() {
    let keys = keys_of("tui");
    for required in ["ascii_icons", "show_mascot", "mascot_style"] {
        assert!(
            keys.contains(&required),
            "TUI_FIELDS missing `{required}` — found {keys:?}"
        );
    }
}

#[test]
fn tui_theme_overrides_lists_every_themeoverrides_field() {
    let overrides = schema_for_config()
        .iter()
        .find(|t| t.name == "tui.theme.overrides")
        .expect("tui.theme.overrides top-level table must be registered");
    assert_eq!(
        overrides.fields.len(),
        28,
        "tui.theme.overrides must enumerate all 28 ThemeOverrides color fields"
    );
    let keys: Vec<&str> = overrides.fields.iter().map(|f| f.key).collect();
    for required in [
        "branding_fg",
        "branding_bg",
        "text_primary",
        "text_secondary",
        "text_muted",
        "border_active",
        "border_inactive",
        "border_focused",
        "accent_success",
        "accent_warning",
        "accent_error",
        "accent_info",
        "accent_identifier",
        "gauge_low",
        "gauge_medium",
        "gauge_high",
        "gauge_background",
        "notification_critical",
        "notification_blocker",
        "notification_default",
        "keybind_key",
        "keybind_label_bg",
        "keybind_label_fg",
        "selection_bg",
        "selection_fg",
        "title_accent",
        "fkey_badge_bg",
        "fkey_badge_fg",
    ] {
        assert!(
            keys.contains(&required),
            "tui.theme.overrides missing `{required}` — found {keys:?}"
        );
    }
}

#[test]
fn turboquant_table_has_token_budget_fields() {
    let keys = keys_of("turboquant");
    for required in [
        "fork_handoff_budget",
        "system_prompt_budget",
        "knowledge_budget",
    ] {
        assert!(
            keys.contains(&required),
            "TURBOQUANT_FIELDS missing `{required}` — found {keys:?}"
        );
    }
}

#[test]
fn concurrency_table_has_team_max_parallel() {
    let keys = keys_of("concurrency");
    assert!(
        keys.contains(&"team_max_parallel"),
        "CONCURRENCY_FIELDS missing `team_max_parallel` — found {keys:?}"
    );
}

#[test]
fn agent_kinds_mirror_rust_enum_variants() {
    use crate::config::AgentKind;
    let schema_variants = super::dynamic::AGENT_KINDS;
    let rust_variants: Vec<&'static str> = [
        AgentKind::Claude,
        AgentKind::Codex,
        AgentKind::Qwen,
        AgentKind::Opencode,
        AgentKind::Ollama,
        AgentKind::Minimax,
    ]
    .iter()
    .map(|k| k.as_str())
    .collect();
    for v in &rust_variants {
        assert!(
            schema_variants.contains(v),
            "AGENT_KINDS missing Rust enum variant `{v}`"
        );
    }
    assert_eq!(
        schema_variants.len(),
        rust_variants.len(),
        "AGENT_KINDS must list every AgentKind variant"
    );
}
