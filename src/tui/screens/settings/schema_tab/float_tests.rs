//! Float-precision arm tests for the schema-driven settings renderer (#785).
//!
//! Split out of `tests.rs` to keep both files under the 400-LOC cap. Covers
//! the `display_scale > 1` path: `from_schema` widget construction,
//! `apply_to_toml` writeback, and the extracted `float_stepper` helper.

use super::build::{float_stepper, from_schema};
use super::sync::apply_to_toml;
use super::test_fixture::{SYNTH_TABLE, default_config};
use crate::config::schema::{DefaultValue, FieldKind, FieldSchema, TableSchema};
use crate::tui::widgets::WidgetKind;

fn empty_toml_table() -> toml::Value {
    toml::Value::Table(toml::map::Map::new())
}

const SCALE_FIELDS: &[FieldSchema] = &[FieldSchema {
    key: "budget_f",
    label: "Budget F",
    help: "test float with scale 10",
    default: DefaultValue::Float(5.5),
    kind: FieldKind::Float {
        min: 0.1,
        max: 100.0,
        step: 0.5,
        display_scale: 10,
    },
    validator: None,
}];

const SCALE_TABLE: TableSchema = TableSchema {
    name: "synth_f",
    label: "SynthF",
    fields: SCALE_FIELDS,
};

#[test]
fn build_float_scale_10_value_5point5_maps_to_55() {
    let fields = from_schema(&SCALE_TABLE, &default_config());
    let WidgetKind::NumberStepper(s) = &fields[0].widget else {
        panic!("expected NumberStepper");
    };
    assert_eq!(s.value, 55);
    assert_eq!(s.min, 1);
    assert_eq!(s.max, 1000);
    assert_eq!(s.step, 5);
    assert_eq!(s.display_value(), "5.5");
}

#[test]
fn build_float_scale_10_renders_default_with_one_decimal() {
    let fields = from_schema(&SCALE_TABLE, &default_config());
    let WidgetKind::NumberStepper(s) = &fields[0].widget else {
        panic!("expected NumberStepper");
    };
    assert_eq!(s.display_value(), "5.5");
}

#[test]
fn build_float_scale_1_behaves_as_legacy_integer_rounding() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let WidgetKind::NumberStepper(s) = &fields[2].widget else {
        panic!("expected NumberStepper");
    };
    assert_eq!(s.value, 2);
    assert_eq!(s.display_value(), "2");
}

#[test]
fn apply_to_toml_float_with_scale_10_divides_correctly() {
    let mut fields = from_schema(&SCALE_TABLE, &default_config());
    if let WidgetKind::NumberStepper(ref mut s) = fields[0].widget {
        s.value = 75;
    }
    let mut root = empty_toml_table();
    apply_to_toml(&SCALE_TABLE, &fields, &mut root).unwrap();
    let synth_f = root.get("synth_f").and_then(|v| v.as_table()).unwrap();
    assert_eq!(synth_f.get("budget_f"), Some(&toml::Value::Float(7.5)));
}

#[test]
fn apply_to_toml_float_with_scale_10_round_trips_12_point_5() {
    let mut fields = from_schema(&SCALE_TABLE, &default_config());
    if let WidgetKind::NumberStepper(ref mut s) = fields[0].widget {
        s.value = 125;
    }
    let mut root = empty_toml_table();
    apply_to_toml(&SCALE_TABLE, &fields, &mut root).unwrap();
    let synth_f = root.get("synth_f").and_then(|v| v.as_table()).unwrap();
    assert_eq!(synth_f.get("budget_f"), Some(&toml::Value::Float(12.5)));
}

#[test]
fn float_stepper_scale_10_rounds_to_nearest_tick() {
    let s = float_stepper("x".into(), 5.5, 0.1, 100.0, 0.5, 10);
    assert_eq!(s.value, 55);
    assert_eq!(s.min, 1);
    assert_eq!(s.max, 1000);
    assert_eq!(s.step, 5);
    assert_eq!(s.display_value(), "5.5");
}

#[test]
fn float_stepper_clamps_value_into_min_max() {
    let above = float_stepper("hi".into(), 999.0, 0.0, 10.0, 1.0, 1);
    assert_eq!(above.value, above.max);
    let below = float_stepper("lo".into(), -1.0, 0.0, 10.0, 1.0, 1);
    assert_eq!(below.value, below.min);
}

#[test]
fn float_stepper_step_floors_at_one_to_avoid_freeze() {
    let s = float_stepper("frozen".into(), 5.0, 0.0, 10.0, 0.1, 1);
    assert_eq!(s.step, 1);
}
