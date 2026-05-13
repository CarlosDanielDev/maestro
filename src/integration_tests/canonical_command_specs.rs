//! Invariant tests for canonical command specs under `.maestro/templates/commands/`.
//!
//! Validates structural and semantic invariants for the four canonical specs
//! authored in issue #702. These tests read the real project filesystem via
//! `CARGO_MANIFEST_DIR` — RED phase fails with descriptive `panic!` messages
//! while the files are absent; GREEN phase passes once they are authored.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::templates::{PlaceholderKind, TemplateError, Token, render_str, tokenize};

const SLUGS: &[&str] = &["implement", "pushup", "plan-feature", "simplify"];

fn project_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_canonical(slug: &str) -> Result<String, String> {
    let path = project_root()
        .join(".maestro/templates/commands")
        .join(format!("{slug}.md"));
    std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "cannot read canonical file {slug}.md at {}: {e}",
            path.display()
        )
    })
}

fn split_frontmatter<'a>(content: &'a str, slug: &str) -> (&'a str, &'a str) {
    assert!(
        content.starts_with("---\n"),
        "{slug}: file must start with '---\\n' frontmatter fence"
    );
    let after_open = &content[4..];
    let close_pos = after_open.find("\n---\n").unwrap_or_else(|| {
        panic!("{slug}: no closing '---' frontmatter fence found");
    });
    let fm_text = &after_open[..close_pos];
    let body_start = close_pos + "\n---\n".len();
    let body = if body_start <= after_open.len() {
        &after_open[body_start..]
    } else {
        ""
    };
    (fm_text, body)
}

#[derive(Debug, Default)]
struct Frontmatter {
    command: String,
    version: String,
    description: String,
    placeholders: Vec<String>,
    includes: Vec<String>,
    ported_from: String,
}

