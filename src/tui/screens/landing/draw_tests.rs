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
