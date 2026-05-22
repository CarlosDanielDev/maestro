//! Pricing-fallback tests for the OpenCode JSON parser, split from
//! `tests.rs` to keep the file under the 400-line cap.

use super::*;
use crate::session::types::StreamEvent;

#[test]
fn step_finish_with_zero_cost_falls_back_to_pricing_for_known_model() {
    // 1000 input + 100 output on sonnet at 3.00/15.00 → 0.003 + 0.0015 = 0.0045.
    let mut parser = OpenCodeJsonParser::with_model("anthropic/claude-sonnet-4-5");
    let events = parser.parse_line(
        r#"{"type":"step_finish","part":{"type":"step-finish","reason":"stop","tokens":{"input":1000,"output":100,"cache":{"read":0,"write":0}},"cost":0}}"#,
    );

    let cost = events
        .iter()
        .find_map(|event| match event {
            StreamEvent::CostUpdate { cost_usd } => Some(*cost_usd),
            _ => None,
        })
        .expect("step_finish with cost=0 should emit CostUpdate via pricing fallback");
    assert!((cost - 0.0045).abs() < 1e-9, "expected 0.0045, got {cost}");

    let completed_cost = events
        .iter()
        .find_map(|event| match event {
            StreamEvent::Completed { cost_usd } => Some(*cost_usd),
            _ => None,
        })
        .expect("reason=stop should emit Completed");
    assert!((completed_cost - 0.0045).abs() < 1e-9);
}

#[test]
fn step_finish_uses_telemetry_cost_when_positive() {
    // Telemetry already reports cost > 0 → fallback must NOT override it.
    let mut parser = OpenCodeJsonParser::with_model("anthropic/claude-sonnet-4-5");
    let events = parser.parse_line(
        r#"{"type":"step_finish","part":{"type":"step-finish","reason":"stop","tokens":{"input":1000,"output":100},"cost":0.42}}"#,
    );

    let completed_cost = events
        .iter()
        .find_map(|event| match event {
            StreamEvent::Completed { cost_usd } => Some(*cost_usd),
            _ => None,
        })
        .expect("Completed should be emitted");
    assert!((completed_cost - 0.42).abs() < 1e-9, "telemetry cost wins");
}

#[test]
fn step_finish_with_unknown_model_keeps_zero_cost() {
    // default() → model empty → pricing returns 0 → no CostUpdate emitted.
    let mut parser = OpenCodeJsonParser::default();
    let events = parser.parse_line(
        r#"{"type":"step_finish","part":{"type":"step-finish","reason":"stop","tokens":{"input":1000,"output":100},"cost":0}}"#,
    );

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, StreamEvent::CostUpdate { .. })),
        "unknown model + zero telemetry → no CostUpdate"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        StreamEvent::Completed { cost_usd } if *cost_usd == 0.0
    )));
}

#[test]
fn step_finish_tool_calls_emits_cost_update_but_no_completed() {
    // Non-terminal step (reason=tool-calls) still emits CostUpdate when tokens
    // produce a positive computed cost, but must NOT emit Completed.
    let mut parser = OpenCodeJsonParser::with_model("anthropic/claude-sonnet-4-5");
    let events = parser.parse_line(
        r#"{"type":"step_finish","part":{"type":"step-finish","reason":"tool-calls","tokens":{"input":1000,"output":100}}}"#,
    );

    assert!(
        events
            .iter()
            .any(|event| matches!(event, StreamEvent::CostUpdate { .. })),
        "tool-calls step with tokens should emit CostUpdate"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, StreamEvent::Completed { .. })),
        "tool-calls step must NOT emit Completed"
    );
}
