use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};
use uuid::Uuid;

use super::draw_agent_graph;
use crate::session::types::{Session, SessionStatus};
use crate::tui::agent_graph::model::build_graph;

fn make_session_with(id: u128, issue: u64, status: SessionStatus, files: &[&str]) -> Session {
    let mut s = Session::new(
        "task".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(issue),
        None,
    );
    s.id = Uuid::from_u128(id);
    s.status = status;
    s.files_touched = files.iter().map(|f| (*f).to_string()).collect();
    s
}

fn render_buffer(
    sessions: &[&Session],
    tick: usize,
    use_nerd_font: bool,
) -> ratatui::buffer::Buffer {
    let (nodes, edges) = build_graph(sessions);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|f| {
            draw_agent_graph(f, f.area(), &nodes, &edges, use_nerd_font, tick, sessions);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn buffer_has_color(buffer: &ratatui::buffer::Buffer, fg: Color) -> bool {
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if buffer[(x, y)].style().fg == Some(fg) {
                return true;
            }
        }
    }
    false
}

fn has_color_with_mod(buffer: &ratatui::buffer::Buffer, fg: Color, modifier: Modifier) -> bool {
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let style = buffer[(x, y)].style();
            if style.fg == Some(fg) && style.add_modifier.contains(modifier) {
                return true;
            }
        }
    }
    false
}

fn has_reversed_for_color(buffer: &ratatui::buffer::Buffer, fg: Color) -> bool {
    has_color_with_mod(buffer, fg, Modifier::REVERSED)
}

#[test]
fn file_style_is_neutral_color() {
    let (color, _) = super::file_style();
    assert_eq!(color, Color::Cyan);
}

#[test]
fn too_small_message_contains_dimensions() {
    let mut terminal = Terminal::new(TestBackend::new(79, 23)).unwrap();
    terminal
        .draw(|f| {
            draw_agent_graph(f, f.area(), &[], &[], false, 0, &[]);
        })
        .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("79"), "width not in message");
    assert!(rendered.contains("23"), "height not in message");
}

#[test]
fn running_node_label_prefixed_with_braille_spinner_at_tick_0() {
    let s1 = make_session_with(0, 101, SessionStatus::Running, &["src/auth.rs"]);
    let s2 = make_session_with(1, 102, SessionStatus::Running, &["src/config.rs"]);
    let buf = render_buffer(&[&s1, &s2], 0, true);
    let text = buffer_text(&buf);
    assert!(
        text.contains("\u{280B} #101") || text.contains("\u{280B} #102"),
        "braille spinner ⠋ must immediately precede a running node label"
    );
}

#[test]
fn running_node_label_prefixed_with_ascii_spinner_at_tick_0() {
    let s1 = make_session_with(0, 101, SessionStatus::Running, &["src/auth.rs"]);
    let s2 = make_session_with(1, 102, SessionStatus::Running, &["src/config.rs"]);
    let buf = render_buffer(&[&s1, &s2], 0, false);
    let text = buffer_text(&buf);
    assert!(
        text.contains("| #101") || text.contains("| #102"),
        "ascii spinner '|' must immediately precede a running node label"
    );
}

#[test]
fn queued_node_has_no_spinner_prefix() {
    // ASCII mode keeps the canvas marker as `█`, so the spinner char `|` is
    // unique to the label prefix.
    let s_queued = make_session_with(0, 101, SessionStatus::Queued, &["src/a.rs"]);
    let s_running = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    let buf = render_buffer(&[&s_queued, &s_running], 0, false);
    let text = buffer_text(&buf);
    assert!(text.contains("| #102"), "running node must show '| #102'");
    assert!(
        !text.contains("| #101"),
        "queued node must NOT show '| #101'"
    );
}

#[test]
fn completed_node_has_no_spinner_prefix() {
    let s_completed = make_session_with(0, 101, SessionStatus::Completed, &["src/a.rs"]);
    let s_running = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    let buf = render_buffer(&[&s_completed, &s_running], 2, false);
    let text = buffer_text(&buf);
    assert!(
        text.contains("- #102"),
        "running node must show '- #102' (tick-2 ASCII spinner)"
    );
    assert!(
        !text.contains("- #101"),
        "completed node must NOT show spinner prefix"
    );
}

