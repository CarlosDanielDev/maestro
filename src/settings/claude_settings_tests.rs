//! Inline-attached test module for `claude_settings.rs`. Kept as a sibling
//! file (via `#[path = ...]`) so the parent stays under the project's
//! 400-line file-size limit while still having access to `pub(crate)` items.

use super::claude_settings::*;
use serde_json::Value;

fn parse(raw: &str) -> CavemanModeState {
    parse_caveman_mode_from_str(raw)
}

#[test]
fn explicit_true_from_json() {
    assert!(matches!(
        parse(r#"{"behavior":{"caveman_mode":true}}"#),
        CavemanModeState::ExplicitTrue
    ));
}

#[test]
fn explicit_false_from_json() {
    assert!(matches!(
        parse(r#"{"behavior":{"caveman_mode":false}}"#),
        CavemanModeState::ExplicitFalse
    ));
}

#[test]
fn default_when_caveman_mode_key_absent() {
    assert!(matches!(
        parse(r#"{"behavior":{"other_flag":true}}"#),
        CavemanModeState::Default
    ));
}

#[test]
fn default_when_behavior_block_absent() {
    assert!(matches!(
        parse(r#"{"mcpServers":{}}"#),
        CavemanModeState::Default
    ));
}

#[test]
fn default_when_file_is_empty_object() {
    assert!(matches!(parse("{}"), CavemanModeState::Default));
}

#[test]
fn error_when_behavior_is_null() {
    assert!(matches!(
        parse(r#"{"behavior":null}"#),
        CavemanModeState::Error(_)
    ));
}

#[test]
fn error_when_behavior_is_string() {
    assert!(matches!(
        parse(r#"{"behavior":"yes"}"#),
        CavemanModeState::Error(_)
    ));
}

#[test]
fn error_when_behavior_is_array() {
    assert!(matches!(
        parse(r#"{"behavior":[1,2,3]}"#),
        CavemanModeState::Error(_)
    ));
}

#[test]
fn error_when_caveman_mode_is_non_boolean() {
    assert!(matches!(
        parse(r#"{"behavior":{"caveman_mode":"yes"}}"#),
        CavemanModeState::Error(_)
    ));
}

#[test]
fn error_when_input_is_not_valid_json() {
    assert!(matches!(parse("{ not json }"), CavemanModeState::Error(_)));
}

#[test]
fn next_value_explicit_true_returns_false() {
    assert_eq!(CavemanModeState::ExplicitTrue.next_value(), Some(false));
}

#[test]
fn next_value_explicit_false_returns_true() {
    assert_eq!(CavemanModeState::ExplicitFalse.next_value(), Some(true));
}

#[test]
fn next_value_default_returns_true() {
    assert_eq!(CavemanModeState::Default.next_value(), Some(true));
}

#[test]
fn next_value_error_returns_none() {
    assert_eq!(CavemanModeState::Error("oops".into()).next_value(), None);
}

#[test]
fn label_explicit_true_is_true_string() {
    assert_eq!(CavemanModeState::ExplicitTrue.label(), "true");
}

#[test]
fn label_explicit_false_is_false_string() {
    assert_eq!(CavemanModeState::ExplicitFalse.label(), "false");
}

#[test]
fn label_default_contains_default_annotation() {
    let label = CavemanModeState::Default.label();
    assert!(label.contains("(default)"), "got {label}");
    assert_ne!(label, CavemanModeState::ExplicitFalse.label());
}

#[test]
fn label_error_contains_error_marker() {
    let label = CavemanModeState::Error("disk failure".into()).label();
    assert!(label.contains("error"), "got {label}");
    assert!(label.contains("disk failure"), "got {label}");
}

#[test]
fn is_toggleable_true_for_non_error_states() {
    assert!(CavemanModeState::ExplicitTrue.is_toggleable());
    assert!(CavemanModeState::ExplicitFalse.is_toggleable());
    assert!(CavemanModeState::Default.is_toggleable());
}

#[test]
fn is_toggleable_false_for_error() {
    assert!(!CavemanModeState::Error("x".into()).is_toggleable());
}

// ----- Pure mutation core -----

#[test]
fn apply_toggle_when_existing_is_none_creates_minimal_object() {
    let result = apply_caveman_toggle(None, true).expect("apply ok");
    let value = Value::Object(result);
    assert_eq!(value, serde_json::json!({"behavior":{"caveman_mode":true}}));
}

#[test]
fn apply_toggle_when_behavior_absent_adds_only_behavior() {
    let existing = serde_json::json!({"mcpServers": {}});
    let result = apply_caveman_toggle(Some(&existing), true).expect("apply ok");
    let value = Value::Object(result);
    assert_eq!(value["behavior"]["caveman_mode"], serde_json::json!(true));
    assert_eq!(value["mcpServers"], serde_json::json!({}));
    assert_eq!(value.as_object().unwrap().len(), 2);
}

#[test]
fn apply_toggle_preserves_other_behavior_keys() {
    let existing = serde_json::json!({"behavior": {"other_flag": true}});
    let result = apply_caveman_toggle(Some(&existing), true).expect("apply ok");
    let value = Value::Object(result);
    assert_eq!(value["behavior"]["other_flag"], serde_json::json!(true));
    assert_eq!(value["behavior"]["caveman_mode"], serde_json::json!(true));
}

#[test]
fn apply_toggle_preserves_unknown_top_level_keys() {
    let existing = serde_json::json!({
        "mcpServers": {"my-server": {"command": "npx"}},
        "env": {"MY_VAR": "1"},
        "alwaysThinkingEnabled": true,
        "hooks": {"postToolUse": []},
        "behavior": {"caveman_mode": false}
    });
    let result = apply_caveman_toggle(Some(&existing), true).expect("apply ok");
    let value = Value::Object(result);
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
fn apply_toggle_rejects_non_object_behavior() {
    let existing = serde_json::json!({"behavior": null});
    let result = apply_caveman_toggle(Some(&existing), true);
    assert!(matches!(result, Err(CavemanWriteError::Serialise(_))));
}

// ----- MockSettingsStore for App-level wiring tests -----

use std::sync::Mutex;

pub(crate) struct MockSettingsStore {
    pub load_result: CavemanModeState,
    pub save_calls: Mutex<Vec<bool>>,
    pub save_result: Mutex<Result<(), CavemanWriteError>>,
}

impl MockSettingsStore {
    pub fn new(load: CavemanModeState) -> Self {
        Self {
            load_result: load,
            save_calls: Mutex::new(Vec::new()),
            save_result: Mutex::new(Ok(())),
        }
    }

    pub fn fail_writes_with(self, err: CavemanWriteError) -> Self {
        *self.save_result.lock().unwrap() = Err(err);
        self
    }

    pub fn save_calls(&self) -> Vec<bool> {
        self.save_calls.lock().unwrap().clone()
    }
}

impl SettingsStore for MockSettingsStore {
    fn load_caveman_mode(&self) -> CavemanModeState {
        self.load_result.clone()
    }

    fn save_caveman_mode(&self, new_value: bool) -> Result<(), CavemanWriteError> {
        self.save_calls.lock().unwrap().push(new_value);
        self.save_result.lock().unwrap().clone()
    }
}

#[test]
fn mock_store_load_returns_configured_state() {
    let store = MockSettingsStore::new(CavemanModeState::ExplicitFalse);
    assert!(matches!(
        store.load_caveman_mode(),
        CavemanModeState::ExplicitFalse
    ));
}

#[test]
fn mock_store_save_records_calls() {
    let store = MockSettingsStore::new(CavemanModeState::Default);
    store.save_caveman_mode(true).expect("save ok");
    store.save_caveman_mode(false).expect("save ok");
    assert_eq!(store.save_calls(), vec![true, false]);
}

#[test]
fn mock_store_save_can_be_configured_to_fail() {
    let store = MockSettingsStore::new(CavemanModeState::Default)
        .fail_writes_with(CavemanWriteError::Io("disk full".into()));
    let err = store.save_caveman_mode(true).unwrap_err();
    assert!(matches!(err, CavemanWriteError::Io(_)));
}
