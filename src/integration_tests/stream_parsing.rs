use crate::integration_tests::helpers::*;
use crate::session::manager::ManagedSession;
use crate::session::parser::parse_stream_line;
use crate::session::types::{SessionStatus, StreamEvent};

// ---------------------------------------------------------------------------
// Direct handle_event tests
// ---------------------------------------------------------------------------

#[test]
fn assistant_message_updates_last_message_and_activity() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::AssistantMessage {
        text: "Hello from Claude".to_string(),
    });

    assert_eq!(managed.session.last_message, "Hello from Claude");
    assert_eq!(managed.session.current_activity, "Hello from Claude");
}

#[test]
fn assistant_message_appends_with_newline() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::AssistantMessage {
        text: "Line 1".to_string(),
    });
    managed.handle_event(&StreamEvent::AssistantMessage {
        text: "Line 2".to_string(),
    });

    assert!(managed.session.last_message.contains("Line 1"));
    assert!(managed.session.last_message.contains("Line 2"));
    assert!(managed.session.last_message.contains('\n'));
}

#[test]
fn assistant_message_empty_text_is_ignored() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::AssistantMessage {
        text: String::new(),
    });

    assert!(managed.session.last_message.is_empty());
}

#[test]
fn tool_use_sets_activity_and_logs() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "Read".to_string(),

        file_path: Some("src/main.rs".to_string()),
        command_preview: None,
    });

    assert_eq!(managed.session.current_activity, "Read: main.rs");
    assert!(
        managed
            .session
            .activity_log
            .iter()
            .any(|e| e.message.contains("Read"))
    );
}

#[test]
fn tool_use_file_touching_tools_track_files() {
    for (tool, path) in &[
        ("Read", "src/config.rs"),
        ("Edit", "src/lib.rs"),
        ("Write", "src/new_module.rs"),
        ("Glob", "src/glob_target.rs"),
        ("Grep", "src/grep_target.rs"),
    ] {
        let mut managed = ManagedSession::new(make_session("s"));
        managed.handle_event(&StreamEvent::ToolUse {
            tool: tool.to_string(),

            file_path: Some(path.to_string()),
            command_preview: None,
        });
        assert!(
            managed.session.files_touched.contains(&path.to_string()),
            "{} tool must track file_path in files_touched",
            tool
        );
    }
}

#[test]
fn tool_use_bash_does_not_track_file_path() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "Bash".to_string(),

        file_path: None,
        command_preview: None,
    });

    assert!(managed.session.files_touched.is_empty());
}

#[test]
fn tool_use_deduplicates_files_touched() {
    let mut managed = ManagedSession::new(make_session("s"));

    for _ in 0..3 {
        managed.handle_event(&StreamEvent::ToolUse {
            tool: "Read".to_string(),

            file_path: Some("src/main.rs".to_string()),
            command_preview: None,
        });
    }

    assert_eq!(
        managed
            .session
            .files_touched
            .iter()
            .filter(|f| *f == "src/main.rs")
            .count(),
        1,
        "each file must appear at most once in files_touched"
    );
}

#[test]
fn tool_result_error_logs_activity() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::ToolResult {
        tool: "Write".to_string(),
        is_error: true,
    });

    assert!(
        managed
            .session
            .activity_log
            .iter()
            .any(|e| e.message.contains("Write") && e.message.to_lowercase().contains("error"))
    );
}

#[test]
fn tool_result_success_logs_done_with_elapsed() {
    let mut managed = ManagedSession::new(make_session("s"));
    let initial_log_len = managed.session.activity_log.len();

    managed.handle_event(&StreamEvent::ToolResult {
        tool: "Read".to_string(),
        is_error: false,
    });

    assert!(
        managed.session.activity_log.len() > initial_log_len,
        "ToolResult success must log a 'done' entry"
    );
}

#[test]
fn cost_update_sets_cost_usd() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::CostUpdate { cost_usd: 0.42 });

    assert!((managed.session.cost_usd - 0.42).abs() < f64::EPSILON);
}

#[test]
fn context_update_sets_context_pct() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::ContextUpdate { context_pct: 0.75 });

    assert!((managed.session.context_pct - 0.75).abs() < f64::EPSILON);
    assert!(
        managed
            .session
            .activity_log
            .iter()
            .any(|e| e.message.contains("75"))
    );
}

#[test]
fn completed_zero_cost_does_not_overwrite_existing_cost() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::CostUpdate { cost_usd: 2.50 });
    managed.handle_event(&StreamEvent::Completed { cost_usd: 0.0 });

    assert_eq!(managed.session.status, SessionStatus::Completed);
    assert!(
        (managed.session.cost_usd - 2.50).abs() < f64::EPSILON,
        "cost must not be overwritten by Completed with cost_usd == 0.0"
    );
}

#[test]
fn unknown_event_is_silent() {
    let mut managed = ManagedSession::new(make_session("s"));
    let initial_log_len = managed.session.activity_log.len();

    managed.handle_event(&StreamEvent::Unknown {
        raw: "junk line".to_string(),
    });

    assert_eq!(managed.session.status, SessionStatus::Queued);
    assert_eq!(managed.session.activity_log.len(), initial_log_len);
}

