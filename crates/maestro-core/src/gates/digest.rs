//! Failure digest for gate and CI output (#1291, §02.23 "Failure digest").

use std::sync::LazyLock;

use regex::Regex;

/// Parsed failures of one output, grouped by file in output order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Digest {
    /// `3 failing · 412 passed`, `1 error · 2 warnings` (the view adds the
    /// glyph); empty when no parser matched.
    pub summary: String,
    pub groups: Vec<FileGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGroup {
    pub path: String,
    pub items: Vec<Failure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// Test name, or the diagnostic's message.
    pub title: String,
    /// Lint name or error code (`clippy::needless-return`, `E0308`).
    pub tag: Option<String>,
    pub line: u32,
    pub col: u32,
    pub kind: FailureKind,
    /// The lines under the title, with the tool's own indentation.
    pub body: Vec<String>,
}

/// `Test` and `Error` draw `✗`, `Warning` draws `!`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Test,
    Error,
    Warning,
}

impl Digest {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

/// Parse `text` (already secret-masked) into a digest. Rules, in order:
/// cargo test blocks (`---- name stdout ----` + `panicked at path:l:c`),
/// then rustc / clippy diagnostics (`error|warning: …` + `--> path:l:c`).
/// No match gives an empty digest; the caller shows the raw text.
pub fn digest(text: &str) -> Digest {
    let lines: Vec<&str> = text.lines().map(strip_ci_prefix).collect();
    let mut found = cargo_test_failures(&lines);
    found.extend(rustc_diagnostics(&lines));
    found.sort_by_key(|(at, _, _)| *at);

    let mut groups: Vec<FileGroup> = Vec::new();
    for (_, path, failure) in found {
        let group = match groups.iter_mut().position(|g| g.path == path) {
            Some(i) => &mut groups[i],
            None => {
                groups.push(FileGroup {
                    path,
                    items: Vec::new(),
                });
                groups.last_mut().expect("just pushed")
            }
        };
        let dup = group.items.iter().any(|f| {
            (f.line, f.col, f.kind, &f.title)
                == (failure.line, failure.col, failure.kind, &failure.title)
        });
        if !dup {
            group.items.push(failure);
        }
    }
    Digest {
        summary: summary(&lines, &groups),
        groups,
    }
}

type Found = (usize, String, Failure);

// Compiled once: the Gates view calls `digest` on every frame.
static CI_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| re(r"^[^\t]*\s[^\t]*?\s\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?Z "));
static TEST_HEAD: LazyLock<Regex> = LazyLock::new(|| re(r"^---- (\S+) stdout ----$"));
static PANIC: LazyLock<Regex> =
    LazyLock::new(|| re(r"panicked at (?:'(.*)', )?([^\s:]+):(\d+):(\d+):?$"));
static DIAG_HEAD: LazyLock<Regex> = LazyLock::new(|| re(r"^(error|warning)(?:\[(E\d+)\])?: (.+)$"));
static ARROW: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*--> (.+):(\d+):(\d+)$"));
static LINT: LazyLock<Regex> =
    LazyLock::new(|| re(r"#\[(?:warn|deny|forbid)\(([\w:-]+)\)\]|`-D ([\w:-]+)` implied"));
static TEST_RESULT: LazyLock<Regex> =
    LazyLock::new(|| re(r"^test result: FAILED\. (\d+) passed; (\d+) failed"));

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static digest regex")
}

/// `gh run view --log-failed` lines start `job<TAB>step<TAB>timestamp `;
/// after `sanitize_for_terminal` the tabs are spaces, so match up to the
/// timestamp either way.
fn strip_ci_prefix(line: &str) -> &str {
    CI_PREFIX.find(line).map_or(line, |m| &line[m.end()..])
}

