use serde::{Deserialize, Serialize};

/// Classifies whether a session prompt expects code/tool work or a text-only answer.
///
/// Used by `RetryPolicy` to decide whether a hollow completion should be retried:
/// `Consultation` prompts that completed with a text response are already "done" and
/// should not be retried; `Work` prompts that completed hollow are retry candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionIntent {
    /// Prompt expects code changes, file modifications, or tool usage.
    #[default]
    Work,
    /// Prompt expects a text-only answer (explanation, summary, Q&A).
    Consultation,
}

/// Phrases that mark a modal / hypothetical question — e.g. "how would you fix X?".
/// Takes precedence over other checks so "how would you fix this?" is not
/// misclassified as Work.
const MODAL_QUESTION_MARKERS: &[&str] = &[
    "how would you",
    "what would you",
    "why would you",
    "how should i",
    "what should i",
    "would you recommend",
    "how does",
    "how do ",
    "how is ",
    "what is ",
    "what are ",
    "what does",
    "what do ",
    "why is ",
    "why does",
    "why do ",
    "when does",
    "when do ",
    "where does",
    "where do ",
];

/// Verbs that indicate the user wants an explanation or enumeration, not work.
const CONSULTATION_VERBS: &[&str] = &[
    "explain ",
    "describe ",
    "tell me ",
    "show me ",
    "list ",
    "summarize ",
    "summarise ",
    "clarify ",
];

/// Polite / imperative prefixes that typically wrap a work request.
/// Matched only at the start of the (normalized) prompt.
const POLITE_WORK_PREFIXES: &[&str] = &[
    "can you ",
    "could you ",
    "please ",
    "would you please ",
    "i need you to ",
    "i want you to ",
    "help me ",
    "let's ",
    "lets ",
];

/// Imperative verbs that indicate a work request when the prompt starts with them.
const WORK_VERBS: &[&str] = &[
    "fix",
    "add",
    "create",
    "build",
    "run",
    "write",
    "delete",
    "remove",
    "update",
    "refactor",
    "rename",
    "move",
    "modify",
    "change",
    "replace",
    "extract",
    "introduce",
    "generate",
    "setup",
    "configure",
    "deploy",
    "merge",
    "rebase",
    "squash",
    "test",
    "bump",
    "upgrade",
    "downgrade",
    "install",
    "uninstall",
    "implement",
    "apply",
    "revert",
    "patch",
    "migrate",
    "port",
    "rewrite",
    "optimize",
    "optimise",
    "cleanup",
    "commit",
    "push",
    "pull",
    "check",
    "verify",
    "validate",
    "scaffold",
    "wire",
    "wire up",
    "hook up",
    "enable",
    "disable",
    "flag",
    "investigate",
    "resolve",
    "debug",
    "inspect",
    "review",
    "rework",
    "handle",
    "set",
    "clear",
    "reset",
    "restart",
    "stop",
    "start",
    "spawn",
    "kill",
];

/// Question words that indicate consultation when they start the prompt.
const QUESTION_WORDS: &[&str] = &[
    "how", "why", "what", "when", "where", "who", "which", "does", "do", "is", "are", "can",
    "could", "would", "should",
];

/// Classify a session prompt as `Work` or `Consultation`.
///
/// Conservative default: unrecognized prompts classify as `Work`, so the retry
/// policy's existing behavior is preserved for anything we don't explicitly
/// detect as a question.
pub fn classify_intent(prompt: &str) -> SessionIntent {
    let normalized = prompt.trim().to_lowercase();

    if normalized.is_empty() {
        return SessionIntent::Work;
    }

    // Modal questions take precedence so "how would you fix this?" is not
    // classified by the "fix" work-verb rule below.
    if MODAL_QUESTION_MARKERS
        .iter()
        .any(|m| normalized.contains(m))
    {
        return SessionIntent::Consultation;
    }

    let after_polite = strip_polite_prefix(&normalized);
    if CONSULTATION_VERBS
        .iter()
        .any(|v| after_polite.starts_with(v))
    {
        return SessionIntent::Consultation;
    }

    if after_polite != normalized && starts_with_work_verb(after_polite) {
        return SessionIntent::Work;
    }

    if contains_issue_reference(&normalized)
        || contains_file_path(&normalized)
        || contains_shell_command(&normalized)
    {
        return SessionIntent::Work;
    }

    if starts_with_work_verb(&normalized) {
        return SessionIntent::Work;
    }

    if starts_with_question_word(&normalized) {
        return SessionIntent::Consultation;
    }

    if normalized.ends_with('?') {
        return SessionIntent::Consultation;
    }

    SessionIntent::Work
}

fn strip_polite_prefix(normalized: &str) -> &str {
    for prefix in POLITE_WORK_PREFIXES {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            return rest;
        }
    }
    normalized
}