#[test]
fn errored_node_has_no_spinner_prefix() {
    let s_errored = make_session_with(0, 101, SessionStatus::Errored, &["src/a.rs"]);
    let s_running = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    let buf = render_buffer(&[&s_errored, &s_running], 0, false);
    let text = buffer_text(&buf);
    assert!(text.contains("| #102"), "running node must show spinner");
    assert!(
        !text.contains("| #101"),
        "errored node must NOT show spinner"
    );
}

#[test]
fn running_node_empty_files_still_shows_spinner() {
    let s1 = make_session_with(0, 101, SessionStatus::Running, &[]);
    let s2 = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    let buf = render_buffer(&[&s1, &s2], 0, false);
    let text = buffer_text(&buf);
    assert!(text.contains("| #101"), "first running node must spin");
    assert!(text.contains("| #102"), "second running node must spin");
}

#[test]
fn edge_pulses_light_cyan_at_tick_0_when_activity_is_tool_use() {
    let mut s1 = make_session_with(0, 101, SessionStatus::Running, &["src/auth.rs"]);
    s1.current_activity = "Read: src/auth.rs".to_string();
    let s2 = make_session_with(1, 102, SessionStatus::Running, &["src/config.rs"]);
    let buf = render_buffer(&[&s1, &s2], 0, true);
    assert!(
        buffer_has_color(&buf, Color::LightCyan),
        "tick 0 with tool-use activity must produce a LightCyan edge cell"
    );
}

#[test]
fn edge_dim_cyan_at_tick_5_when_activity_is_tool_use() {
    let mut s1 = make_session_with(0, 101, SessionStatus::Running, &["src/auth.rs"]);
    s1.current_activity = "Read: src/auth.rs".to_string();
    let s2 = make_session_with(1, 102, SessionStatus::Running, &["src/config.rs"]);
    let buf = render_buffer(&[&s1, &s2], 5, true);
    assert!(
        buffer_has_color(&buf, Color::Cyan),
        "tick 5 (dim phase) must produce a Cyan edge cell"
    );
    assert!(
        !buffer_has_color(&buf, Color::LightCyan),
        "tick 5 must NOT produce LightCyan"
    );
}

#[test]
fn edge_no_pulse_when_activity_does_not_match_tool_prefix() {
    let mut s1 = make_session_with(0, 101, SessionStatus::Running, &["src/auth.rs"]);
    s1.current_activity = "Working on something".to_string();
    let s2 = make_session_with(1, 102, SessionStatus::Running, &["src/config.rs"]);
    let buf = render_buffer(&[&s1, &s2], 0, true);
    assert!(
        !buffer_has_color(&buf, Color::LightCyan),
        "non-tool-prefix activity must not pulse LightCyan"
    );
}

#[test]
fn edge_pulse_only_for_owning_agent_when_file_shared() {
    let mut s_tool = make_session_with(0, 101, SessionStatus::Running, &["src/shared.rs"]);
    s_tool.current_activity = "Read: src/shared.rs".to_string();
    let mut s_idle = make_session_with(1, 102, SessionStatus::Running, &["src/shared.rs"]);
    s_idle.current_activity = "Working on something".to_string();
    let buf = render_buffer(&[&s_tool, &s_idle], 0, true);
    assert!(
        buffer_has_color(&buf, Color::LightCyan),
        "tool-using agent's edge must pulse even when file is shared"
    );
}

#[test]
fn completed_node_flashes_light_green_bold_reversed_at_flash_4() {
    let mut s_completed = make_session_with(0, 101, SessionStatus::Completed, &["src/a.rs"]);
    s_completed.transition_flash_remaining = 4;
    let s_running = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    let buf = render_buffer(&[&s_completed, &s_running], 0, true);
    assert!(
        has_color_with_mod(&buf, Color::LightGreen, Modifier::BOLD | Modifier::REVERSED),
        "completed flash at counter=4 must apply LightGreen + BOLD|REVERSED"
    );
}