// ---------------------------------------------------------------------------
// Parser round-trip: parse_stream_line -> handle_event
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_assistant_text_updates_session() {
    let line = r#"{"type":"assistant","message":{"type":"text","text":"I will fix the bug."}}"#;
    let event = parse_stream_line(line);

    let mut managed = ManagedSession::new(make_session("s"));
    managed.handle_event(&event);

    assert_eq!(managed.session.last_message, "I will fix the bug.");
    assert_eq!(managed.session.current_activity, "I will fix the bug.");
}

#[test]
fn roundtrip_tool_use_with_file_path_updates_files_touched() {
    let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Write","input":{"file_path":"src/session/pool.rs"}}}"#;
    let event = parse_stream_line(line);

    let mut managed = ManagedSession::new(make_session("s"));
    managed.handle_event(&event);

    assert!(
        managed
            .session
            .files_touched
            .contains(&"src/session/pool.rs".to_string())
    );
}

#[test]
fn roundtrip_result_event_transitions_to_completed() {
    let line = r#"{"type":"result","cost_usd":2.10,"duration_ms":45000}"#;
    let event = parse_stream_line(line);

    let mut managed = ManagedSession::new(make_session("s"));
    managed.handle_event(&event);

    assert_eq!(managed.session.status, SessionStatus::Completed);
    assert!((managed.session.cost_usd - 2.10).abs() < f64::EPSILON);
}

#[test]
fn roundtrip_error_event_transitions_to_errored() {
    let line = r#"{"type":"error","error":{"message":"context window exceeded"}}"#;
    let event = parse_stream_line(line);

    let mut managed = ManagedSession::new(make_session("s"));
    managed.handle_event(&event);

    assert_eq!(managed.session.status, SessionStatus::Errored);
    assert!(
        managed
            .session
            .activity_log
            .iter()
            .any(|e| e.message.contains("context window exceeded"))
    );
}

#[test]
fn roundtrip_context_update_from_usage_tokens() {
    let line = r#"{"type":"system","usage":{"input_tokens":80000,"max_input_tokens":200000}}"#;
    let event = parse_stream_line(line);

    let mut managed = ManagedSession::new(make_session("s"));
    managed.handle_event(&event);

    assert!(
        (managed.session.context_pct - 0.4).abs() < 0.001,
        "context_pct must be 0.4 for 80k/200k tokens"
    );
}

#[test]
fn roundtrip_full_session_transcript() {
    let lines = [
        r#"{"type":"assistant","message":{"type":"text","text":"Let me read the file."}}"#,
        r#"{"type":"assistant","message":{"type":"tool_use","name":"Read","input":{"file_path":"src/main.rs"}}}"#,
        r#"{"type":"tool_result","tool_name":"Read","is_error":false}"#,
        r#"{"type":"assistant","message":{"type":"text","text":"I see the issue. Fixing now."}}"#,
        r#"{"type":"assistant","message":{"type":"tool_use","name":"Edit","input":{"file_path":"src/main.rs"}}}"#,
        r#"{"type":"tool_result","tool_name":"Edit","is_error":false}"#,
        r#"{"type":"system","usage":{"input_tokens":50000,"max_input_tokens":200000}}"#,
        r#"{"type":"result","cost_usd":1.50,"duration_ms":30000}"#,
    ];

    let mut managed = ManagedSession::new(make_session("s"));
    for line in &lines {
        let event = parse_stream_line(line);
        managed.handle_event(&event);
    }

    assert_eq!(managed.session.status, SessionStatus::Completed);
    assert!((managed.session.cost_usd - 1.50).abs() < f64::EPSILON);
    assert!(
        managed
            .session
            .files_touched
            .contains(&"src/main.rs".to_string())
    );
    assert!(
        (managed.session.context_pct - 0.25).abs() < 0.001,
        "context_pct must be 0.25 for 50k/200k tokens"
    );
    assert!(managed.session.last_message.contains("I see the issue"));
    assert_eq!(managed.session.current_activity, "Done");
}

// ---------------------------------------------------------------------------
// Issue #102: Phase 1 — Richer tool activity messages
// ---------------------------------------------------------------------------

#[test]
fn tool_use_read_with_file_path_formats_activity_as_read_basename() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "Read".to_string(),

        file_path: Some("/src/session/manager.rs".to_string()),
        command_preview: None,
    });

    assert_eq!(managed.session.current_activity, "Read: manager.rs");
}

#[test]
fn tool_use_write_with_file_path_formats_activity_as_write_basename() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "Write".to_string(),

        file_path: Some("/src/new_module.rs".to_string()),
        command_preview: None,
    });

    assert_eq!(managed.session.current_activity, "Write: new_module.rs");
}

#[test]
fn tool_use_bash_with_command_preview_formats_activity_with_dollar_prefix() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "Bash".to_string(),

        file_path: None,
        command_preview: Some("cargo test".to_string()),
    });

    assert_eq!(managed.session.current_activity, "$ cargo test");
}

