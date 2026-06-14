//! User-facing surface extraction: CLI `Commands`, TUI `TuiMode`, slash-command
//! templates, and subagents become [`Feature`]s.
//!
//! Like [`super::modules::analyze_source`], the parsers are intentionally
//! infallible — malformed source or a missing enum yields an empty `Vec`
//! rather than aborting the whole scan. Module mapping and coverage live in
//! [`super::coverage`].

use crate::insight::modules::extract_doc;
use crate::insight::schema::{Feature, SurfaceType};

/// Parse one Rust enum's variants from source into [`Feature`]s.
///
/// `enum_name` selects which enum in `src` to read ("Commands" or "TuiMode").
/// `entry_point_prefix` is the stable location, e.g. `src/cli.rs:Commands`.
/// Infallible: unparseable source or a missing enum yields an empty `Vec`.
pub(crate) fn surfaces_from_enum(
    src: &str,
    enum_name: &str,
    surface_type: SurfaceType,
    entry_point_prefix: &str,
) -> Vec<Feature> {
    let Ok(file) = syn::parse_file(src) else {
        return Vec::new();
    };
    for item in &file.items {
        if let syn::Item::Enum(e) = item
            && e.ident == enum_name
        {
            return e
                .variants
                .iter()
                .map(|v| variant_to_feature(v, surface_type, entry_point_prefix))
                .collect();
        }
    }
    Vec::new()
}

/// Build a [`Feature`] from one enum variant: kebab `id`, doc-comment summary
/// (empty when absent), and a stable `entry_point`.
fn variant_to_feature(
    v: &syn::Variant,
    surface_type: SurfaceType,
    entry_point_prefix: &str,
) -> Feature {
    let name = v.ident.to_string();
    let summary = v.attrs.iter().find_map(extract_doc).unwrap_or_default();
    Feature {
        id: slug_id(surface_type, &name),
        surface_type,
        entry_points: vec![format!("{entry_point_prefix}::{name}")],
        modules: vec![],
        summary_static: summary,
        behavior_narrative: None,
        since: None,
        related: vec![],
        name,
    }
}

/// Build [`Feature`]s from markdown files (slash commands or subagents).
///
/// `files` is `(file_stem, contents)` already read by the caller. The summary
/// is the first line that is not blank, not a heading, not an HTML comment, and
/// not inside the leading YAML frontmatter block.
pub(crate) fn surfaces_from_md_dir(
    files: &[(String, String)],
    surface_type: SurfaceType,
    entry_point_prefix: &str,
) -> Vec<Feature> {
    files
        .iter()
        .map(|(stem, contents)| Feature {
            id: slug_id(surface_type, stem),
            surface_type,
            entry_points: vec![format!("{entry_point_prefix}/{stem}.md")],
            modules: vec![],
            summary_static: first_summary_line(contents),
            behavior_narrative: None,
            since: None,
            related: vec![],
            name: stem.clone(),
        })
        .collect()
}

/// First meaningful prose line: skips leading blank lines, HTML comment blocks,
/// a YAML frontmatter block, and `#` headings.
fn first_summary_line(contents: &str) -> String {
    let mut lines = contents.lines().peekable();
    while let Some(&line) = lines.peek() {
        let t = line.trim();
        if t.is_empty() {
            lines.next();
        } else if t.starts_with("<!--") {
            for l in lines.by_ref() {
                if l.contains("-->") {
                    break;
                }
            }
        } else if t == "---" {
            lines.next();
            for l in lines.by_ref() {
                if l.trim() == "---" {
                    break;
                }
            }
        } else {
            break;
        }
    }
    for line in lines {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        return t.to_string();
    }
    String::new()
}

/// Stable kebab `id` from a surface kind + variant/command name.
/// e.g. `Cli` + `Run` -> `cli-run`; `TuiMode` + `SessionView` -> `tui-mode-session-view`.
pub(crate) fn slug_id(surface_type: SurfaceType, name: &str) -> String {
    format!("{}-{}", surface_prefix(surface_type), kebab(name))
}

/// Kebab prefix for each surface kind.
fn surface_prefix(surface_type: SurfaceType) -> &'static str {
    match surface_type {
        SurfaceType::Cli => "cli",
        SurfaceType::TuiMode => "tui-mode",
        SurfaceType::SlashCommand => "slash-command",
        SurfaceType::Subagent => "subagent",
    }
}

