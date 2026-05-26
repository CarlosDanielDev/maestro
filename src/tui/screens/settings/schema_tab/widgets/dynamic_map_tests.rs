#![cfg(test)]
//! Behavioral and state-machine tests for [`super::DynamicMapWidget`].

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::clock::{Clock, FakeClock};
use super::dynamic_map::{DynamicMapWidget, MapFocus};
use super::test_fixture::TEST_AGENT_FIELDS;
use crate::tui::widgets::WidgetKind;

fn ev(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn ev_ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn typed_seq(w: &mut DynamicMapWidget, s: &str) {
    for c in s.chars() {
        w.handle_input(ev(KeyCode::Char(c)));
    }
}

fn fresh_with_clock() -> (DynamicMapWidget, Arc<FakeClock>) {
    let clock: Arc<FakeClock> = Arc::new(FakeClock::new());
    let w = DynamicMapWidget::with_clock(
        "agents.entries",
        "agents",
        TEST_AGENT_FIELDS,
        None,
        clock.clone() as Arc<dyn Clock>,
    );
    (w, clock)
}

#[test]
fn starts_empty() {
    let (w, _) = fresh_with_clock();
    assert!(w.entries().is_empty());
    assert!(w.active_entry().is_none());
    assert_eq!(*w.focus(), MapFocus::SubtabStrip);
}

#[test]
fn pressing_a_opens_add_modal() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    assert_eq!(*w.focus(), MapFocus::AddModal);
}

#[test]
fn add_modal_submit_inserts_entry() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(w.entries().len(), 1);
    assert_eq!(w.entries()[0].id, "claude");
}

#[test]
fn add_modal_submit_focuses_first_editable_field() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(*w.focus(), MapFocus::EntryField(0));
}

#[test]
fn add_modal_invalid_id_keeps_modal_open() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "X");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(*w.focus(), MapFocus::AddModal);
    assert!(w.entries().is_empty());
}

#[test]
fn add_modal_collision_keeps_modal_open() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(w.entries().len(), 1);
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(*w.focus(), MapFocus::AddModal);
    assert_eq!(w.entries().len(), 1);
}

#[test]
fn bracket_right_advances_tab() {
    let (mut w, _) = fresh_with_clock();
    for id in ["alpha", "bravo"] {
        w.handle_input(ev(KeyCode::Char('a')));
        typed_seq(&mut w, id);
        w.handle_input(ev(KeyCode::Enter));
    }
    // After Submit, focus is on EntryField(0); step Up to the subtab strip
    // so `]` is interpreted as next-tab and not delegated to the inner field.
    w.handle_input(ev(KeyCode::Up));
    w.handle_input(ev(KeyCode::Char(']')));
    assert_eq!(w.active_index(), Some(0));
}

#[test]
fn bracket_left_moves_back() {
    let (mut w, _) = fresh_with_clock();
    for id in ["alpha", "bravo"] {
        w.handle_input(ev(KeyCode::Char('a')));
        typed_seq(&mut w, id);
        w.handle_input(ev(KeyCode::Enter));
    }
    w.handle_input(ev(KeyCode::Up));
    w.handle_input(ev(KeyCode::Char('[')));
    assert_eq!(w.active_index(), Some(0));
}

#[test]
fn bracket_ignored_when_entry_field_focused() {
    let (mut w, _) = fresh_with_clock();
    for id in ["alpha", "bravo"] {
        w.handle_input(ev(KeyCode::Char('a')));
        typed_seq(&mut w, id);
        w.handle_input(ev(KeyCode::Enter));
    }
    // Focus is on EntryField(0); `]` must be delegated to the inner widget
    // (so users can type the literal bracket into a text field) instead of
    // switching tabs.
    let before = w.active_index();
    w.handle_input(ev(KeyCode::Char(']')));
    assert_eq!(w.active_index(), before);
}

#[test]
fn agents_kind_subprocess_hides_http_only_fields() {
    use crate::config::schema::dynamic::AGENTS_ENTRY_FIELDS;
    use std::sync::Arc;
    let clock: Arc<FakeClock> = Arc::new(FakeClock::new());
    let mut w = DynamicMapWidget::with_clock(
        "agents",
        "agents",
        AGENTS_ENTRY_FIELDS,
        None,
        clock.clone() as Arc<dyn Clock>,
    );
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    // kind defaults to "claude" (subprocess). HTTP-only fields must not
    // appear in the visible index list.
    let visible = w.visible_field_indices();
    let visible_keys: Vec<&str> = visible
        .iter()
        .map(|&i| AGENTS_ENTRY_FIELDS[i].key)
        .collect();
    for hidden in ["base_url", "api_key_env", "request_timeout_secs"] {
        assert!(
            !visible_keys.contains(&hidden),
            "subprocess kind must hide `{hidden}`; visible = {:?}",
            visible_keys
        );
    }
    for shown in [
        "kind",
        "enabled",
        "command",
        "model",
        "extra_args",
        "permission_mode",
        "allowed_tools",
        "sandbox",
    ] {
        assert!(
            visible_keys.contains(&shown),
            "subprocess kind must show `{shown}`; visible = {:?}",
            visible_keys
        );
    }
}