#[test]
fn tool_use_without_file_path_and_without_command_preview_falls_back() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "WebSearch".to_string(),

        file_path: None,
        command_preview: None,
    });

    assert_eq!(managed.session.current_activity, "Using WebSearch");
}

#[test]
fn tool_result_success_logs_elapsed_time_string() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "Read".to_string(),

        file_path: Some("src/main.rs".to_string()),
        command_preview: None,
    });

    let log_len_before = managed.session.activity_log.len();

    managed.handle_event(&StreamEvent::ToolResult {
        tool: "Read".to_string(),
        is_error: false,
    });

    assert!(
        managed.session.activity_log.len() > log_len_before,
        "ToolResult success must add an elapsed time log entry"
    );
    let last = &managed.session.activity_log.last().unwrap().message;
    assert!(
        last.contains("done") || last.contains("ms") || last.contains("s"),
        "elapsed log entry must contain time indicator, got: {:?}",
        last
    );
}

// ---------------------------------------------------------------------------
// Issue #102: Phase 2 — Thinking block extraction
// ---------------------------------------------------------------------------

#[test]
fn thinking_event_sets_current_activity_to_thinking() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::Thinking {
        text: "some internal reasoning".to_string(),
    });

    assert_eq!(managed.session.current_activity, "Thinking...");
}

#[test]
fn multiple_thinking_events_do_not_flood_activity_log() {
    let mut managed = ManagedSession::new(make_session("s"));

    for _ in 0..3 {
        managed.handle_event(&StreamEvent::Thinking {
            text: "still thinking".to_string(),
        });
    }

    assert!(
        managed.session.activity_log.len() <= 1,
        "Thinking events must not flood activity_log; got {} entries",
        managed.session.activity_log.len()
    );
}

#[test]
fn non_thinking_event_after_thinking_logs_thought_duration() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::Thinking {
        text: "pondering".to_string(),
    });

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "Read".to_string(),

        file_path: Some("src/lib.rs".to_string()),
        command_preview: None,
    });

    assert!(
        managed
            .session
            .activity_log
            .iter()
            .any(|e| e.message.contains("Thought for")),
        "activity log must contain a 'Thought for' duration entry"
    );
}

#[test]
fn multiple_thinking_events_produce_single_duration_log_on_transition() {
    let mut managed = ManagedSession::new(make_session("s"));

    for _ in 0..3 {
        managed.handle_event(&StreamEvent::Thinking {
            text: "chain reasoning".to_string(),
        });
    }

    managed.handle_event(&StreamEvent::ToolUse {
        tool: "Bash".to_string(),

        file_path: None,
        command_preview: Some("cargo fmt".to_string()),
    });

    let thought_entries = managed
        .session
        .activity_log
        .iter()
        .filter(|e| e.message.contains("Thought for"))
        .count();

    assert_eq!(
        thought_entries, 1,
        "exactly one 'Thought for' entry must be logged"
    );
}

// ---------------------------------------------------------------------------
// Issue #102: Phase 3 — Streaming text feedback
// ---------------------------------------------------------------------------

#[test]
fn assistant_message_long_sets_truncated_preview_in_current_activity() {
    let mut managed = ManagedSession::new(make_session("s"));
    let long_text = "A".repeat(80);

    managed.handle_event(&StreamEvent::AssistantMessage { text: long_text });

    let activity = &managed.session.current_activity;
    assert!(
        activity.ends_with('…'),
        "long AssistantMessage preview must end with ellipsis, got: {:?}",
        activity
    );
    let without_ellipsis = activity.trim_end_matches('…');
    assert!(
        without_ellipsis.chars().count() <= 40,
        "preview prefix must be at most 40 chars, got: {:?}",
        without_ellipsis
    );
}

#[test]
fn assistant_message_short_shown_fully_in_current_activity() {
    let mut managed = ManagedSession::new(make_session("s"));

    managed.handle_event(&StreamEvent::AssistantMessage {
        text: "Fixed.".to_string(),
    });

    assert_eq!(managed.session.current_activity, "Fixed.");
}

#[test]
fn assistant_message_does_not_add_to_activity_log() {
    let mut managed = ManagedSession::new(make_session("s"));

    for i in 0..5 {
        managed.handle_event(&StreamEvent::AssistantMessage {
            text: format!("chunk {}", i),
        });
    }

    assert_eq!(
        managed.session.activity_log.len(),
        0,
        "AssistantMessage events must not push entries to activity_log"
    );
}

// Regression: roundtrip through parser and handler
#[test]
fn roundtrip_tool_use_bash_command_preview_preserved_through_parse_and_handle() {
    let line = r#"{"type":"assistant","message":{"type":"tool_use","name":"Bash","input":{"command":"cargo test --lib"}}}"#;
    let event = parse_stream_line(line);

    let mut managed = ManagedSession::new(make_session("s"));
    managed.handle_event(&event);

    assert_eq!(managed.session.current_activity, "$ cargo test --lib");
}
