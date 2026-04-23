//! Inline unit tests for `wizard_fields` (split into a sibling file so
//! the module itself stays under the project's 400-LOC cap).

use super::*;

#[test]
fn single_line_constructor_starts_empty() {
    let field = TextAreaField::single_line();
    assert!(field.is_single_line());
    assert_eq!(field.text(), "");
}

#[test]
fn multi_line_constructor_starts_empty() {
    let field = TextAreaField::multi_line();
    assert!(!field.is_single_line());
    assert_eq!(field.text(), "");
}

#[test]
fn insert_sanitized_strips_c0_control_chars_except_newline_and_tab() {
    let mut field = TextAreaField::multi_line();
    field.insert_sanitized("a\x00b\x01c\t\nd");
    assert_eq!(field.text(), "abc\t\nd");
}

#[test]
fn insert_sanitized_strips_del_and_c1_range() {
    let mut field = TextAreaField::multi_line();
    field.insert_sanitized("a\x7fb\u{80}c\u{9f}d");
    assert_eq!(field.text(), "abcd");
}

#[test]
fn insert_sanitized_strips_ansi_csi_escape_sequences() {
    let mut field = TextAreaField::single_line();
    field.insert_sanitized("\x1b[31mred\x1b[0m");
    assert!(!field.text().contains('\x1b'));
    assert_eq!(field.text(), "[31mred[0m");
}

#[test]
fn insert_sanitized_single_line_collapses_newline_to_space() {
    let mut field = TextAreaField::single_line();
    field.insert_sanitized("line1\nline2");
    assert_eq!(field.text(), "line1 line2");
}

#[test]
fn insert_sanitized_single_line_collapses_cr_to_space() {
    let mut field = TextAreaField::single_line();
    field.insert_sanitized("a\rb");
    assert_eq!(field.text(), "a b");
}

#[test]
fn insert_sanitized_single_line_collapses_crlf_to_single_space() {
    let mut field = TextAreaField::single_line();
    field.insert_sanitized("a\r\nb");
    assert_eq!(field.text(), "a b");
}

#[test]
fn insert_sanitized_multi_line_preserves_newlines() {
    let mut field = TextAreaField::multi_line();
    field.insert_sanitized("line1\nline2");
    assert_eq!(field.text(), "line1\nline2");
}

#[test]
fn insert_sanitized_empty_input_is_noop() {
    let mut field = TextAreaField::multi_line();
    field.insert_sanitized("\x1b\x1b");
    assert_eq!(field.text(), "");
}

#[test]
fn insert_sanitized_strips_bidi_overrides() {
    // CVE-2021-42574 — Trojan Source. LRO/RLO/PDF and isolates.
    let mut field = TextAreaField::multi_line();
    field.insert_sanitized("a\u{202D}b\u{202E}c\u{2066}d\u{2069}e");
    assert_eq!(field.text(), "abcde");
}

#[test]
fn insert_sanitized_strips_unicode_line_separators_multi_line() {
    let mut field = TextAreaField::multi_line();
    field.insert_sanitized("a\u{2028}b\u{2029}c");
    // LS/PS are stripped even on multi-line fields (they're not ASCII
    // newlines — nothing in the codebase emits them intentionally).
    assert_eq!(field.text(), "abc");
}

#[test]
fn insert_sanitized_collapses_unicode_line_separators_single_line() {
    let mut field = TextAreaField::single_line();
    field.insert_sanitized("a\u{2028}b\u{2029}c");
    // On single-line fields LS/PS are collapsed to space (same
    // treatment as \n / \r) before the filter runs.
    assert_eq!(field.text(), "a b c");
}

#[test]
fn insert_sanitized_strips_byte_order_mark() {
    let mut field = TextAreaField::single_line();
    field.insert_sanitized("\u{FEFF}hello");
    assert_eq!(field.text(), "hello");
}

#[test]
fn set_text_replaces_full_content_single_line() {
    let mut field = TextAreaField::single_line();
    field.insert_sanitized("old");
    field.set_text("new title");
    assert_eq!(field.text(), "new title");
}

#[test]
fn set_text_replaces_full_content_multi_line() {
    let mut field = TextAreaField::multi_line();
    field.insert_sanitized("old");
    field.set_text("line1\nline2\nline3");
    assert_eq!(field.text(), "line1\nline2\nline3");
}

#[test]
fn set_text_collapses_newlines_on_single_line() {
    let mut field = TextAreaField::single_line();
    field.set_text("a\nb");
    assert_eq!(field.text(), "a b");
}

#[test]
fn text_round_trips_set_and_get() {
    let mut field = TextAreaField::multi_line();
    field.set_text("alpha\nbeta\ngamma");
    assert_eq!(field.text(), "alpha\nbeta\ngamma");
}

#[test]
fn set_text_empty_leaves_one_empty_line() {
    let mut field = TextAreaField::multi_line();
    field.set_text("something");
    field.set_text("");
    assert_eq!(field.text(), "");
}

#[test]
fn wizard_fields_focus_next_cycles_forward() {
    let mut wf = WizardFields::new(vec![
        TextAreaField::single_line(),
        TextAreaField::single_line(),
        TextAreaField::single_line(),
    ]);
    assert_eq!(wf.focus(), 0);
    wf.focus_next();
    assert_eq!(wf.focus(), 1);
    wf.focus_next();
    assert_eq!(wf.focus(), 2);
    wf.focus_next();
    assert_eq!(wf.focus(), 0);
}

#[test]
fn wizard_fields_focus_prev_cycles_backward() {
    let mut wf = WizardFields::new(vec![
        TextAreaField::single_line(),
        TextAreaField::single_line(),
        TextAreaField::single_line(),
    ]);
    wf.focus_prev();
    assert_eq!(wf.focus(), 2);
}

#[test]
fn wizard_fields_empty_focus_ops_are_noops() {
    let mut wf = WizardFields::empty();
    wf.focus_next();
    wf.focus_prev();
    assert_eq!(wf.focus(), 0);
    assert!(wf.focused_mut().is_none());
}

#[test]
fn refresh_focus_styles_sets_focused_reversed_others_hidden() {
    let mut wf = WizardFields::new(vec![
        TextAreaField::multi_line(),
        TextAreaField::multi_line(),
    ]);
    wf.refresh_focus_styles();
    assert!(
        wf.fields[0]
            .area
            .cursor_style()
            .add_modifier
            .contains(Modifier::REVERSED)
    );
    assert!(
        wf.fields[1]
            .area
            .cursor_style()
            .add_modifier
            .contains(Modifier::HIDDEN)
    );
}

#[test]
fn refresh_focus_styles_after_focus_change() {
    let mut wf = WizardFields::new(vec![
        TextAreaField::multi_line(),
        TextAreaField::multi_line(),
    ]);
    wf.refresh_focus_styles();
    wf.focus_next();
    wf.refresh_focus_styles();
    assert!(
        wf.fields[0]
            .area
            .cursor_style()
            .add_modifier
            .contains(Modifier::HIDDEN)
    );
    assert!(
        wf.fields[1]
            .area
            .cursor_style()
            .add_modifier
            .contains(Modifier::REVERSED)
    );
}