fn starts_with_token(s: &str, token: &str) -> bool {
    match s.strip_prefix(token) {
        Some("") => true,
        Some(rest) => rest.starts_with(' '),
        None => false,
    }
}

fn starts_with_work_verb(s: &str) -> bool {
    WORK_VERBS.iter().any(|v| starts_with_token(s, v))
}

fn starts_with_question_word(s: &str) -> bool {
    QUESTION_WORDS.iter().any(|q| starts_with_token(s, q))
}

fn contains_issue_reference(s: &str) -> bool {
    // Matches "#123", "issue 123", "issue #123", "pr 123", "pr #123", "#123"
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '#'
            && let Some(&next) = chars.peek()
            && next.is_ascii_digit()
        {
            return true;
        }
    }
    s.contains("issue ") && has_number_after(s, "issue ")
        || s.contains("pr ") && has_number_after(s, "pr ")
}

fn has_number_after(s: &str, marker: &str) -> bool {
    if let Some(idx) = s.find(marker) {
        let rest = &s[idx + marker.len()..];
        let trimmed = rest.trim_start_matches('#').trim_start();
        trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
    } else {
        false
    }
}

fn contains_file_path(s: &str) -> bool {
    // Common code/doc extensions preceded by '.' — require at least one char
    // before the dot to avoid matching sentence-ending periods.
    const EXTENSIONS: &[&str] = &[
        ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".java", ".kt", ".swift", ".rb", ".php",
        ".c", ".cpp", ".h", ".hpp", ".cs", ".md", ".toml", ".yaml", ".yml", ".json", ".lock",
        ".sh", ".fish", ".zsh", ".bash",
    ];
    for ext in EXTENSIONS {
        if let Some(idx) = s.find(ext) {
            // Ensure the character before the extension is not whitespace or start-of-string.
            // i.e. there is a filename token attached to the extension.
            if idx == 0 {
                continue;
            }
            let prev = s.as_bytes()[idx - 1];
            if !prev.is_ascii_whitespace() && prev != b'.' {
                // Ensure the extension is at end-of-word (followed by space, punctuation, or EOS)
                let after = idx + ext.len();
                if after == s.len() {
                    return true;
                }
                let next = s.as_bytes()[after];
                if !next.is_ascii_alphanumeric() {
                    return true;
                }
            }
        }
    }
    // Obvious directory markers.
    s.contains("src/") || s.contains("./") || s.contains("../")
}

