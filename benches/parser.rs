use criterion::{Criterion, black_box, criterion_group, criterion_main};
use maestro::session::parser::parse_stream_line;

// ---------------------------------------------------------------------------
// Test data generators
// ---------------------------------------------------------------------------

fn make_text_event(text: &str) -> String {
    format!(r#"{{"type":"assistant","message":{{"type":"text","text":"{text}"}}}}"#)
}

fn make_tool_use_event(tool: &str, file_path: &str) -> String {
    format!(
        r#"{{"type":"assistant","message":{{"type":"tool_use","name":"{tool}","input":{{"file_path":"{file_path}","content":"fn main() {{}}"}}}}}}"#
    )
}

fn make_tool_result_event(tool: &str, is_error: bool) -> String {
    format!(r#"{{"type":"tool_result","tool_name":"{tool}","content":"ok","is_error":{is_error}}}"#)
}

fn make_result_event(cost: f64) -> String {
    format!(r#"{{"type":"result","cost_usd":{cost},"duration_ms":30000,"session_id":"abc-123"}}"#)
}

fn make_error_event(msg: &str) -> String {
    format!(r#"{{"type":"error","error":{{"message":"{msg}"}}}}"#)
}

fn make_system_context_event(pct: f64) -> String {
    format!(r#"{{"type":"system","context_pct":{pct}}}"#)
}

fn make_unknown_event() -> String {
    r#"{"type":"delta","index":0,"content_block":{"text":"x"}}"#.to_string()
}

/// Generates a realistic mixed transcript simulating a Claude Code session.
///
/// Distribution mirrors real usage patterns:
///   - 50% assistant text messages (streaming chunks)
///   - 20% tool use (Read, Write, Edit, Bash)
///   - 15% tool results
///   -  5% system/context events
///   -  5% unknown/delta events
///   -  5% error + result events
fn generate_realistic_transcript(count: usize) -> Vec<String> {
    let mut lines = Vec::with_capacity(count);
    for i in 0..count {
        let line = match i % 20 {
            0..10 => make_text_event(&format!("Analyzing the code structure in module {i}...")),
            10 | 11 => make_tool_use_event("Read", &format!("/src/module_{i}.rs")),
            12 => make_tool_use_event("Write", &format!("/src/new_{i}.rs")),
            13 => make_tool_use_event("Bash", "/bin/cargo"),
            14..17 => make_tool_result_event("Read", false),
            17 => make_system_context_event(45.0 + (i as f64 * 0.1)),
            18 => make_unknown_event(),
            19 => {
                if i % 40 == 19 {
                    make_error_event("rate limited")
                } else {
                    make_result_event(0.05)
                }
            }
            _ => unreachable!(),
        };
        lines.push(line);
    }
    lines
}

/// Generates N homogeneous text events for pure throughput testing.
fn generate_homogeneous_events(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| make_text_event(&format!("Processing item {i} of the analysis pipeline")))
        .collect()
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_throughput_10k(c: &mut Criterion) {
    let events = generate_homogeneous_events(10_000);

    c.bench_function("parse_10k_text_events", |b| {
        b.iter(|| {
            for line in &events {
                black_box(parse_stream_line(black_box(line)));
            }
        });
    });
}

fn bench_realistic_transcript(c: &mut Criterion) {
    let transcript = generate_realistic_transcript(10_000);

    c.bench_function("parse_10k_mixed_transcript", |b| {
        b.iter(|| {
            for line in &transcript {
                black_box(parse_stream_line(black_box(line)));
            }
        });
    });
}

fn bench_individual_event_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_by_event_type");

    let text = make_text_event("Hello world, I am analyzing your codebase");
    let tool_use = make_tool_use_event("Read", "/src/main.rs");
    let tool_result = make_tool_result_event("Read", false);
    let result = make_result_event(1.5);
    let error = make_error_event("rate limited");
    let system = make_system_context_event(68.4);
    let unknown = make_unknown_event();
    let empty = String::new();
    let garbage = "not json at all".to_string();

    group.bench_function("text_message", |b| {
        b.iter(|| black_box(parse_stream_line(black_box(&text))));
    });
    group.bench_function("tool_use", |b| {
        b.iter(|| black_box(parse_stream_line(black_box(&tool_use))));
    });
    group.bench_function("tool_result", |b| {
        b.iter(|| black_box(parse_stream_line(black_box(&tool_result))));
    });
    group.bench_function("result_completed", |b| {
        b.iter(|| black_box(parse_stream_line(black_box(&result))));
    });
    group.bench_function("error", |b| {
        b.iter(|| black_box(parse_stream_line(black_box(&error))));
    });
    group.bench_function("system_context", |b| {
        b.iter(|| black_box(parse_stream_line(black_box(&system))));
    });
    group.bench_function("unknown", |b| {
        b.iter(|| black_box(parse_stream_line(black_box(&unknown))));
    });
    group.bench_function("empty_line", |b| {
        b.iter(|| black_box(parse_stream_line(black_box(&empty))));
    });
    group.bench_function("garbage_non_json", |b| {
        b.iter(|| black_box(parse_stream_line(black_box(&garbage))));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_throughput_10k,
    bench_realistic_transcript,
    bench_individual_event_types
);
criterion_main!(benches);
