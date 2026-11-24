//! Failure digest lines (§02.23 "Failure digest", #1291): one parser
//! output (`gates::digest::digest`), drawn for a local gate and a CI log.
//! Summary, then FAILURES BY FILE in output order; the first failure is
//! open, the rest are one line each with `:line:col`.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::gates::digest::{Digest, FailureKind};
use crate::icons::{self, IconId};
use crate::tui::theme::Theme;
use crate::util::sanitize::sanitize_log;

/// Shown instead of a digest when no parser matched.
pub const NO_PARSER: &str = "no parser for this output · raw";

/// Lines for `d`, each starting with `indent`. Every external string goes
/// through `sanitize_log` (control and bidi characters); the digest text
/// is already masked.
pub fn failure_lines(d: &Digest, indent: &str, theme: &Theme) -> Vec<Line<'static>> {
    let muted = Style::default().fg(theme.text_muted);
    if d.is_empty() {
        return vec![Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(NO_PARSER, muted),
        ])];
    }
    let mut out = vec![
        Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(
                format!("{} {}", icons::get(IconId::ChipErrored), d.summary),
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled("FAILURES BY FILE", muted.add_modifier(Modifier::BOLD)),
        ]),
    ];
    let mut first = true;
    for group in &d.groups {
        out.push(Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(
                sanitize_log(&group.path),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {} failing", group.items.len()), muted),
        ]));
        for item in &group.items {
            let (glyph, color) = match item.kind {
                FailureKind::Warning => ("!", theme.accent_warning),
                FailureKind::Test | FailureKind::Error => {
                    (icons::get(IconId::ChipErrored), theme.accent_error)
                }
            };
            let mut row = vec![
                Span::raw(format!("{indent} {} ", if first { "▾" } else { "▸" })),
                Span::styled(format!("{glyph} "), Style::default().fg(color)),
                Span::styled(
                    sanitize_log(&item.title),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if let Some(tag) = &item.tag {
                row.push(Span::styled(format!("  {}", sanitize_log(tag)), muted));
            }
            row.push(Span::styled(
                format!("  :{}:{}", item.line, item.col),
                muted,
            ));
            out.push(Line::from(row));
            if first {
                out.extend(item.body.iter().map(|l| {
                    Line::from(vec![
                        Span::raw(format!("{indent}     ")),
                        Span::styled(sanitize_log(l), Style::default().fg(theme.text_secondary)),
                    ])
                }));
                first = false;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gates::digest::digest;

    fn text(lines: &[Line]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    const TWO_TESTS: &str = "\
---- a::one stdout ----
thread 'a::one' panicked at src/a.rs:3:5:
first body

---- a::two stdout ----
thread 'a::two' panicked at src/a.rs:9:5:
second body

test result: FAILED. 0 passed; 2 failed; 0 ignored
";

    #[test]
    fn first_failure_is_open_the_rest_closed() {
        let out = text(&failure_lines(&digest(TWO_TESTS), "  ", &Theme::dark()));
        assert!(out.contains("▾"), "{out}");
        assert!(out.contains("first body"), "first item open:\n{out}");
        assert!(out.contains("a::two  :9:5"), "second item one line:\n{out}");
        assert!(!out.contains("second body"), "second item closed:\n{out}");
    }

    #[test]
    fn no_parser_says_so() {
        let out = text(&failure_lines(&digest("boom\n"), "", &Theme::dark()));
        assert_eq!(out, NO_PARSER);
    }

    #[test]
    fn body_is_sanitized() {
        let d =
            digest("---- t stdout ----\nthread 't' panicked at src/x.rs:1:1:\nbad \u{202E}text\n");
        let out = text(&failure_lines(&d, "", &Theme::dark()));
        assert!(!out.contains('\u{202E}'));
    }
}