fn contains_shell_command(s: &str) -> bool {
    const COMMANDS: &[&str] = &[
        "cargo ", "npm ", "yarn ", "pnpm ", "git ", "bash ", "sh ", "make ", "docker ", "kubectl ",
        "rustc ", "rustup ", "brew ",
    ];
    COMMANDS.iter().any(|c| s.contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SessionIntent enum & serde ---

    #[test]
    fn default_intent_is_work() {
        assert_eq!(SessionIntent::default(), SessionIntent::Work);
    }

    #[test]
    fn intent_serializes_as_snake_case_work() {
        let json = serde_json::to_string(&SessionIntent::Work).unwrap();
        assert_eq!(json, r#""work""#);
    }

    #[test]
    fn intent_serializes_as_snake_case_consultation() {
        let json = serde_json::to_string(&SessionIntent::Consultation).unwrap();
        assert_eq!(json, r#""consultation""#);
    }

    #[test]
    fn intent_deserializes_from_snake_case() {
        let v: SessionIntent = serde_json::from_str(r#""consultation""#).unwrap();
        assert_eq!(v, SessionIntent::Consultation);
    }

    // --- Work classification ---

    #[test]
    fn classify_fix_bug_is_work() {
        assert_eq!(classify_intent("fix bug in login"), SessionIntent::Work);
    }

    #[test]
    fn classify_implement_issue_is_work() {
        assert_eq!(classify_intent("implement #42"), SessionIntent::Work);
    }

    #[test]
    fn classify_run_cargo_test_is_work() {
        assert_eq!(classify_intent("run cargo test"), SessionIntent::Work);
    }

    #[test]
    fn classify_add_error_handling_with_path_is_work() {
        assert_eq!(
            classify_intent("add error handling to parser.rs"),
            SessionIntent::Work
        );
    }

    #[test]
    fn classify_create_is_work() {
        assert_eq!(
            classify_intent("create a new function in src/foo.rs"),
            SessionIntent::Work
        );
    }

    #[test]
    fn classify_refactor_is_work() {
        assert_eq!(
            classify_intent("refactor the session module"),
            SessionIntent::Work
        );
    }

    #[test]
    fn classify_polite_imperative_can_you_fix_is_work() {
        assert_eq!(
            classify_intent("can you fix this?"),
            SessionIntent::Work,
            "polite imperative should be Work even with ?"
        );
    }

    #[test]
    fn classify_please_run_tests_is_work() {
        assert_eq!(classify_intent("please run the tests"), SessionIntent::Work);
    }

    #[test]
    fn classify_issue_reference_anywhere_is_work() {
        assert_eq!(
            classify_intent("work on issue #123 today"),
            SessionIntent::Work
        );
    }

    #[test]
    fn classify_file_path_anywhere_is_work() {
        assert_eq!(
            classify_intent("take a look at src/main.rs"),
            SessionIntent::Work
        );
    }

    #[test]
    fn classify_single_verb_is_work() {
        assert_eq!(classify_intent("fix"), SessionIntent::Work);
    }

    // --- Consultation classification ---

    #[test]
    fn classify_how_are_you_is_consultation() {
        assert_eq!(classify_intent("how are you?"), SessionIntent::Consultation);
    }

    #[test]
    fn classify_explain_flow_is_consultation() {
        assert_eq!(
            classify_intent("explain the auth flow"),
            SessionIntent::Consultation
        );
    }

    #[test]
    fn classify_what_does_this_do_is_consultation() {
        assert_eq!(
            classify_intent("what does this function do?"),
            SessionIntent::Consultation
        );
    }

    #[test]
    fn classify_list_dependencies_is_consultation() {
        assert_eq!(
            classify_intent("list the dependencies"),
            SessionIntent::Consultation
        );
    }

    #[test]
    fn classify_modal_how_would_you_fix_is_consultation() {
        assert_eq!(
            classify_intent("how would you fix this?"),
            SessionIntent::Consultation,
            "modal question beats work verb 'fix'"
        );
    }

    #[test]
    fn classify_describe_is_consultation() {
        assert_eq!(
            classify_intent("describe the retry policy"),
            SessionIntent::Consultation
        );
    }

    #[test]
    fn classify_tell_me_about_is_consultation() {
        assert_eq!(
            classify_intent("tell me about session retry"),
            SessionIntent::Consultation
        );
    }

    #[test]
    fn classify_why_is_it_failing_is_consultation() {
        assert_eq!(
            classify_intent("why is it failing?"),
            SessionIntent::Consultation
        );
    }

    #[test]
    fn classify_what_is_the_status_is_consultation() {
        assert_eq!(
            classify_intent("what is the status?"),
            SessionIntent::Consultation
        );
    }

    #[test]
    fn classify_trailing_question_mark_is_consultation() {
        assert_eq!(
            classify_intent("the retry policy?"),
            SessionIntent::Consultation
        );
    }

    // --- Edge cases ---

    #[test]
    fn classify_empty_prompt_is_work_default() {
        assert_eq!(classify_intent(""), SessionIntent::Work);
    }

    #[test]
    fn classify_whitespace_only_is_work_default() {
        assert_eq!(classify_intent("    \n\t"), SessionIntent::Work);
    }

    #[test]
    fn classify_is_case_insensitive() {
        assert_eq!(classify_intent("FIX BUG"), SessionIntent::Work);
        assert_eq!(
            classify_intent("HOW DOES X WORK?"),
            SessionIntent::Consultation
        );
    }

    #[test]
    fn classify_please_explain_is_consultation() {
        assert_eq!(
            classify_intent("please explain the hollow retry"),
            SessionIntent::Consultation,
            "polite prefix + explanatory verb → consultation"
        );
    }

    // --- Accuracy sweep (>90% on a 20+ prompt suite per acceptance criteria) ---

    #[test]
    fn classifier_accuracy_on_spec_corpus_is_above_90_percent() {
        let work: &[&str] = &[
            "fix bug in login",
            "implement #42",
            "run cargo test",
            "add error handling to parser.rs",
            "refactor the session module",
            "create a new unit test for retry",
            "delete the unused helper",
            "update Cargo.toml dependencies",
            "apply the migration",
            "can you fix this?",
            "please commit and push",
            "work on issue #123",
            "wire up the new adapter",
            "resolve merge conflicts in src/main.rs",
        ];
        let consultation: &[&str] = &[
            "how are you?",
            "explain the auth flow",
            "what does this function do?",
            "list the dependencies",
            "how would you fix this?",
            "describe the retry policy",
            "tell me about session retry",
            "why is it failing?",
            "what is the status?",
            "summarize the recent changes",
        ];

        let mut correct = 0;
        let mut total = 0;
        for p in work {
            total += 1;
            if classify_intent(p) == SessionIntent::Work {
                correct += 1;
            }
        }
        for p in consultation {
            total += 1;
            if classify_intent(p) == SessionIntent::Consultation {
                correct += 1;
            }
        }
        let accuracy = correct as f64 / total as f64;
        assert!(
            accuracy > 0.90,
            "accuracy {:.2}% on {} prompts (correct: {})",
            accuracy * 100.0,
            total,
            correct
        );
    }
}
