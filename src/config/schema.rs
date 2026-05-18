//! Transport-neutral schema describing every field in `Config`.
//!
//! The schema is the single source of truth consumed by the TUI renderer,
//! tab migrations, and the auto-generated configuration reference. The
//! types in this module are intentionally simple data records — no runtime
//! state, no closures — so the entire registry lives in `const` arrays.

#[allow(dead_code)]
mod core;
#[allow(dead_code)]
mod extras;

/// Default value for a [`FieldSchema`]. Mirrors [`FieldKind`] so the entire
/// schema can live in a `const` (no heap, no `toml::Value`, no `String`).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(&'static str),
    StrList(&'static [&'static str]),
    /// Marker for `NestedTable` — sub-table defaults live on inner fields.
    Nested,
}

/// Kind of a configuration field, used by downstream renderers/generators
/// to pick a widget and enforce a domain.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum FieldKind {
    Bool,
    Int {
        min: i64,
        max: i64,
        step: i64,
    },
    Float {
        min: f64,
        max: f64,
        step: f64,
    },
    String,
    Enum(&'static [&'static str]),
    StringList,
    /// Reserved for future use. The registry currently models nested tables
    /// (e.g. `sessions.hollow_retry`) as top-level `TableSchema` entries with
    /// dotted `name`s, so every consumer iterates the flat slice without
    /// recursion. Kept here so downstream tickets can opt into recursive
    /// rendering without a schema-shape change.
    NestedTable(&'static [FieldSchema]),
}

/// Pure validator. Function pointer (not closure) so [`FieldSchema`] stays
/// `Copy` and const-promotable. Returns `Err(message)` on failure.
pub(crate) type Validator = fn(&toml::Value) -> Result<(), String>;

/// Schema for one leaf field (or nested sub-table) in `Config`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct FieldSchema {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub default: DefaultValue,
    pub kind: FieldKind,
    pub validator: Option<Validator>,
}

/// Schema for one TOML table. Nested tables (e.g. `tui.theme`) use dotted
/// `name`s — every consumer iterates the flat slice without recursion.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct TableSchema {
    pub name: &'static str,
    pub label: &'static str,
    pub fields: &'static [FieldSchema],
}

#[allow(dead_code)]
pub(crate) fn validate_url_or_empty(value: &toml::Value) -> Result<(), String> {
    match value.as_str() {
        Some("") => Ok(()),
        Some(s) if s.starts_with("https://") && s.len() > "https://".len() => Ok(()),
        Some(s) if s.starts_with("http://") && s.len() > "http://".len() => Ok(()),
        Some(_) => Err("must be empty or start with http:// / https://".into()),
        None => Err("expected a string".into()),
    }
}

#[allow(dead_code)]
pub(crate) fn validate_non_empty(value: &toml::Value) -> Result<(), String> {
    match value.as_str() {
        Some(s) if !s.trim().is_empty() => Ok(()),
        Some(_) => Err("must not be empty".into()),
        None => Err("expected a string".into()),
    }
}

pub(crate) const PROJECT_TABLE: TableSchema = TableSchema {
    name: "project",
    label: "Project",
    fields: core::PROJECT_FIELDS,
};

#[allow(dead_code)]
const SCHEMA: &[TableSchema] = &[
    PROJECT_TABLE,
    TableSchema {
        name: "sessions",
        label: "Sessions",
        fields: core::SESSIONS_FIELDS,
    },
    TableSchema {
        name: "budget",
        label: "Budget",
        fields: core::BUDGET_FIELDS,
    },
    TableSchema {
        name: "github",
        label: "GitHub",
        fields: core::GITHUB_FIELDS,
    },
    TableSchema {
        name: "notifications",
        label: "Notifications",
        fields: core::NOTIFICATIONS_FIELDS,
    },
    TableSchema {
        name: "gates",
        label: "Gates",
        fields: extras::GATES_FIELDS,
    },
    TableSchema {
        name: "review",
        label: "Review",
        fields: extras::REVIEW_FIELDS,
    },
    TableSchema {
        name: "tui",
        label: "TUI",
        fields: extras::TUI_FIELDS,
    },
    TableSchema {
        name: "tui.theme",
        label: "Theme",
        fields: extras::TUI_THEME_FIELDS,
    },
    TableSchema {
        name: "tui.layout",
        label: "Layout",
        fields: extras::TUI_LAYOUT_FIELDS,
    },
    TableSchema {
        name: "turboquant",
        label: "TurboQuant",
        fields: extras::TURBOQUANT_FIELDS,
    },
    TableSchema {
        name: "concurrency",
        label: "Concurrency",
        fields: extras::CONCURRENCY_FIELDS,
    },
    TableSchema {
        name: "monitoring",
        label: "Monitoring",
        fields: extras::MONITORING_FIELDS,
    },
];

#[allow(dead_code)]
pub(crate) const fn schema_for_config() -> &'static [TableSchema] {
    SCHEMA
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    const MINIMAL_TOML: &str = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\nslack_webhook_url = \"\"\n";

    const EXPECTED_NON_NESTED_FIELDS: usize = 52;

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
        s.as_bytes().windows(3).any(|w| {
            matches!(w[0], b'.' | b'!' | b'?') && w[1] == b' ' && w[2].is_ascii_uppercase()
        })
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
    ) {
        for field in fields {
            if let FieldKind::NestedTable(inner) = field.kind {
                let next_prefix = format!("{prefix}.{}", field.key);
                walk_table_paths(toml_val, &next_prefix, inner, walked);
                continue;
            }
            let path = format!("{prefix}.{}", field.key);
            assert!(
                resolve(toml_val, &path).is_some(),
                "schema path not found in serialized Config: {path}"
            );
            *walked += 1;
        }
    }

    #[test]
    fn schema_all_dotted_paths_resolve_on_default_config() {
        let config = default_config();
        let toml_val = config_as_toml_value(&config);

        let mut walked = 0usize;
        for table in schema_for_config() {
            walk_table_paths(&toml_val, table.name, table.fields, &mut walked);
        }
        assert_eq!(
            walked, EXPECTED_NON_NESTED_FIELDS,
            "expected {EXPECTED_NON_NESTED_FIELDS} non-nested schema fields, walked {walked}"
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
                let actual = node.as_str().unwrap_or_else(|| {
                    panic!("enum field {path} serializes as non-string: {node:?}")
                });
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
                        | (DefaultValue::Str(_), FieldKind::String | FieldKind::Enum(_),)
                        | (DefaultValue::StrList(_), FieldKind::StringList)
                        | (DefaultValue::Nested, FieldKind::NestedTable(_)),
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
}
