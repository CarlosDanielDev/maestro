//! Test-only `FieldSchema` fixtures used by the dynamic widget tests.

#![cfg(test)]

use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};

pub(crate) const TEST_AGENT_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "kind",
        label: "Kind",
        help: "Agent kind",
        default: DefaultValue::Str("implementer"),
        kind: FieldKind::Enum(&["implementer", "reviewer", "docs", "devops"]),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "enabled",
        label: "Enabled",
        help: "Whether this agent is active",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "model",
        label: "Model",
        help: "Model override",
        default: DefaultValue::Str("claude-opus"),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
];

pub(crate) const TEST_COMMAND_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "enabled",
        label: "Enabled",
        help: "Enable this command step",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "gate",
        label: "Gate",
        help: "Completion gate name",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "command",
        label: "Command",
        help: "Shell command",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "timeout",
        label: "Timeout",
        help: "Seconds before timeout",
        default: DefaultValue::Int(30),
        kind: FieldKind::Int {
            min: 1,
            max: 3600,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
];
