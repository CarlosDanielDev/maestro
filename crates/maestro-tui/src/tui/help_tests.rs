use super::*;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup};

fn make_groups() -> Vec<KeyBindingGroup> {
    vec![
        KeyBindingGroup {
            title: "Scrolling",
            bindings: vec![
                KeyBinding {
                    key: "Up/Down",
                    description: "Scroll agent panel output",
                },
                KeyBinding {
                    key: "Shift+Up/Down",
                    description: "Scroll activity log",
                },
            ],
        },
        KeyBindingGroup {
            title: "General",
            bindings: vec![KeyBinding {
                key: "q",
                description: "Quit maestro",
            }],
        },
    ]
}

#[test]
fn help_overlay_state_new_is_default() {
    let state = HelpOverlayState::new();
    assert_eq!(state.scroll, 0);
    assert!(state.search_query.is_empty());
    assert!(!state.search_active);
}

#[test]
fn help_overlay_state_scroll_down_increments() {
    let mut state = HelpOverlayState::new();
    state.scroll = 3;
    state.scroll_down();
    assert_eq!(state.scroll, 4);
}

#[test]
fn help_overlay_state_scroll_up_decrements() {
    let mut state = HelpOverlayState::new();
    state.scroll = 3;
    state.scroll_up();
    assert_eq!(state.scroll, 2);
}

#[test]
fn help_overlay_state_scroll_up_saturates_at_zero() {
    let mut state = HelpOverlayState::new();
    state.scroll = 0;
    state.scroll_up();
    assert_eq!(state.scroll, 0);
}

#[test]
fn help_overlay_state_scroll_saturates_at_max() {
    let mut state = HelpOverlayState::new();
    state.scroll = u16::MAX;
    state.scroll_down();
    assert_eq!(state.scroll, u16::MAX);
}

#[test]
fn help_overlay_state_toggle_search_activates_then_deactivates() {
    let mut state = HelpOverlayState::new();
    assert!(!state.search_active);
    state.toggle_search();
    assert!(state.search_active);
    state.toggle_search();
    assert!(!state.search_active);
}

#[test]
fn help_overlay_state_push_char_appends() {
    let mut state = HelpOverlayState::new();
    state.push_char('s');
    state.push_char('c');
    assert_eq!(state.search_query, "sc");
}

#[test]
fn help_overlay_state_pop_char_removes_last() {
    let mut state = HelpOverlayState::new();
    state.search_query = "scroll".to_string();
    state.pop_char();
    assert_eq!(state.search_query, "scrol");
}

#[test]
fn help_overlay_state_pop_char_on_empty_is_noop() {
    let mut state = HelpOverlayState::new();
    state.pop_char();
    assert!(state.search_query.is_empty());
}

#[test]
fn help_overlay_state_clear_search_resets() {
    let mut state = HelpOverlayState::new();
    state.search_query = "foo".to_string();
    state.search_active = true;
    state.clear_search();
    assert!(state.search_query.is_empty());
    assert!(!state.search_active);
}

#[test]
fn filter_bindings_empty_query_returns_all() {
    let groups = make_groups();
    let result = filter_bindings(&groups, "");
    assert_eq!(result.len(), groups.len());
    let total_in: usize = groups.iter().map(|g| g.bindings.len()).sum();
    let total_out: usize = result.iter().map(|g| g.bindings.len()).sum();
    assert_eq!(total_in, total_out);
}

#[test]
fn filter_bindings_scroll_matches_relevant() {
    let groups = make_groups();
    let result = filter_bindings(&groups, "scroll");
    assert!(!result.is_empty());
    let all_descs: Vec<&str> = result
        .iter()
        .flat_map(|g| g.bindings.iter())
        .map(|b| b.description)
        .collect();
    for desc in &all_descs {
        assert!(
            desc.to_lowercase().contains("scroll"),
            "non-scroll binding survived filter: {}",
            desc
        );
    }
}

#[test]
fn filter_bindings_is_case_insensitive() {
    let groups = make_groups();
    let lower = filter_bindings(&groups, "scroll");
    let upper = filter_bindings(&groups, "SCROLL");
    let count_lower: usize = lower.iter().map(|g| g.bindings.len()).sum();
    let count_upper: usize = upper.iter().map(|g| g.bindings.len()).sum();
    assert_eq!(count_lower, count_upper);
}

#[test]
fn filter_bindings_no_match_returns_empty() {
    let groups = make_groups();
    let result = filter_bindings(&groups, "zzznomatch");
    assert!(result.is_empty());
}

#[test]
fn filter_bindings_matches_key() {
    let groups = make_groups();
    let result = filter_bindings(&groups, "shift");
    assert!(!result.is_empty());
    let matched: Vec<&str> = result
        .iter()
        .flat_map(|g| g.bindings.iter())
        .map(|b| b.key)
        .collect();
    assert!(matched.iter().any(|k| k.to_lowercase().contains("shift")));
}