#[test]
fn agents_kind_http_hides_subprocess_only_fields() {
    use crate::config::schema::dynamic::AGENTS_ENTRY_FIELDS;
    use std::sync::Arc;
    let clock: Arc<FakeClock> = Arc::new(FakeClock::new());
    let mut w = DynamicMapWidget::with_clock(
        "agents",
        "agents",
        AGENTS_ENTRY_FIELDS,
        None,
        clock.clone() as Arc<dyn Clock>,
    );
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "ollama");
    w.handle_input(ev(KeyCode::Enter));
    // Switch kind from default "claude" to "ollama" by walking the
    // Dropdown to position of "ollama" in AGENT_KINDS:
    // [claude, codex, qwen, opencode, ollama, minimax] — 4 Right presses.
    assert_eq!(*w.focus(), MapFocus::EntryField(0));
    for _ in 0..4 {
        w.handle_input(ev(KeyCode::Right));
    }
    let visible = w.visible_field_indices();
    let visible_keys: Vec<&str> = visible
        .iter()
        .map(|&i| AGENTS_ENTRY_FIELDS[i].key)
        .collect();
    for hidden in [
        "command",
        "permission_mode",
        "sandbox",
        "allowed_tools",
        "extra_args",
    ] {
        assert!(
            !visible_keys.contains(&hidden),
            "http kind must hide `{hidden}`; visible = {:?}",
            visible_keys
        );
    }
    for shown in [
        "kind",
        "enabled",
        "base_url",
        "model",
        "api_key_env",
        "request_timeout_secs",
    ] {
        assert!(
            visible_keys.contains(&shown),
            "http kind must show `{shown}`; visible = {:?}",
            visible_keys
        );
    }
}

#[test]
fn switching_kind_clamps_focus_off_now_hidden_field() {
    use crate::config::schema::dynamic::AGENTS_ENTRY_FIELDS;
    use std::sync::Arc;
    let clock: Arc<FakeClock> = Arc::new(FakeClock::new());
    let mut w = DynamicMapWidget::with_clock(
        "agents",
        "agents",
        AGENTS_ENTRY_FIELDS,
        None,
        clock.clone() as Arc<dyn Clock>,
    );
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    // Walk down to `command` (field 2 — subprocess-only). Validate.
    w.handle_input(ev(KeyCode::Down));
    w.handle_input(ev(KeyCode::Down));
    assert_eq!(*w.focus(), MapFocus::EntryField(2));
    // Go back to kind, switch to ollama.
    w.handle_input(ev(KeyCode::Up));
    w.handle_input(ev(KeyCode::Up));
    assert_eq!(*w.focus(), MapFocus::EntryField(0));
    for _ in 0..4 {
        w.handle_input(ev(KeyCode::Right));
    }
    // Walk Down from kind — must land on a VISIBLE field, never on
    // `command` (now hidden under http kind).
    w.handle_input(ev(KeyCode::Down));
    let visible = w.visible_field_indices();
    if let MapFocus::EntryField(n) = *w.focus() {
        assert!(
            visible.contains(&n),
            "after switching kind, focus {} must be on a visible field; visible = {:?}",
            n,
            visible
        );
        assert_ne!(
            AGENTS_ENTRY_FIELDS[n].key, "command",
            "focus must not land on now-hidden `command`"
        );
    } else {
        panic!("expected EntryField focus");
    }
}

