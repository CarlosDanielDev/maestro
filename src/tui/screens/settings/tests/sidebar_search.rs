use super::*;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

#[test]
fn slash_enters_search_mode() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    assert!(!screen.sidebar_search_active);
    screen.handle_input(&key(KeyCode::Char('/')), InputMode::Normal);
    assert!(screen.sidebar_search_active);
}

#[test]
fn typing_in_search_appends_to_query() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key(KeyCode::Char('/')), InputMode::Normal);
    for c in "agen".chars() {
        screen.handle_input(&key(KeyCode::Char(c)), InputMode::Normal);
    }
    assert_eq!(screen.sidebar_search, "agen");
}

#[test]
fn backspace_pops_query_char() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key(KeyCode::Char('/')), InputMode::Normal);
    for c in "abc".chars() {
        screen.handle_input(&key(KeyCode::Char(c)), InputMode::Normal);
    }
    screen.handle_input(&key(KeyCode::Backspace), InputMode::Normal);
    assert_eq!(screen.sidebar_search, "ab");
}

#[test]
fn enter_exits_search_and_keeps_query() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key(KeyCode::Char('/')), InputMode::Normal);
    for c in "th".chars() {
        screen.handle_input(&key(KeyCode::Char(c)), InputMode::Normal);
    }
    screen.handle_input(&key(KeyCode::Enter), InputMode::Normal);
    assert!(!screen.sidebar_search_active);
    assert_eq!(screen.sidebar_search, "th");
}

#[test]
fn esc_clears_query_and_exits_search() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key(KeyCode::Char('/')), InputMode::Normal);
    for c in "th".chars() {
        screen.handle_input(&key(KeyCode::Char(c)), InputMode::Normal);
    }
    screen.handle_input(&key(KeyCode::Esc), InputMode::Normal);
    assert!(!screen.sidebar_search_active);
    assert!(screen.sidebar_search.is_empty());
}

#[test]
fn visible_indices_filters_by_substring_case_insensitive() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.sidebar_search = "th".into();
    let visible = screen.sidebar_visible_indices();
    let labels: Vec<&str> = visible
        .iter()
        .map(|&idx| SettingsTab::ALL[idx].label())
        .collect();
    // "th" is a case-insensitive substring of both "GitHub" and "Theme".
    // Result preserves alphabetical order from ALPHABETICAL_INDICES.
    assert_eq!(labels, vec!["GitHub", "Theme"]);

    screen.sidebar_search = "TH".into();
    let visible_upper = screen.sidebar_visible_indices();
    assert_eq!(visible, visible_upper, "search is case-insensitive");
}

#[test]
fn visible_indices_returns_all_when_query_empty() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let visible = screen.sidebar_visible_indices();
    assert_eq!(visible.len(), SettingsTab::ALPHABETICAL_INDICES.len());
}

#[test]
fn visible_indices_empty_when_no_match() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.sidebar_search = "zzz_no_match".into();
    let visible = screen.sidebar_visible_indices();
    assert!(visible.is_empty());
}

#[test]
fn slash_inside_search_is_a_literal_not_a_re_entry() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key(KeyCode::Char('/')), InputMode::Normal);
    screen.handle_input(&key(KeyCode::Char('/')), InputMode::Normal);
    assert!(screen.sidebar_search_active);
    assert_eq!(screen.sidebar_search, "/");
}