#[test]
fn completed_node_flashes_bold_only_at_flash_3() {
    let mut s_completed = make_session_with(0, 101, SessionStatus::Completed, &["src/a.rs"]);
    s_completed.transition_flash_remaining = 3;
    let s_running = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    let buf = render_buffer(&[&s_completed, &s_running], 0, true);
    assert!(
        has_color_with_mod(&buf, Color::LightGreen, Modifier::BOLD),
        "completed flash at counter=3 must apply LightGreen + BOLD"
    );
    assert!(
        !has_reversed_for_color(&buf, Color::LightGreen),
        "counter=3 (odd) must NOT apply REVERSED on LightGreen"
    );
}

#[test]
fn errored_node_flashes_light_red_bold_reversed_at_flash_4() {
    let mut s_errored = make_session_with(0, 101, SessionStatus::Errored, &["src/a.rs"]);
    s_errored.transition_flash_remaining = 4;
    let s_running = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    let buf = render_buffer(&[&s_errored, &s_running], 0, true);
    assert!(
        has_color_with_mod(&buf, Color::LightRed, Modifier::BOLD | Modifier::REVERSED),
        "errored flash at counter=4 must apply LightRed + BOLD|REVERSED"
    );
}

#[test]
fn completed_node_no_flash_when_counter_is_zero() {
    let mut s_completed = make_session_with(0, 101, SessionStatus::Completed, &["src/a.rs"]);
    s_completed.transition_flash_remaining = 0;
    let s_running = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    let buf = render_buffer(&[&s_completed, &s_running], 0, true);
    assert!(
        !buffer_has_color(&buf, Color::LightGreen),
        "no flash color when counter=0; node uses base style only"
    );
}

#[test]
fn running_node_no_flash_even_with_flash_counter_set() {
    let mut s_running = make_session_with(0, 101, SessionStatus::Running, &["src/a.rs"]);
    s_running.transition_flash_remaining = 4;
    let s_other = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    let buf = render_buffer(&[&s_running, &s_other], 0, true);
    assert!(
        !buffer_has_color(&buf, Color::LightGreen),
        "running node must never use LightGreen flash color"
    );
    assert!(
        !buffer_has_color(&buf, Color::LightRed),
        "running node must never use LightRed flash color"
    );
}

#[test]
fn flash_parity_rule_for_all_4_ticks() {
    let s_running = make_session_with(1, 102, SessionStatus::Running, &["src/b.rs"]);
    for counter in [4u8, 2u8] {
        let mut s_completed = make_session_with(0, 101, SessionStatus::Completed, &["src/a.rs"]);
        s_completed.transition_flash_remaining = counter;
        let buf = render_buffer(&[&s_completed, &s_running], 0, true);
        assert!(
            has_color_with_mod(&buf, Color::LightGreen, Modifier::BOLD | Modifier::REVERSED),
            "counter={counter} (even) must apply BOLD|REVERSED"
        );
    }
    for counter in [3u8, 1u8] {
        let mut s_completed = make_session_with(0, 101, SessionStatus::Completed, &["src/a.rs"]);
        s_completed.transition_flash_remaining = counter;
        let buf = render_buffer(&[&s_completed, &s_running], 0, true);
        assert!(
            !has_reversed_for_color(&buf, Color::LightGreen),
            "counter={counter} (odd) must NOT apply REVERSED on LightGreen"
        );
    }
}

#[test]
fn draw_agent_graph_idempotent_same_tick_and_sessions() {
    let mut s1 = make_session_with(0, 101, SessionStatus::Running, &["src/auth.rs"]);
    s1.current_activity = "Read: src/auth.rs".to_string();
    let s2 = make_session_with(1, 102, SessionStatus::Running, &["src/config.rs"]);
    let b1 = render_buffer(&[&s1, &s2], 7, true);
    let b2 = render_buffer(&[&s1, &s2], 7, true);
    assert_eq!(
        b1, b2,
        "identical inputs must produce byte-identical buffers"
    );
}

