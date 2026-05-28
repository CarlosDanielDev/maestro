#![cfg(test)]
//! Tests for [`super::dynamic_map_draw`]. Split from the rendering
//! module (#909) to keep the production-code file under the 400-LOC
//! guardrail.

use std::collections::HashMap;

use ratatui::text::Line;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::schema::{DefaultValue, FieldKind, FieldSchema};
use crate::tui::screens::settings::validation::ValidationFeedback;
use crate::tui::theme::Theme;

use super::dynamic_map::DynamicMapWidget;
use super::dynamic_map_draw::truncated_titles;
use super::entry_state::EntryState;

const EMPTY_FIELDS: &[FieldSchema] = &[];

fn entry(id: &str) -> EntryState {
    EntryState::build("agents", id.to_string(), EMPTY_FIELDS, None)
}

// #909 — minimal single-text-field schema for the role_overrides
// sub-tree, used by the `draw_with_warnings` plumbing tests below.
const ROLE_OVERRIDE_FIELDS: &[FieldSchema] = &[FieldSchema {
    key: "agent",
    label: "agent",
    help: "",
    default: DefaultValue::Str(""),
    kind: FieldKind::String,
    validator: None,
    presentation: None,
}];

fn role_overrides_widget(role_id: &str, agent_value: &str) -> DynamicMapWidget {
    let mut role_table = toml::map::Map::new();
    let mut entry_table = toml::map::Map::new();
    entry_table.insert(
        "agent".to_string(),
        toml::Value::String(agent_value.to_string()),
    );
    role_table.insert(role_id.to_string(), toml::Value::Table(entry_table));
    let existing = toml::Value::Table(role_table);
    DynamicMapWidget::new(
        "teams.worker-pool.role_overrides",
        "teams.worker-pool.role_overrides",
        ROLE_OVERRIDE_FIELDS,
        Some(&existing),
    )
}

fn title_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>()
}

#[test]
fn truncated_titles_returns_empty_for_no_entries() {
    let (titles, idx) = truncated_titles(&[], 0, 80);
    assert!(titles.is_empty());
    assert_eq!(idx, 0);
}

#[test]
fn truncated_titles_fits_all_when_under_budget() {
    let entries: Vec<EntryState> = (0..5).map(|i| entry(&format!("a{i}"))).collect();
    let (titles, idx) = truncated_titles(&entries, 2, 80);
    assert_eq!(titles.len(), 5, "all entries fit, no truncation");
    assert_eq!(idx, 2, "highlight index unchanged when all fit");
}

#[test]
fn truncated_titles_active_first_truncates_right_only() {
    let entries: Vec<EntryState> = (0..12).map(|i| entry(&format!("agent-{i:02}"))).collect();
    let (titles, idx) = truncated_titles(&entries, 0, 40);
    assert_eq!(
        title_text(&titles[0]),
        "agent-00",
        "active-first must place the active entry at slot 0"
    );
    assert_eq!(
        title_text(titles.last().unwrap()),
        "…",
        "active-first must end with the trailing ellipsis"
    );
    assert_eq!(idx, 0);
}

#[test]
fn truncated_titles_active_last_truncates_left_only() {
    let entries: Vec<EntryState> = (0..12).map(|i| entry(&format!("agent-{i:02}"))).collect();
    let (titles, idx) = truncated_titles(&entries, 11, 40);
    assert_eq!(
        title_text(&titles[0]),
        "…",
        "active-last must start with the leading ellipsis"
    );
    assert_eq!(
        title_text(titles.last().unwrap()),
        "agent-11",
        "active-last must place the active entry at the end"
    );
    assert_eq!(idx, titles.len() - 1);
}

#[test]
fn truncated_titles_active_middle_truncates_both_sides() {
    let entries: Vec<EntryState> = (0..12).map(|i| entry(&format!("agent-{i:02}"))).collect();
    let (titles, _idx) = truncated_titles(&entries, 6, 40);
    assert_eq!(
        title_text(&titles[0]),
        "…",
        "middle-active must have leading ellipsis"
    );
    assert_eq!(
        title_text(titles.last().unwrap()),
        "…",
        "middle-active must have trailing ellipsis"
    );
}

#[test]
fn draw_with_warnings_passes_lookup_to_text_input() {
    // #909 — render the role_overrides DynamicMap with a warning map
    // that has an entry for the inner `agent` TextInput. The widget
    // must thread the warning through to TextInput::draw and the
    // inline-warning line must appear in the rendered buffer.
    let widget = role_overrides_widget("reviewer", "bad-agent");
    let mut warnings: HashMap<String, ValidationFeedback> = HashMap::new();
    warnings.insert(
        "teams.worker-pool.role_overrides.reviewer.agent".to_string(),
        ValidationFeedback::warning("unknown agent `bad-agent`"),
    );

    let theme = Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(80, 16)).expect("backend");
    terminal
        .draw(|f| {
            widget.draw_with_warnings(f, f.area(), &theme, true, &warnings);
        })
        .expect("draw");

    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(
        rendered.contains("unknown agent") || rendered.contains("bad-agent"),
        "warning text must appear in rendered buffer, got:\n{rendered}",
    );
}

#[test]
fn draw_with_warnings_empty_map_renders_clean() {
    // #909 — passing an empty warning map must produce a buffer
    // identical to the existing `draw` path (no spurious glyphs, no
    // height changes).
    let widget = role_overrides_widget("reviewer", "claude");
    let empty: HashMap<String, ValidationFeedback> = HashMap::new();
    let theme = Theme::dark();

    let mut t_with = Terminal::new(TestBackend::new(80, 16)).expect("backend");
    let mut t_plain = Terminal::new(TestBackend::new(80, 16)).expect("backend");

    t_with
        .draw(|f| {
            widget.draw_with_warnings(f, f.area(), &theme, false, &empty);
        })
        .expect("draw");
    t_plain
        .draw(|f| {
            widget.draw(f, f.area(), &theme, false);
        })
        .expect("draw");

    assert_eq!(
        format!("{:?}", t_with.backend().buffer()),
        format!("{:?}", t_plain.backend().buffer()),
        "draw_with_warnings(empty) must produce output identical to draw()",
    );
}
