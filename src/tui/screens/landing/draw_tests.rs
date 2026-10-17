use super::should_emit_styles_from_env;

#[test]
fn styles_are_disabled_when_no_color_is_present() {
    assert!(!should_emit_styles_from_env(|_| None, true));
}

#[test]
fn styles_are_disabled_for_dumb_term() {
    assert!(!should_emit_styles_from_env(
        |key| (key == "TERM").then(|| "dumb".to_string()),
        false
    ));
}

#[test]
fn styles_are_enabled_for_normal_term() {
    assert!(should_emit_styles_from_env(
        |key| (key == "TERM").then(|| "xterm-256color".to_string()),
        false
    ));
}

/// The footer carries the quit and notes hints. At 60x20 the grown menu
/// leaves no room for the blank line above the footer; the footer must still
/// land inside the area.
#[test]
fn footer_stays_inside_the_area_at_60x20_when_the_menu_grows() {
    use super::welcome_layout;
    use ratatui::layout::Rect;

    let area = Rect::new(0, 0, 60, 20);
    let layout = welcome_layout(area);

    assert!(
        layout.footer_y < area.y + area.height,
        "footer at row {} is off the 20-row area",
        layout.footer_y
    );
}
