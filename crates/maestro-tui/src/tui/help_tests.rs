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

#[test]
fn key_column_is_capped_and_long_keys_keep_one_space() {
    let groups = vec![KeyBindingGroup {
        title: "Long",
        bindings: vec![KeyBinding {
            key: "a-key-much-longer-than-a-third-of-the-box",
            description: "desc",
        }],
    }];
    let key_col = key_column_width(&groups, 69);
    assert_eq!(key_col, 23, "capped at a third of the inner width");
    let line = key_line(
        groups[0].bindings[0].key,
        "desc",
        key_col,
        &Theme::default(),
    );
    assert_eq!(
        line.spans[1].content,
        "a-key-much-longer-than-a-third-of-the-box "
    );
    assert_eq!(
        key_column_width(&make_groups(), 69),
        "Shift+Up/Down".len() + 2
    );
}

// #1397: the Help overlay drawn at 80 columns. The scroll stops at the
// content end and every key keeps a gap before its description.

pub(crate) mod drawn {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    use crate::tui::app::{App, TuiMode};
    use crate::tui::input_handler::handle_key;
    use crate::tui::screens::LandingScreen;

    const FOOTER: &str = "[j/k] Scroll  [/] Search  [?/F1/Esc] Close";
    const LONG_KEY: &str = "⌃Q / ⌃X / ⌃C / q";
    pub(crate) const SIZES: [(u16, u16); 2] = [(80, 24), (80, 27)];

    pub(crate) async fn press(app: &mut App, code: KeyCode) {
        handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
    }

    pub(crate) fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>) -> Vec<String> {
        terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
        let buf = terminal.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    /// The text rows inside the Help box, without its borders.
    pub(crate) fn inner_rows(rows: &[String], width: u16, height: u16) -> Vec<String> {
        let popup = crate::tui::help::centered_rect(90, 90, Rect::new(0, 0, width, height));
        assert!(
            rows[popup.y as usize].contains("[ Help ]"),
            "Help box is drawn"
        );
        let (x0, x1) = (popup.x as usize + 1, (popup.x + popup.width) as usize - 1);
        rows[popup.y as usize + 1..(popup.y + popup.height) as usize - 1]
            .iter()
            .map(|r| r.chars().skip(x0).take(x1 - x0).collect())
            .collect()
    }

    async fn help_open(width: u16, height: u16) -> (App, Terminal<TestBackend>) {
        let mut app = crate::tui::make_test_app("help-1397");
        app.screen_state.landing_screen = Some(LandingScreen::new());
        app.tui_mode = TuiMode::Landing;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        draw(&mut app, &mut terminal);
        press(&mut app, KeyCode::Char('?')).await;
        assert!(app.help_state.is_some(), "? opens Help");
        (app, terminal)
    }

    #[tokio::test]
    async fn help_scroll_clamps_to_content_end() {
        for (w, h) in SIZES {
            let (mut app, mut terminal) = help_open(w, h).await;
            for _ in 0..100 {
                press(&mut app, KeyCode::Char('j')).await;
                draw(&mut app, &mut terminal);
            }
            let inner = inner_rows(&draw(&mut app, &mut terminal), w, h);
            let last = inner.last().unwrap();
            assert!(
                last.contains(FOOTER),
                "{w}x{h}: last inner row must be the footer, got {last:?}\n{}",
                inner.join("\n")
            );
        }
    }

    #[tokio::test]
    async fn help_page_down_clamps_and_k_moves_up_at_once() {
        for (w, h) in SIZES {
            let (mut app, mut terminal) = help_open(w, h).await;
            for _ in 0..100 {
                press(&mut app, KeyCode::Char('j')).await;
                draw(&mut app, &mut terminal);
            }
            let end_scroll = app.help_state.as_ref().unwrap().scroll;
            let at_end = inner_rows(&draw(&mut app, &mut terminal), w, h);

            for _ in 0..5 {
                press(&mut app, KeyCode::PageDown).await;
                draw(&mut app, &mut terminal);
            }
            let after_page = inner_rows(&draw(&mut app, &mut terminal), w, h);
            assert_eq!(after_page, at_end, "{w}x{h}: page_down past the end stays");
            assert_eq!(app.help_state.as_ref().unwrap().scroll, end_scroll);

            press(&mut app, KeyCode::Char('k')).await;
            let after_k = inner_rows(&draw(&mut app, &mut terminal), w, h);
            assert_eq!(
                app.help_state.as_ref().unwrap().scroll,
                end_scroll - 1,
                "{w}x{h}: one k moves up at once"
            );
            assert_eq!(after_k[1..], at_end[..at_end.len() - 1], "{w}x{h}");
            assert!(!after_k.last().unwrap().contains(FOOTER), "{w}x{h}");
        }
    }

    #[tokio::test]
    async fn help_key_line_keeps_gap_for_long_keys() {
        for (w, h) in SIZES {
            let (mut app, mut terminal) = help_open(w, h).await;
            // #1398: the longest key (quit) is in the Global group, below
            // the first page, so read every page.
            let mut inner = inner_rows(&draw(&mut app, &mut terminal), w, h);
            for _ in 0..60 {
                press(&mut app, KeyCode::Char('j')).await;
                inner.extend(inner_rows(&draw(&mut app, &mut terminal), w, h));
            }
            let col = |row: &str, pat: &str| row[..row.find(pat).unwrap()].chars().count();
            let long = inner
                .iter()
                .find(|r| r.contains(LONG_KEY))
                .unwrap_or_else(|| panic!("{w}x{h}: quit row\n{}", inner.join("\n")));
            let at = col(long, "quit — ");
            assert!(
                long[..long.find("quit — ").unwrap()].ends_with(' '),
                "{w}x{h}: no gap before the description: {long:?}"
            );
            // Key column = longest key of this mode + 2.
            let longest = app
                .cached_mode_km
                .as_ref()
                .unwrap()
                .help_groups
                .iter()
                .flat_map(|g| &g.bindings)
                .map(|b| b.key.chars().count())
                .max()
                .unwrap();
            assert!(longest >= LONG_KEY.chars().count());
            let key_at = col(long, LONG_KEY);
            assert_eq!(at, key_at + longest + 2, "{w}x{h}: {long:?}");
            // Short keys line up in the same column.
            let short = inner.iter().find(|r| r.contains("Release notes")).unwrap();
            assert_eq!(col(short, "Release notes"), at, "{w}x{h}: {short:?}");
        }
    }
}