fn parse_frontmatter(fm_text: &str, slug: &str) -> Frontmatter {
    let mut fm = Frontmatter::default();
    let mut in_placeholders = false;
    let mut in_includes = false;
    let mut in_provenance = false;

    for line in fm_text.lines() {
        if line.starts_with("  - ") {
            let val = line[4..].trim().trim_matches('"').to_string();
            if in_placeholders {
                fm.placeholders.push(val);
                continue;
            }
            if in_includes {
                fm.includes.push(val);
                continue;
            }
        }

        if !line.starts_with("  ") {
            in_placeholders = false;
            in_includes = false;
            in_provenance = false;
        }

        if let Some(rest) = line.strip_prefix("command:") {
            fm.command = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("version:") {
            fm.version = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("description:") {
            fm.description = rest.trim().trim_matches('"').to_string();
        } else if line.starts_with("placeholders:") {
            let rest = line["placeholders:".len()..].trim();
            if rest != "[]" {
                in_placeholders = true;
            }
        } else if line.starts_with("includes:") {
            let rest = line["includes:".len()..].trim();
            if rest != "[]" {
                in_includes = true;
            }
        } else if line.starts_with("source_provenance:") {
            in_provenance = true;
        } else if in_provenance {
            if let Some(rest) = line.strip_prefix("  ported_from:") {
                fm.ported_from = rest.trim().trim_matches('"').to_string();
            }
        }
    }
    assert!(
        !fm.command.is_empty(),
        "{slug}: frontmatter missing 'command:' field"
    );
    fm
}

fn placeholder_tokens(body: &str, slug: &str) -> Vec<Token> {
    tokenize(body, slug)
        .unwrap_or_else(|e| panic!("{slug}: tokenize failed: {e}"))
        .into_iter()
        .filter(|t| matches!(t, Token::Placeholder { .. }))
        .collect()
}

// ── Invariants 1 & 2: frontmatter fences and parse ───────────────────────────

#[test]
fn canonical_files_have_valid_frontmatter_fences() {
    for slug in SLUGS {
        let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
        let (fm_text, _body) = split_frontmatter(&content, slug);
        let _fm = parse_frontmatter(fm_text, slug);
    }
}

// ── Invariant 3: command field matches filename ──────────────────────────────

#[test]
fn canonical_command_field_matches_filename_stem() {
    for slug in SLUGS {
        let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
        let (fm_text, _) = split_frontmatter(&content, slug);
        let fm = parse_frontmatter(fm_text, slug);
        assert_eq!(
            fm.command, *slug,
            "{slug}: command: field '{}' does not match filename stem",
            fm.command
        );
    }
}

// ── Invariant 4: version is N.N.N semver ─────────────────────────────────────

#[test]
fn canonical_version_field_is_semver() {
    for slug in SLUGS {
        let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
        let (fm_text, _) = split_frontmatter(&content, slug);
        let fm = parse_frontmatter(fm_text, slug);
        let parts: Vec<&str> = fm.version.splitn(3, '.').collect();
        assert_eq!(
            parts.len(),
            3,
            "{slug}: version '{}' is not N.N.N semver",
            fm.version
        );
        for part in &parts {
            assert!(
                part.parse::<u32>().is_ok(),
                "{slug}: version component '{}' in '{}' is not a number",
                part,
                fm.version
            );
        }
    }
}

// ── Invariant 5: frontmatter placeholders == body placeholders ───────────────

#[test]
fn frontmatter_placeholders_match_body_placeholders() {
    for slug in SLUGS {
        let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
        let (fm_text, body) = split_frontmatter(&content, slug);
        let fm = parse_frontmatter(fm_text, slug);

        let tokens = placeholder_tokens(body, slug);
        let mut body_names: Vec<String> = tokens
            .iter()
            .filter_map(|t| match t {
                Token::Placeholder { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        body_names.sort();

        let mut fm_names = fm.placeholders.clone();
        fm_names.sort();

        assert_eq!(
            fm_names, body_names,
            "{slug}: frontmatter placeholders {fm_names:?} != body placeholders {body_names:?}"
        );
    }
}

// ── Invariants 6 & 7: known kinds + required args ────────────────────────────

#[test]
fn all_placeholder_names_are_known_kinds_with_required_args() {
    for slug in SLUGS {
        let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
        let (_, body) = split_frontmatter(&content, slug);
        let tokens = placeholder_tokens(body, slug);

        for tok in &tokens {
            if let Token::Placeholder { name, args, .. } = tok {
                let kind = PlaceholderKind::from_name(name)
                    .unwrap_or_else(|| panic!("{slug}: unknown placeholder kind '{name}'"));
                for required in kind.required_args() {
                    assert!(
                        args.contains_key(*required),
                        "{slug}: placeholder '{name}' missing required arg '{required}'"
                    );
                }
            }
        }
    }
}

// ── Invariants 8, 9, 10: includes match and resolve under core/ ──────────────

#[test]
fn frontmatter_includes_match_body_and_resolve_under_core() {
    for slug in SLUGS {
        let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
        let (fm_text, body) = split_frontmatter(&content, slug);
        let fm = parse_frontmatter(fm_text, slug);

        let tokens = placeholder_tokens(body, slug);
        let mut body_includes: Vec<String> = tokens
            .iter()
            .filter_map(|t| match t {
                Token::Placeholder { name, args, .. } if name == "INCLUDE" => {
                    args.get("path").cloned()
                }
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        body_includes.sort();

        let mut fm_includes = fm.includes.clone();
        fm_includes.sort();

        assert_eq!(
            fm_includes, body_includes,
            "{slug}: frontmatter includes {fm_includes:?} != body includes {body_includes:?}"
        );

        for inc_path in &body_includes {
            let resolved = project_root().join(".maestro/templates").join(inc_path);
            assert!(
                resolved.exists(),
                "{slug}: INCLUDE path '{inc_path}' does not resolve to existing file at {}",
                resolved.display()
            );
            assert!(
                !inc_path.starts_with("commands/"),
                "{slug}: INCLUDE path '{inc_path}' starts with 'commands/' — only 'core/' is allowed"
            );
        }
    }
}

// ── Invariant 11: no raw .claude/commands or .claude/agents in body ──────────

#[test]
fn body_contains_no_raw_claude_paths() {
    for slug in SLUGS {
        let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
        let (_, body) = split_frontmatter(&content, slug);
        assert!(
            !body.contains(".claude/commands/"),
            "{slug}: body contains raw '.claude/commands/' path — must go through a placeholder"
        );
        assert!(
            !body.contains(".claude/agents/"),
            "{slug}: body contains raw '.claude/agents/' path — must go through a placeholder"
        );
    }
}

// ── Invariants 12, 13, 15: tokenize ok, NullRules fails closed, prompts cap ──

#[test]
fn tokenize_ok_null_render_fails_closed_prompts_under_cap() {
    let null = crate::templates::null_rules();
    for slug in SLUGS {
        let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
        let (_, body) = split_frontmatter(&content, slug);

        let tokens =
            tokenize(body, slug).unwrap_or_else(|e| panic!("{slug}: tokenize returned Err: {e}"));

        for tok in &tokens {
            if let Token::Placeholder { name, args, .. } = tok {
                if let Some(prompt) = args.get("prompt") {
                    assert!(
                        prompt.len() <= 200,
                        "{slug}: placeholder '{name}' prompt= is {} bytes (max 200): {:?}",
                        prompt.len(),
                        &prompt[..prompt.len().min(60)]
                    );
                }
            }
        }

        let has_placeholder = tokens
            .iter()
            .any(|t| matches!(t, Token::Placeholder { .. }));
        if has_placeholder {
            let result = render_str(body, null);
            assert!(
                matches!(result, Err(TemplateError::UnsupportedByProvider { .. })),
                "{slug}: render with NullRules should be Err(UnsupportedByProvider), got {result:?}"
            );
        }
    }
}

// ── Invariant 14: source_provenance.ported_from is valid ─────────────────────

#[test]
fn source_provenance_ported_from_is_valid() {
    for slug in SLUGS {
        let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
        let (fm_text, _) = split_frontmatter(&content, slug);
        let fm = parse_frontmatter(fm_text, slug);

        assert!(
            !fm.ported_from.is_empty(),
            "{slug}: frontmatter missing 'ported_from' under source_provenance"
        );

        if fm.ported_from != "new" {
            let source = project_root().join(&fm.ported_from);
            assert!(
                source.exists(),
                "{slug}: ported_from '{}' does not point to an existing file",
                fm.ported_from
            );
        }
    }
}

// ── Tokenize snapshots ───────────────────────────────────────────────────────

fn snapshot_tokens(slug: &str) -> Vec<(String, Vec<(String, String)>)> {
    let content = read_canonical(slug).unwrap_or_else(|e| panic!("{e}"));
    let (_, body) = split_frontmatter(&content, slug);
    let tokens = tokenize(body, slug).unwrap_or_else(|e| panic!("{slug}: tokenize failed: {e}"));
    tokens
        .into_iter()
        .filter_map(|t| match t {
            Token::Placeholder { name, args, .. } => {
                let mut sorted_args: Vec<(String, String)> = args.into_iter().collect();
                sorted_args.sort_by(|a, b| a.0.cmp(&b.0));
                Some((name, sorted_args))
            }
            Token::Text(_) => None,
        })
        .collect()
}

#[test]
fn snapshot_tokenize_implement() {
    insta::assert_debug_snapshot!(snapshot_tokens("implement"));
}

#[test]
fn snapshot_tokenize_pushup() {
    insta::assert_debug_snapshot!(snapshot_tokens("pushup"));
}

#[test]
fn snapshot_tokenize_plan_feature() {
    insta::assert_debug_snapshot!(snapshot_tokens("plan-feature"));
}

#[test]
fn snapshot_tokenize_simplify() {
    insta::assert_debug_snapshot!(snapshot_tokens("simplify"));
}
