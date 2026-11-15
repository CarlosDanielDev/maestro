mod fakes;

use super::*;
use crate::templates::TemplateError;
use fakes::{FakeRules, RecursiveIncludeRules, TerminatingIncludeRules};

#[test]
fn tokenize_empty_string_yields_no_tokens() {
    let toks = tokenize("", "x.md").expect("ok");
    assert!(toks.is_empty());
}

#[test]
fn tokenize_plain_text_yields_single_text_token() {
    let toks = tokenize("Hello world\nSecond line", "x.md").expect("ok");
    assert_eq!(toks.len(), 1);
    assert!(matches!(&toks[0], Token::Text(t) if t == "Hello world\nSecond line"));
}

#[test]
fn tokenize_argument_less_placeholder() {
    let toks = tokenize("{{SUBAGENT_LIST}}", "x.md").expect("ok");
    assert_eq!(toks.len(), 1);
    match &toks[0] {
        Token::Placeholder { name, args, offset } => {
            assert_eq!(name, "SUBAGENT_LIST");
            assert!(args.is_empty());
            assert_eq!(*offset, 0);
        }
        _ => panic!("expected placeholder"),
    }
}

#[test]
fn tokenize_placeholder_with_args() {
    let toks = tokenize(
        r#"{{INVOKE_SUBAGENT name="subagent-qa" prompt="design tests"}}"#,
        "x.md",
    )
    .expect("ok");
    match &toks[0] {
        Token::Placeholder { name, args, .. } => {
            assert_eq!(name, "INVOKE_SUBAGENT");
            assert_eq!(args.get("name").map(String::as_str), Some("subagent-qa"));
            assert_eq!(args.get("prompt").map(String::as_str), Some("design tests"));
        }
        _ => panic!("expected placeholder"),
    }
}

