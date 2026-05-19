//! Field arrays for the remaining tables: gates, review, tui, turboquant,
//! concurrency, monitoring.

use super::{DefaultValue, FieldKind, FieldSchema, validate_non_empty};

const THEME_PRESETS: &[&str] = &["dark", "light", "retro"];
const LAYOUT_MODES: &[&str] = &["vertical", "horizontal"];
const LAYOUT_DENSITIES: &[&str] = &["default", "comfortable", "compact"];
const QUANT_STRATEGIES: &[&str] = &["turboquant", "polarquant", "qjl"];
const QUANT_APPLY: &[&str] = &["keys", "values", "both"];

pub(super) const GATES_CI_AUTO_FIX_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "enabled",
        label: "Auto-Fix Enabled",
        help: "Spawn a fix session when CI fails on an open PR",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "max_retries",
        label: "Max Fix Attempts",
        help: "How many auto-fix passes to run per PR",
        default: DefaultValue::Int(3),
        kind: FieldKind::Int {
            min: 0,
            max: 10,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
];

pub(super) const GATES_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "enabled",
        label: "Gates Enabled",
        help: "Run completion gates before creating PRs",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "test_command",
        label: "Test Command",
        help: "Command used as the default test gate",
        default: DefaultValue::Str("cargo test"),
        kind: FieldKind::String,
        validator: Some(validate_non_empty),
        presentation: None,
    },
    FieldSchema {
        key: "ci_poll_interval_secs",
        label: "CI Poll Interval (sec)",
        help: "Seconds between CI status polls",
        default: DefaultValue::Int(30),
        kind: FieldKind::Int {
            min: 5,
            max: 300,
            step: 5,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "ci_max_wait_secs",
        label: "CI Max Wait (sec)",
        help: "Maximum seconds to wait for CI to finish",
        default: DefaultValue::Int(1800),
        kind: FieldKind::Int {
            min: 60,
            max: 7200,
            step: 60,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "ci_auto_fix",
        label: "CI Auto-Fix",
        help: "Auto-fix passes when CI fails on an open PR",
        default: DefaultValue::Nested,
        kind: FieldKind::NestedTable(GATES_CI_AUTO_FIX_FIELDS),
        validator: None,
        presentation: None,
    },
];

pub(super) const REVIEW_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "enabled",
        label: "Review Dispatch",
        help: "Dispatch the review command on PR creation",
        default: DefaultValue::Bool(false),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "command",
        label: "Review Command",
        help: "Template — `{pr_number}` and `{branch}` are substituted at dispatch",
        default: DefaultValue::Str(
            "gh pr review {pr_number} --comment --body 'Automated review by Maestro'",
        ),
        kind: FieldKind::String,
        validator: Some(validate_non_empty),
        presentation: None,
    },
];

pub(super) const TUI_FIELDS: &[FieldSchema] = &[FieldSchema {
    key: "ascii_icons",
    label: "ASCII Icons",
    help: "Use ASCII-only icons in terminals without emoji support",
    default: DefaultValue::Bool(false),
    kind: FieldKind::Bool,
    validator: None,
    presentation: None,
}];

pub(super) const TUI_THEME_FIELDS: &[FieldSchema] = &[FieldSchema {
    key: "preset",
    label: "Theme Preset",
    help: "Color preset applied to the TUI",
    default: DefaultValue::Str("dark"),
    kind: FieldKind::Enum(THEME_PRESETS),
    validator: None,
    presentation: None,
}];

pub(super) const TUI_LAYOUT_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "mode",
        label: "Layout Mode",
        help: "Stack the preview panel below the list or beside it",
        default: DefaultValue::Str("vertical"),
        kind: FieldKind::Enum(LAYOUT_MODES),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "density",
        label: "Density",
        help: "Information density across all list views",
        default: DefaultValue::Str("default"),
        kind: FieldKind::Enum(LAYOUT_DENSITIES),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "preview_ratio",
        label: "Preview Ratio (%)",
        help: "Width or height percentage allocated to the preview panel",
        default: DefaultValue::Int(50),
        kind: FieldKind::Int {
            min: 10,
            max: 90,
            step: 5,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "activity_log_height",
        label: "Activity Log Height (%)",
        help: "Percentage of screen height for the activity log",
        default: DefaultValue::Int(25),
        kind: FieldKind::Int {
            min: 10,
            max: 50,
            step: 5,
        },
        validator: None,
        presentation: None,
    },
];

pub(super) const TURBOQUANT_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "enabled",
        label: "TurboQuant Enabled",
        help: "Compress contexts before forking or compacting",
        default: DefaultValue::Bool(false),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "bit_width",
        label: "Bit Width",
        help: "Bits per coefficient used by the quantizer",
        default: DefaultValue::Int(4),
        kind: FieldKind::Int {
            min: 1,
            max: 8,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "strategy",
        label: "Strategy",
        help: "Quantization algorithm",
        default: DefaultValue::Str("turboquant"),
        kind: FieldKind::Enum(QUANT_STRATEGIES),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "apply_to",
        label: "Apply To",
        help: "Vector components to compress",
        default: DefaultValue::Str("both"),
        kind: FieldKind::Enum(QUANT_APPLY),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "auto_on_overflow",
        label: "Auto on Overflow",
        help: "Enable compression automatically when context overflows",
        default: DefaultValue::Bool(false),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
];

pub(super) const CONCURRENCY_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "heavy_task_limit",
        label: "Heavy Task Limit",
        help: "Maximum simultaneous heavy tasks",
        default: DefaultValue::Int(2),
        kind: FieldKind::Int {
            min: 1,
            max: 10,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "heavy_task_labels",
        label: "Heavy Task Labels",
        help: "Labels that mark a task as resource-intensive",
        default: DefaultValue::StrList(&[]),
        kind: FieldKind::StringList,
        validator: None,
        presentation: None,
    },
];

pub(super) const MONITORING_FIELDS: &[FieldSchema] = &[FieldSchema {
    key: "work_tick_interval_secs",
    label: "Work Tick Interval (sec)",
    help: "How often the work assigner ticks",
    default: DefaultValue::Int(10),
    kind: FieldKind::Int {
        min: 1,
        max: 120,
        step: 5,
    },
    validator: None,
    presentation: None,
}];