/// Normalize a name to kebab-case: CamelCase boundaries become hyphens, runs of
/// non-alphanumeric characters collapse to a single hyphen, all lowercased.
pub(crate) fn kebab(name: &str) -> String {
    let mut out = String::new();
    let mut prev_alnum = false;
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            if prev_alnum {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
            prev_alnum = true;
        } else if ch.is_alphanumeric() {
            out.push(ch);
            prev_alnum = true;
        } else if prev_alnum {
            out.push('-');
            prev_alnum = false;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- slug_id -----------------------------------------------------------

    #[test]
    fn slug_id_cli_run_produces_cli_run() {
        assert_eq!(slug_id(SurfaceType::Cli, "Run"), "cli-run");
    }

    #[test]
    fn slug_id_tui_mode_produces_kebab() {
        assert_eq!(
            slug_id(SurfaceType::TuiMode, "SessionView"),
            "tui-mode-session-view"
        );
    }

    #[test]
    fn slug_id_slash_command_lowercases() {
        assert_eq!(
            slug_id(SurfaceType::SlashCommand, "implement"),
            "slash-command-implement"
        );
    }

    #[test]
    fn slug_id_subagent_lowercases() {
        assert_eq!(
            slug_id(SurfaceType::Subagent, "Gatekeeper"),
            "subagent-gatekeeper"
        );
    }

    // --- surfaces_from_enum ------------------------------------------------

    #[test]
    fn surfaces_from_enum_extracts_variants_with_doc_comments() {
        let src = r#"
pub enum Commands {
    /// Run a session.
    Run,
    /// Stop everything.
    Stop,
}
"#;
        let features = surfaces_from_enum(src, "Commands", SurfaceType::Cli, "src/cli.rs:Commands");

        assert_eq!(features.len(), 2, "expected 2 variants");

        let run = features
            .iter()
            .find(|f| f.name == "Run")
            .expect("Run variant");
        assert_eq!(run.surface_type, SurfaceType::Cli);
        assert_eq!(run.id, slug_id(SurfaceType::Cli, "Run"));
        assert_eq!(run.summary_static, "Run a session.");
        assert!(
            !run.entry_points.is_empty(),
            "entry_points should contain the entry_point_prefix"
        );

        let stop = features
            .iter()
            .find(|f| f.name == "Stop")
            .expect("Stop variant");
        assert_eq!(stop.summary_static, "Stop everything.");
    }

    #[test]
    fn surfaces_from_enum_variant_without_doc_has_empty_summary() {
        let src = r#"
pub enum Commands {
    /// Has a doc.
    WithDoc,
    NoDocs,
}
"#;
        let features = surfaces_from_enum(src, "Commands", SurfaceType::Cli, "src/cli.rs:Commands");

        assert_eq!(features.len(), 2);
        let no_doc = features
            .iter()
            .find(|f| f.name == "NoDocs")
            .expect("NoDocs");
        assert_eq!(
            no_doc.summary_static, "",
            "missing doc must yield empty summary"
        );
    }

    #[test]
    fn surfaces_from_enum_extracts_tuple_and_struct_variants() {
        let src = r#"
pub enum TuiMode {
    /// Detail with payload.
    Detail(u32),
    /// Struct variant.
    Config { x: u32 },
    /// Unit variant.
    Home,
}
"#;
        let features = surfaces_from_enum(
            src,
            "TuiMode",
            SurfaceType::TuiMode,
            "src/tui/app/types.rs:TuiMode",
        );

        let names: Vec<&str> = features.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"Detail"), "tuple variant must be extracted");
        assert!(
            names.contains(&"Config"),
            "struct variant must be extracted"
        );
        assert!(names.contains(&"Home"), "unit variant must be extracted");
        assert_eq!(features.len(), 3);
    }

    #[test]
    fn surfaces_from_enum_missing_enum_returns_empty() {
        let src = r#"
pub enum Other {
    Foo,
}
"#;
        let features = surfaces_from_enum(src, "Commands", SurfaceType::Cli, "src/cli.rs:Commands");
        assert!(features.is_empty(), "missing enum must yield empty Vec");
    }

    #[test]
    fn surfaces_from_enum_malformed_source_returns_empty() {
        let src = "not valid rust @@@@ )(";
        let features = surfaces_from_enum(src, "Commands", SurfaceType::Cli, "src/cli.rs:Commands");
        assert!(
            features.is_empty(),
            "malformed source must not panic and must yield empty Vec"
        );
    }

    // --- surfaces_from_md_dir ----------------------------------------------

    #[test]
    fn surfaces_from_md_dir_skips_yaml_frontmatter_and_headings() {
        let files = vec![(
            "implement".to_string(),
            "---\ntitle: Implement\nauthor: test\n---\n# Title heading\n## Sub heading\n\nDeploys the feature to production.\n".to_string(),
        )];

        let features = surfaces_from_md_dir(
            &files,
            SurfaceType::SlashCommand,
            ".maestro/templates/commands",
        );

        assert_eq!(features.len(), 1);
        let f = &features[0];
        assert_eq!(
            f.summary_static, "Deploys the feature to production.",
            "summary must be first non-frontmatter non-heading non-blank line"
        );
        assert_eq!(f.name, "implement");
        assert_eq!(f.surface_type, SurfaceType::SlashCommand);
        assert!(!f.entry_points.is_empty());
    }

    #[test]
    fn surfaces_from_md_dir_skips_leading_html_comment() {
        let files = vec![(
            "auto".to_string(),
            "<!-- AUTO-GENERATED do not edit -->\n---\ntitle: Auto\n---\n# Auto\n\nRuns the ship-it loop.\n".to_string(),
        )];

        let features = surfaces_from_md_dir(
            &files,
            SurfaceType::SlashCommand,
            ".maestro/templates/commands",
        );

        assert_eq!(features[0].summary_static, "Runs the ship-it loop.");
    }

    #[test]
    fn surfaces_from_md_dir_file_with_only_headings_has_empty_summary() {
        let files = vec![(
            "empty-body".to_string(),
            "# Just a heading\n## Another heading\n".to_string(),
        )];

        let features = surfaces_from_md_dir(
            &files,
            SurfaceType::SlashCommand,
            ".maestro/templates/commands",
        );

        assert_eq!(features.len(), 1);
        assert_eq!(
            features[0].summary_static, "",
            "no body line → empty summary"
        );
    }

    #[test]
    fn surfaces_from_md_dir_no_frontmatter_picks_first_body_line() {
        let files = vec![(
            "simple".to_string(),
            "First body line.\nSecond line.\n".to_string(),
        )];

        let features = surfaces_from_md_dir(&files, SurfaceType::Subagent, ".claude/agents");

        assert_eq!(features[0].summary_static, "First body line.");
    }

    #[test]
    fn surfaces_from_md_dir_empty_files_list_returns_empty() {
        let features = surfaces_from_md_dir(
            &[],
            SurfaceType::SlashCommand,
            ".maestro/templates/commands",
        );
        assert!(features.is_empty());
    }
}
