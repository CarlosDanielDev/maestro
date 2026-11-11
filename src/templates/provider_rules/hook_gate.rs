//! Validation for `{{HOOK_GATE script="..." args="..."}}` values (TPL-1).
//!
//! Shared by every provider `hook_gate` impl; the rendered output is a
//! `bash .maestro/hooks/<script> <args>` line, so both fields are allowlisted
//! before formatting (CWE-88). Lives in its own module to keep `mod.rs`
//! under the 400-line cap.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::templates::TemplateError;

/// Validate `HOOK_GATE` `script`/`args` before they are rendered into a
/// `bash .maestro/hooks/<script> <args>` line (TPL-1, CWE-88 — the one
/// placeholder not sandboxed by `read_sandboxed`).
///
/// - `script` must match `^[A-Za-z0-9._-]+\.(sh|py)$` — a bare hook filename
///   with no separators, so the rendered command cannot leave
///   `.maestro/hooks/`.
/// - `args` must contain only alphanumerics, space, or `_ . = $ / - { } : +`.
///   That covers the real values in the canonical templates (`$ISSUE_NUMBER`,
///   `${DIRTY_TREE_ACTION:+--dirty-tree-action=...}`) while rejecting every
///   shell separator and substitution operator (`;`, `|`, `&`, `(`, `)`,
///   `<`, `>`, backtick, quotes, backslash), newlines, and control bytes.
///
/// Errors are [`TemplateError::InvalidPlaceholder`] named `HOOK_GATE`. The
/// rules layer has no render position, so `offset` is 0 and `source_path` is
/// empty; the reason names the offending value.
pub(super) fn validate_hook_gate(script: &str, args: &str) -> Result<(), TemplateError> {
    let invalid = |reason: String| TemplateError::InvalidPlaceholder {
        name: "HOOK_GATE".to_string(),
        offset: 0,
        source_path: String::new(),
        reason,
    };

    let stem = script
        .strip_suffix(".sh")
        .or_else(|| script.strip_suffix(".py"));
    let script_ok = stem.is_some_and(|stem| {
        !stem.is_empty()
            && stem
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    });
    if !script_ok {
        return Err(invalid(format!(
            "script `{script}` must match ^[A-Za-z0-9._-]+\\.(sh|py)$ \
             (a bare hook filename ending in .sh or .py)"
        )));
    }

    let args_ok = args.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(
                c,
                ' ' | '_' | '.' | '=' | '$' | '/' | '-' | '{' | '}' | ':' | '+'
            )
    });
    if !args_ok {
        return Err(invalid(format!(
            "args `{args}` must contain only alphanumerics, space, or \
             one of `_ . = $ / - {{ }} : +`"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_hook_gate_rejects_parent_dir_script() {
        let err = validate_hook_gate("../evil.sh", "").unwrap_err();
        assert!(
            matches!(err, TemplateError::InvalidPlaceholder { .. }),
            "expected InvalidPlaceholder, got {err:?}"
        );
    }

    #[test]
    fn validate_hook_gate_rejects_shell_injection_args() {
        let err = validate_hook_gate("a.sh", "; rm -rf /").unwrap_err();
        assert!(
            matches!(err, TemplateError::InvalidPlaceholder { .. }),
            "expected InvalidPlaceholder, got {err:?}"
        );
    }

    #[test]
    fn validate_hook_gate_accepts_canonical_args() {
        validate_hook_gate("implement-gates.sh", "$ISSUE_NUMBER").expect("ok");
    }

    #[test]
    fn validate_hook_gate_accepts_canonical_dirty_tree_action_expansion() {
        // Exact `args` from .maestro/templates/commands/implement.md — the
        // `${VAR:+...}` form is why `{ } : +` joined the allowed set.
        validate_hook_gate(
            "implement-gates.sh",
            "$ISSUE_NUMBER ${DIRTY_TREE_ACTION:+--dirty-tree-action=$DIRTY_TREE_ACTION}",
        )
        .expect("ok");
    }

    #[test]
    fn validate_hook_gate_rejects_script_with_path_separator() {
        assert!(validate_hook_gate(".maestro/hooks/gates.sh", "").is_err());
    }

    #[test]
    fn validate_hook_gate_rejects_wrong_or_missing_extension() {
        assert!(validate_hook_gate("gates", "").is_err());
        assert!(validate_hook_gate("gates.txt", "").is_err());
        assert!(validate_hook_gate("gates.SH", "").is_err());
        assert!(validate_hook_gate(".sh", "").is_err());
        assert!(validate_hook_gate("", "").is_err());
    }

    #[test]
    fn validate_hook_gate_accepts_python_and_dotted_stems() {
        validate_hook_gate("parse_gatekeeper_report.py", "").expect("ok");
        validate_hook_gate("v1.2-gate_name.sh", "").expect("ok");
    }
}