fn cargo_test_failures(lines: &[&str]) -> Vec<Found> {
    let (head, panic) = (&*TEST_HEAD, &*PANIC);
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(name) = head.captures(lines[i].trim()).map(|c| c[1].to_string()) else {
            i += 1;
            continue;
        };
        let start = i;
        i += 1;
        let end = (i..lines.len())
            .find(|&j| {
                let l = lines[j].trim_end();
                l.starts_with("---- ") || l == "failures:" || l.starts_with("test result:")
            })
            .unwrap_or(lines.len());
        let block = &lines[i..end];
        i = end;
        let Some((at, caps)) = block
            .iter()
            .enumerate()
            .find_map(|(k, l)| panic.captures(l).map(|c| (k, c)))
        else {
            continue;
        };
        let mut body: Vec<String> = caps
            .get(1)
            .map(|m| vec![m.as_str().to_string()])
            .unwrap_or_default();
        body.extend(
            block[at + 1..]
                .iter()
                .filter(|l| !l.starts_with("note: run with `RUST_BACKTRACE"))
                .map(|l| l.to_string()),
        );
        while body.last().is_some_and(|l| l.trim().is_empty()) {
            body.pop();
        }
        out.push((
            start,
            caps[2].to_string(),
            Failure {
                title: name,
                tag: None,
                line: caps[3].parse().unwrap_or(0),
                col: caps[4].parse().unwrap_or(0),
                kind: FailureKind::Test,
                body,
            },
        ));
    }
    out
}

fn rustc_diagnostics(lines: &[&str]) -> Vec<Found> {
    let (head, arrow, lint) = (&*DIAG_HEAD, &*ARROW, &*LINT);
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(caps) = head.captures(line) else {
            continue;
        };
        let end = (i + 1..lines.len())
            .find(|&j| lines[j].trim().is_empty() || head.is_match(lines[j]))
            .unwrap_or(lines.len());
        let block = &lines[i + 1..end];
        // A header with no location is a summary line (`could not compile`).
        let Some(loc) = block.iter().find_map(|l| arrow.captures(l)) else {
            continue;
        };
        let tag = caps.get(2).map(|m| m.as_str().to_string()).or_else(|| {
            block
                .iter()
                .find_map(|l| lint.captures(l))
                .and_then(|c| c.get(1).or(c.get(2)).map(|m| m.as_str().to_string()))
        });
        out.push((
            i,
            loc[1].to_string(),
            Failure {
                title: caps[3].to_string(),
                tag,
                line: loc[2].parse().unwrap_or(0),
                col: loc[3].parse().unwrap_or(0),
                kind: if &caps[1] == "warning" {
                    FailureKind::Warning
                } else {
                    FailureKind::Error
                },
                body: block
                    .iter()
                    .filter(|l| !arrow.is_match(l))
                    .map(|l| l.to_string())
                    .collect(),
            },
        ));
    }
    out
}

fn summary(lines: &[&str], groups: &[FileGroup]) -> String {
    let result = &*TEST_RESULT;
    let (mut passed, mut failed, mut runs) = (0u64, 0u64, 0);
    for caps in lines.iter().filter_map(|l| result.captures(l)) {
        passed += caps[1].parse::<u64>().unwrap_or(0);
        failed += caps[2].parse::<u64>().unwrap_or(0);
        runs += 1;
    }
    let count = |kind| {
        groups
            .iter()
            .flat_map(|g| &g.items)
            .filter(|f| f.kind == kind)
            .count()
    };
    let (errors, warnings) = (count(FailureKind::Error), count(FailureKind::Warning));
    let mut parts = Vec::new();
    if runs > 0 {
        parts.push(format!("{failed} failing · {passed} passed"));
    }
    if errors > 0 {
        parts.push(plural(errors, "error"));
    }
    if warnings > 0 {
        parts.push(plural(warnings, "warning"));
    }
    if parts.is_empty() {
        return String::new();
    }
    parts.join(" · ")
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

#[cfg(test)]
#[path = "digest_tests.rs"]
mod tests;