#[test]
fn d_typed_into_text_field_does_not_open_remove_modal() {
    // Regression: user types "opencode" into a String field; the inner
    // 'd' should land in the TextInput buffer, NOT trigger the Remove-
    // entry shortcut. Pre-fix, DynamicMap.handle_input matched
    // Char('d') before delegating, so the modal opened mid-word.
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    // Focus is EntryField(0). Walk Down to a String field — TEST_AGENT_FIELDS
    // index 2 is `command` (String). 0 = kind (Enum), 1 = enabled (Bool),
    // 2 = command (String).
    w.handle_input(ev(KeyCode::Down));
    w.handle_input(ev(KeyCode::Down));
    assert_eq!(*w.focus(), MapFocus::EntryField(2));
    // Enter insert mode on the TextInput.
    w.handle_input(ev(KeyCode::Enter));
    // Type "opencode" — the `d` and `a` must stay characters, not open
    // Remove / Add modals.
    typed_seq(&mut w, "opencode");
    assert!(
        w.add_modal().is_none(),
        "`a` typed while editing must not open Add modal"
    );
    assert!(
        w.remove_modal().is_none(),
        "`d` typed while editing must not open Remove modal"
    );
}

#[test]
fn down_advances_through_entry_fields() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "alpha");
    w.handle_input(ev(KeyCode::Enter));
    // Submit puts focus on EntryField(0); Down walks to (1), (2), ... until
    // saturated.
    let len = w.entry_fields.len();
    assert!(len >= 2);
    assert_eq!(*w.focus(), MapFocus::EntryField(0));
    w.handle_input(ev(KeyCode::Down));
    assert_eq!(*w.focus(), MapFocus::EntryField(1));
    // Saturates at the last entry-field index.
    for _ in 0..(len + 4) {
        w.handle_input(ev(KeyCode::Down));
    }
    assert_eq!(*w.focus(), MapFocus::EntryField(len - 1));
}

#[test]
fn up_from_first_entry_field_returns_to_subtab_strip() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "alpha");
    w.handle_input(ev(KeyCode::Enter));
    assert_eq!(*w.focus(), MapFocus::EntryField(0));
    w.handle_input(ev(KeyCode::Up));
    assert_eq!(*w.focus(), MapFocus::SubtabStrip);
}

#[test]
fn remove_then_undo_restores() {
    let (mut w, clock) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    w.handle_input(ev(KeyCode::Char('d')));
    w.handle_input(ev(KeyCode::Char('y')));
    assert!(w.entries().is_empty());
    assert!(w.undo_active());
    clock.advance(Duration::from_secs(2));
    w.handle_input(ev(KeyCode::Char('u')));
    assert_eq!(w.entries().len(), 1);
    assert_eq!(w.entries()[0].id, "claude");
}

#[test]
fn remove_then_undo_after_window_is_noop() {
    let (mut w, clock) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    w.handle_input(ev(KeyCode::Char('d')));
    w.handle_input(ev(KeyCode::Char('y')));
    clock.advance(Duration::from_secs(6));
    assert!(!w.undo_active());
    w.handle_input(ev(KeyCode::Char('u')));
    assert!(w.entries().is_empty());
}

#[test]
fn remove_cancel_keeps_entry() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    w.handle_input(ev(KeyCode::Char('d')));
    w.handle_input(ev(KeyCode::Char('n')));
    assert_eq!(w.entries().len(), 1);
}

#[test]
fn serialize_to_toml_emits_keyed_table() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    let v = w.serialize_to_toml();
    let t = v.as_table().expect("table");
    assert!(t.contains_key("claude"));
}

#[test]
fn d_with_no_entries_is_noop() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('d')));
    assert_eq!(*w.focus(), MapFocus::SubtabStrip);
}