// ── status_modifier ──────────────────────────────────────────────────────────
//
// Composition contract: status is a Style::add_modifier overlay on the role
// color. These tests pin the table from
// docs/adr/002-agent-personalities.md § Status Modifier Composition.

#[test]
fn status_modifier_running_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::Running),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_gates_running_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::GatesRunning),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_needs_review_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::NeedsReview),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_needs_pr_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::NeedsPr),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_ci_fix_is_bold() {
    assert_eq!(super::status_modifier(SessionStatus::CiFix), Modifier::BOLD);
}

#[test]
fn status_modifier_conflict_fix_is_bold() {
    assert_eq!(
        super::status_modifier(SessionStatus::ConflictFix),
        Modifier::BOLD
    );
}

#[test]
fn status_modifier_errored_is_dim_bold() {
    // Intentionally NOT Modifier::CROSSED_OUT — crossterm portability concern
    // flagged in the issue test hints. DIM | BOLD reads "agent stopped working"
    // on every terminal.
    assert_eq!(
        super::status_modifier(SessionStatus::Errored),
        Modifier::DIM | Modifier::BOLD
    );
}

#[test]
fn status_modifier_completed_is_dim() {
    assert_eq!(
        super::status_modifier(SessionStatus::Completed),
        Modifier::DIM
    );
}

#[test]
fn status_modifier_killed_is_dim() {
    assert_eq!(super::status_modifier(SessionStatus::Killed), Modifier::DIM);
}

#[test]
fn status_modifier_paused_is_dim() {
    assert_eq!(super::status_modifier(SessionStatus::Paused), Modifier::DIM);
}

#[test]
fn status_modifier_stalled_is_dim_reversed() {
    assert_eq!(
        super::status_modifier(SessionStatus::Stalled),
        Modifier::DIM | Modifier::REVERSED
    );
}

#[test]
fn status_modifier_spawning_is_empty() {
    assert_eq!(
        super::status_modifier(SessionStatus::Spawning),
        Modifier::empty()
    );
}

#[test]
fn status_modifier_queued_is_empty() {
    assert_eq!(
        super::status_modifier(SessionStatus::Queued),
        Modifier::empty()
    );
}

#[test]
fn status_modifier_retrying_is_empty() {
    assert_eq!(
        super::status_modifier(SessionStatus::Retrying),
        Modifier::empty()
    );
}

// ── role color survives status overlay ──────────────────────────────────────

#[test]
fn reviewer_role_color_renders_in_buffer_for_running_status() {
    use crate::session::role::Role;

    let mut s = make_session_with(0, 539, SessionStatus::Running, &["src/x.rs"]);
    s.role = Role::Reviewer;
    let s2 = make_session_with(1, 540, SessionStatus::Queued, &["src/y.rs"]);
    let buf = render_buffer(&[&s, &s2], 0, true);

    assert!(
        buffer_has_color(&buf, Color::Magenta),
        "Reviewer role color (Magenta) must appear somewhere in the rendered \
         buffer for a Running node — the status overlay must NOT replace the \
         role color"
    );
}

#[test]
fn devops_role_color_renders_for_completed_status() {
    use crate::session::role::Role;

    // Even Completed (DIM modifier) must keep the role color (Red for DevOps).
    // The DIM modifier is layered; the Red foreground survives.
    let mut s = make_session_with(0, 539, SessionStatus::Completed, &["src/x.rs"]);
    s.role = Role::DevOps;
    s.transition_flash_remaining = 0; // Disable the LightRed flash overlay.
    let s2 = make_session_with(1, 540, SessionStatus::Running, &["src/y.rs"]);
    let buf = render_buffer(&[&s, &s2], 0, true);

    assert!(
        buffer_has_color(&buf, Color::Red),
        "DevOps role color (Red) must survive Completed status overlay"
    );
}