#[test]
fn tokenize_handles_escaped_quote_in_value() {
    let toks = tokenize(
        r#"{{INVOKE_SUBAGENT name="qa" prompt="say \"hi\""}}"#,
        "x.md",
    )
    .expect("ok");
    match &toks[0] {
        Token::Placeholder { args, .. } => {
            assert_eq!(args.get("prompt").map(String::as_str), Some(r#"say "hi""#));
        }
        _ => panic!("expected placeholder"),
    }
}

#[test]
fn tokenize_unterminated_placeholder_returns_error() {
    let result = tokenize(r#"Before {{INVOKE_SUBAGENT name="x""#, "x.md");
    match result {
        Err(TemplateError::UnterminatedPlaceholder { offset, .. }) => {
            assert_eq!(offset, 7);
        }
        other => panic!("expected UnterminatedPlaceholder, got {other:?}"),
    }
}

#[test]
fn tokenize_newline_inside_placeholder_returns_error() {
    let result = tokenize("{{INVOKE_SUBAGENT name=\"x\"\nprompt=\"y\"}}", "x.md");
    assert!(matches!(
        result,
        Err(TemplateError::UnterminatedPlaceholder { .. })
    ));
}

#[test]
fn tokenize_records_byte_offset_for_each_placeholder() {
    let toks = tokenize("abc{{SUBAGENT_LIST}}def{{SUBAGENT_LIST}}", "x.md").expect("ok");
    let offsets: Vec<usize> = toks
        .iter()
        .filter_map(|t| match t {
            Token::Placeholder { offset, .. } => Some(*offset),
            _ => None,
        })
        .collect();
    assert_eq!(offsets, vec![3, 23]);
}

#[test]
fn render_plain_text_passes_through() {
    let out = render("Hello, world!", &FakeRules).expect("ok");
    assert_eq!(out, "Hello, world!");
}

#[test]
fn render_invoke_subagent_expands() {
    let out = render(
        r#"{{INVOKE_SUBAGENT name="subagent-qa" prompt="design tests"}}"#,
        &FakeRules,
    )
    .expect("ok");
    assert_eq!(out, "[INVOKE name=subagent-qa prompt=design tests]");
}

#[test]
fn render_hook_gate_expands() {
    let out = render(
        r#"{{HOOK_GATE script=".maestro/hooks/implement-gates.sh" args="701"}}"#,
        &FakeRules,
    )
    .expect("ok");
    assert_eq!(
        out,
        "[HOOK script=.maestro/hooks/implement-gates.sh args=701]"
    );
}

#[test]
fn render_include_expands_with_path() {
    let out = render(r#"{{INCLUDE path="core/premises.md"}}"#, &FakeRules).expect("ok");
    assert_eq!(out, "[INCLUDE path=core/premises.md]");
}

#[test]
fn render_subagent_list_expands() {
    let out = render("{{SUBAGENT_LIST}}", &FakeRules).expect("ok");
    assert_eq!(out, "[SUBAGENT_LIST]");
}

#[test]
fn render_skill_expands() {
    let out = render(r#"{{SKILL name="project-patterns"}}"#, &FakeRules).expect("ok");
    assert_eq!(out, "[SKILL name=project-patterns]");
}

#[test]
fn render_skill_with_hyphenated_name() {
    let out = render(r#"{{SKILL name="security-patterns"}}"#, &FakeRules).expect("ok");
    assert_eq!(out, "[SKILL name=security-patterns]");
}

#[test]
fn render_multi_placeholder_template_preserves_order() {
    let input = "Pre\n{{SUBAGENT_LIST}}\nMid\n{{SKILL name=\"project-patterns\"}}\nEnd";
    let out = render(input, &FakeRules).expect("ok");
    assert_eq!(
        out,
        "Pre\n[SUBAGENT_LIST]\nMid\n[SKILL name=project-patterns]\nEnd"
    );
}

#[test]
fn render_utf8_text_passes_through() {
    let out = render("Héllo 🌍\n{{SUBAGENT_LIST}}", &FakeRules).expect("ok");
    assert_eq!(out, "Héllo 🌍\n[SUBAGENT_LIST]");
}

#[test]
fn render_utf8_in_arg_value() {
    let out = render(
        r#"{{INVOKE_SUBAGENT name="ünicode-agent" prompt="résumé"}}"#,
        &FakeRules,
    )
    .expect("ok");
    assert_eq!(out, "[INVOKE name=ünicode-agent prompt=résumé]");
}

#[test]
fn render_unknown_placeholder_returns_err() {
    let result = render(r#"{{BOGUS_PLACEHOLDER name="x"}}"#, &FakeRules);
    match result {
        Err(TemplateError::UnknownPlaceholder { name, .. }) => {
            assert_eq!(name, "BOGUS_PLACEHOLDER");
        }
        other => panic!("expected UnknownPlaceholder, got {other:?}"),
    }
}

#[test]
fn render_unknown_placeholder_records_offset() {
    let result = render("prefix text {{UNKNOWN}}suffix", &FakeRules);
    match result {
        Err(TemplateError::UnknownPlaceholder { offset, .. }) => {
            assert_eq!(offset, 12);
        }
        other => panic!("expected UnknownPlaceholder, got {other:?}"),
    }
}

#[test]
fn render_unknown_placeholder_aborts_after_partial_render() {
    let result = render("{{SUBAGENT_LIST}} then {{TOTALLY_BOGUS}}", &FakeRules);
    match result {
        Err(TemplateError::UnknownPlaceholder { name, .. }) => {
            assert_eq!(name, "TOTALLY_BOGUS");
        }
        other => panic!("expected UnknownPlaceholder, got {other:?}"),
    }
}

#[test]
fn render_null_rules_invoke_subagent_fails_closed() {
    let result = render(
        r#"{{INVOKE_SUBAGENT name="qa" prompt="test"}}"#,
        crate::templates::null_rules(),
    );
    match result {
        Err(TemplateError::UnsupportedByProvider { name, .. }) => {
            assert_eq!(name, "INVOKE_SUBAGENT");
        }
        other => panic!("expected UnsupportedByProvider, got {other:?}"),
    }
}

#[test]
fn render_null_rules_hook_gate_fails_closed() {
    let result = render(
        r#"{{HOOK_GATE script="x.sh" args=""}}"#,
        crate::templates::null_rules(),
    );
    assert!(matches!(
        result,
        Err(TemplateError::UnsupportedByProvider { ref name, .. }) if name == "HOOK_GATE"
    ));
}

#[test]
fn render_null_rules_include_fails_closed() {
    let result = render(
        r#"{{INCLUDE path="core/x.md"}}"#,
        crate::templates::null_rules(),
    );
    assert!(matches!(
        result,
        Err(TemplateError::UnsupportedByProvider { ref name, .. }) if name == "INCLUDE"
    ));
}

#[test]
fn render_null_rules_subagent_list_fails_closed() {
    let result = render("{{SUBAGENT_LIST}}", crate::templates::null_rules());
    assert!(matches!(
        result,
        Err(TemplateError::UnsupportedByProvider { ref name, .. }) if name == "SUBAGENT_LIST"
    ));
}

#[test]
fn render_null_rules_skill_fails_closed() {
    let result = render(
        r#"{{SKILL name="project-patterns"}}"#,
        crate::templates::null_rules(),
    );
    assert!(matches!(
        result,
        Err(TemplateError::UnsupportedByProvider { ref name, .. }) if name == "SKILL"
    ));
}

#[test]
fn render_hook_gate_preserves_whitespace_in_args() {
    let out = render(
        r#"{{HOOK_GATE script="gate.sh" args="  arg1   arg2  "}}"#,
        &FakeRules,
    )
    .expect("ok");
    assert_eq!(out, "[HOOK script=gate.sh args=  arg1   arg2  ]");
}

#[test]
fn render_missing_required_arg_returns_invalid_placeholder() {
    let result = render("{{INCLUDE}}", &FakeRules);
    match result {
        Err(TemplateError::InvalidPlaceholder { name, .. }) => {
            assert_eq!(name, "INCLUDE");
        }
        other => panic!("expected InvalidPlaceholder, got {other:?}"),
    }
}

#[test]
fn render_include_at_max_depth_minus_one_succeeds() {
    let rules = TerminatingIncludeRules {
        cap: MAX_INCLUDE_DEPTH - 1,
        counter: std::sync::atomic::AtomicUsize::new(0),
    };
    let out = render(r#"{{INCLUDE path="x.md"}}"#, &rules).expect("should terminate");
    assert_eq!(out, "TERMINAL");
}

#[test]
fn tokenize_rejects_raw_control_byte_in_arg_value() {
    // Inject a literal newline (0x0A) inside the value by using \\n escape would
    // be allowed; here we attempt a raw control byte via the tab+formfeed range.
    // Using ASCII bell (0x07) which is not handled by any escape branch.
    let input = "{{INVOKE_SUBAGENT name=\"qa\" prompt=\"hi\x07injected\"}}";
    let result = tokenize(input, "x.md");
    match result {
        Err(TemplateError::InvalidPlaceholder { reason, .. }) => {
            assert!(reason.contains("control byte"), "{reason}");
        }
        other => panic!("expected InvalidPlaceholder for control byte, got {other:?}"),
    }
}

#[test]
fn render_include_at_max_depth_returns_cycle_error() {
    let result = render(r#"{{INCLUDE path="x.md"}}"#, &RecursiveIncludeRules);
    assert!(matches!(result, Err(TemplateError::IncludeCycle { .. })));
}