#[test]
fn edit_hint_is_static() {
    let (w, _) = fresh_with_clock();
    let hints = w.edit_hint();
    assert!(
        !hints.is_empty(),
        "edit_hint must return ≥1 (key, label) pair"
    );
    for (k, l) in hints {
        assert!(!k.is_empty());
        assert!(!l.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Issue #809 — flat-render (no surrounding Block) tests for DynamicMapWidget.
// ---------------------------------------------------------------------------

use ratatui::style::Modifier;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

const BOX_GLYPHS: &[char] = &['┌', '─', '┐', '│', '└', '┘', '╔', '═', '╗', '║', '╚', '╝'];

fn render_map(widget: &DynamicMapWidget, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            widget.draw(f, f.area(), &crate::tui::theme::Theme::dark(), false);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn assert_no_border_glyphs(buf: &ratatui::buffer::Buffer) {
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let sym = buf[(x, y)].symbol();
            for &bc in BOX_GLYPHS {
                assert!(
                    !sym.contains(bc),
                    "unexpected border glyph {:?} at ({}, {}); symbol={:?}",
                    bc,
                    x,
                    y,
                    sym
                );
            }
        }
    }
}

#[test]
fn draw_does_not_paint_block_borders_empty_state() {
    let (w, _) = fresh_with_clock();
    let buf = render_map(&w, 80, 8);
    assert_no_border_glyphs(&buf);
}

#[test]
fn draw_does_not_paint_block_borders_with_entries() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    let buf = render_map(&w, 80, 8);
    assert_no_border_glyphs(&buf);
}

#[test]
fn draw_renders_label_as_header_with_colon_on_row_0() {
    let (w, _) = fresh_with_clock();
    let buf = render_map(&w, 80, 8);
    let row0: String = (0..80u16)
        .map(|x| buf[(x, 0)].symbol().to_owned())
        .collect();
    assert!(
        row0.contains("agents.entries:"),
        "header row 0 must contain 'agents.entries:'; got: {:?}",
        row0
    );
}

#[test]
fn draw_renders_label_as_header_with_text_secondary_style() {
    let (w, _) = fresh_with_clock();
    let buf = render_map(&w, 80, 8);
    let theme = crate::tui::theme::Theme::dark();
    let non_space: Vec<_> = (0..80u16)
        .map(|x| buf[(x, 0)].clone())
        .filter(|c| c.symbol() != " ")
        .collect();
    assert!(
        !non_space.is_empty(),
        "row 0 must contain non-space header text"
    );
    for cell in &non_space {
        assert_eq!(
            cell.style().fg,
            Some(theme.text_secondary),
            "header cell fg must be text_secondary; symbol={:?}",
            cell.symbol()
        );
        assert!(
            cell.style().add_modifier.contains(Modifier::BOLD),
            "header cell must have BOLD modifier; symbol={:?}",
            cell.symbol()
        );
    }
}

// ---------------------------------------------------------------------------
// Issue #901 — nested DynamicMap inside an EntryState (role_overrides editor).
// ---------------------------------------------------------------------------

fn teams_dm_with_one_role_override() -> DynamicMapWidget {
    let mut reviewer = toml::map::Map::new();
    reviewer.insert("agent".into(), toml::Value::String("opencode".into()));
    reviewer.insert("mode".into(), toml::Value::String("review-strict".into()));
    let mut roles = toml::map::Map::new();
    roles.insert("reviewer".into(), toml::Value::Table(reviewer));

    let mut team = toml::map::Map::new();
    team.insert("extends".into(), toml::Value::String("".into()));
    team.insert("primitive".into(), toml::Value::String("pipeline".into()));
    team.insert("role_overrides".into(), toml::Value::Table(roles));

    let mut outer = toml::map::Map::new();
    outer.insert("worker-pool".into(), toml::Value::Table(team));
    let val = toml::Value::Table(outer);

    DynamicMapWidget::new(
        "entries",
        "teams",
        crate::config::schema::dynamic::TEAMS_ENTRY_FIELDS,
        Some(&val),
    )
}

#[test]
fn display_name_for_role_overrides_returns_role() {
    use super::dynamic_map::display_name_for;
    assert_eq!(
        display_name_for("teams.worker-pool.role_overrides"),
        "role",
        "section paths ending in role_overrides must surface as 'role' in user-facing strings"
    );
}

#[test]
fn display_name_for_role_overrides_extra_suffix_does_not_match() {
    // Edge case A — suffix match must be exact, not substring. A
    // hypothetical future `role_overrides_v2` key must NOT inherit the
    // "role" display name.
    use super::dynamic_map::display_name_for;
    let result = display_name_for("teams.worker-pool.role_overrides_extra");
    assert_ne!(
        result, "role",
        "suffix match must require trailing component to be exactly role_overrides; got {result:?}"
    );
}

#[test]
fn nested_dynamic_map_field_is_built_as_dynamic_map_widget() {
    // Sanity: the role_overrides field on a teams entry is a DynamicMap,
    // not a TextInput placeholder.
    let w = teams_dm_with_one_role_override();
    let entry = w
        .entries()
        .iter()
        .find(|e| e.id == "worker-pool")
        .expect("worker-pool entry present");
    let WidgetKind::DynamicMap(ref _inner) = entry.fields[4].widget else {
        panic!(
            "role_overrides field (index 4) must be DynamicMap, got label {:?}",
            entry.fields[4].widget.label()
        );
    };
}

#[test]
fn nested_dynamic_map_owns_a_chord_when_focused() {
    // Pressing `a` while focused on the role_overrides field must NOT
    // open the outer "Add team entry" modal. The inner DynamicMap takes
    // the chord via focused_field_owns_chord and opens its own modal.
    let mut w = teams_dm_with_one_role_override();
    for _ in 0..5 {
        w.handle_input(ev(KeyCode::Tab));
    }
    assert_eq!(*w.focus(), MapFocus::EntryField(4));

    w.handle_input(ev(KeyCode::Char('a')));

    assert!(
        w.add_modal().is_none(),
        "outer add_modal must remain None — inner DynamicMap owns `a`"
    );
    assert_eq!(
        *w.focus(),
        MapFocus::EntryField(4),
        "outer focus must remain on role_overrides field, not jump to AddModal"
    );

    let entry = w
        .entries()
        .iter()
        .find(|e| e.id == "worker-pool")
        .expect("worker-pool");
    let WidgetKind::DynamicMap(ref inner) = entry.fields[4].widget else {
        panic!("role_overrides field must be DynamicMap");
    };
    assert!(
        matches!(inner.focus(), MapFocus::AddModal),
        "inner DynamicMap must have opened its AddModal in response to `a`"
    );
}

#[test]
fn nested_dynamic_map_d_does_not_open_outer_remove_modal() {
    // Pressing `d` while focused on the role_overrides field must NOT
    // open the outer remove-team-entry modal.
    let mut w = teams_dm_with_one_role_override();
    for _ in 0..5 {
        w.handle_input(ev(KeyCode::Tab));
    }
    assert_eq!(*w.focus(), MapFocus::EntryField(4));

    w.handle_input(ev(KeyCode::Char('d')));

    assert!(
        w.remove_modal().is_none(),
        "outer remove_modal must remain None — inner DynamicMap owns `d`"
    );
    assert_eq!(
        *w.focus(),
        MapFocus::EntryField(4),
        "outer focus must remain on role_overrides field, not jump to RemoveConfirm"
    );
}

#[test]
fn nested_dynamic_map_contributes_multi_line_desired_height() {
    // Focusing the role_overrides field on an entry that has at least one
    // role must grow desired_height to fit the nested editor (header +
    // tabstrip + per-role-field rows).
    let mut w = teams_dm_with_one_role_override();
    let base = w.desired_height();
    for _ in 0..5 {
        w.handle_input(ev(KeyCode::Tab));
    }
    assert_eq!(*w.focus(), MapFocus::EntryField(4));
    let focused = w.desired_height();
    assert!(
        focused > base + 3,
        "focused nested DynamicMap must grow desired_height by more than 3 lines; base={base} focused={focused}"
    );
}

#[test]
fn add_modal_title_on_inner_role_overrides_dm_says_add_role() {
    // Edge case D — the inner modal must read "Add role", not
    // "Add teams.worker-pool.role_overrides entry".
    let mut w = teams_dm_with_one_role_override();
    for _ in 0..5 {
        w.handle_input(ev(KeyCode::Tab));
    }
    w.handle_input(ev(KeyCode::Char('a')));

    let entry = w
        .entries()
        .iter()
        .find(|e| e.id == "worker-pool")
        .expect("worker-pool");
    let WidgetKind::DynamicMap(ref inner) = entry.fields[4].widget else {
        panic!("role_overrides must be DynamicMap");
    };
    let modal = inner.add_modal().expect("inner add_modal must be open");
    assert!(
        modal.title.contains("Add role"),
        "inner add modal title must contain 'Add role', got {:?}",
        modal.title
    );
    assert!(
        !modal.title.contains("role_overrides"),
        "inner add modal must not expose raw key 'role_overrides' in title, got {:?}",
        modal.title
    );
}

#[test]
fn draw_undo_banner_starts_at_column_0() {
    let (mut w, _) = fresh_with_clock();
    w.handle_input(ev(KeyCode::Char('a')));
    typed_seq(&mut w, "claude");
    w.handle_input(ev(KeyCode::Enter));
    w.handle_input(ev(KeyCode::Char('d')));
    w.handle_input(ev(KeyCode::Char('y')));
    assert!(w.undo_active(), "undo must be active after delete");

    let mut terminal = Terminal::new(TestBackend::new(80, 8)).unwrap();
    terminal
        .draw(|f| {
            w.draw(
                f,
                Rect::new(0, 0, 80, 8),
                &crate::tui::theme::Theme::dark(),
                false,
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();

    let last_row: String = (0..80u16)
        .map(|x| buf[(x, 7)].symbol().to_owned())
        .collect();
    assert!(
        last_row.starts_with("Removed 'claude'"),
        "undo banner must start at column 0 of the last row (no border +1 offset); got: {:?}",
        last_row
    );
}
