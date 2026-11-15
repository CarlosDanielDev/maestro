//! Comment-preserving config-mutation primitive.
//!
//! `ensure_field` walks a dotted `schema_path` against a `toml_edit::DocumentMut`,
//! inserting the leaf with `default` if missing. Existing values (and surrounding
//! comments/blank lines) are never touched — this is the foundation for
//! formatting-preserving single-key migrations.

use anyhow::{Result, bail};
use toml_edit::{DocumentMut, Item, Table, Value};

/// Outcome of an `ensure_field` call. Callers branch on this to decide
/// whether a notice should be emitted / a write should happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnsureOutcome {
    /// Leaf was already present (any value); document untouched.
    AlreadyPresent,
    /// Leaf was inserted with `default`; document mutated.
    Inserted,
}

/// Ensure `schema_path` exists in `doc`, inserting `default` if missing.
///
/// Idempotent: if the leaf already exists at all (regardless of value), this
/// is a no-op returning `AlreadyPresent` — the caller's `default` never
/// overwrites the user's explicit setting. Missing intermediate tables are
/// created on demand.
///
/// Returns `Err` only when:
/// - `schema_path` is empty, has a leading/trailing dot, or contains an empty
///   segment (programmer error);
/// - an intermediate segment exists but is not a table (config schema clash).
pub(crate) fn ensure_field(
    doc: &mut DocumentMut,
    schema_path: &str,
    default: Value,
) -> Result<EnsureOutcome> {
    let segments = split_path(schema_path)?;
    let Some((leaf, intermediates)) = segments.split_last() else {
        bail!("ensure_field: schema_path {schema_path:?} has no leaf segment");
    };

    let table = walk_to_table(doc.as_table_mut(), intermediates, schema_path)?;
    if table.contains_key(leaf) {
        return Ok(EnsureOutcome::AlreadyPresent);
    }
    table.insert(leaf, Item::Value(default));
    Ok(EnsureOutcome::Inserted)
}

fn split_path(schema_path: &str) -> Result<Vec<&str>> {
    if schema_path.is_empty() {
        bail!("ensure_field: schema_path must not be empty");
    }
    let segments: Vec<&str> = schema_path.split('.').collect();
    if segments.iter().any(|s| s.is_empty()) {
        bail!("ensure_field: schema_path {schema_path:?} has empty segment");
    }
    Ok(segments)
}

fn walk_to_table<'a>(
    root: &'a mut Table,
    segments: &[&str],
    full_path: &str,
) -> Result<&'a mut Table> {
    let mut current: &mut Table = root;
    for segment in segments {
        if !current.contains_key(segment) {
            let mut new_table = Table::new();
            new_table.set_implicit(false);
            current.insert(segment, Item::Table(new_table));
        }
        let Some(next) = current.get_mut(segment) else {
            bail!(
                "ensure_field: intermediate segment {segment:?} in {full_path:?} vanished after insert"
            );
        };
        match next {
            Item::Table(t) => current = t,
            _ => bail!(
                "ensure_field: intermediate segment {segment:?} in {full_path:?} is not a table"
            ),
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml_edit::DocumentMut;

    fn assert_original_lines_untouched(before: &str, after: &str) {
        let before_lines: Vec<&str> = before.lines().collect();
        let after_lines: Vec<&str> = after.lines().collect();
        for original_line in &before_lines {
            assert!(
                after_lines.contains(original_line),
                "original line was mutated or removed.\n  missing: {original_line:?}\n--- before ---\n{before}\n--- after ---\n{after}"
            );
        }
    }

    #[test]
    fn insert_into_empty_doc_returns_inserted_and_leaf_present() {
        let mut doc: DocumentMut = "".parse().unwrap();
        let outcome =
            ensure_field(&mut doc, "views.agent_graph_enabled", Value::from(true)).unwrap();
        assert_eq!(outcome, EnsureOutcome::Inserted);
        let rendered = doc.to_string();
        let reparsed: DocumentMut = rendered.parse().unwrap();
        let val = reparsed["views"]["agent_graph_enabled"]
            .as_value()
            .and_then(Value::as_bool);
        assert_eq!(
            val,
            Some(true),
            "inserted value must be readable: {rendered}"
        );
    }

    #[test]
    fn leaf_present_same_value_returns_already_present_no_mutation() {
        let toml_in = "[views]\nagent_graph_enabled = true\n";
        let mut doc: DocumentMut = toml_in.parse().unwrap();
        let outcome =
            ensure_field(&mut doc, "views.agent_graph_enabled", Value::from(true)).unwrap();
        assert_eq!(outcome, EnsureOutcome::AlreadyPresent);
        assert_eq!(doc.to_string(), toml_in);
    }

    #[test]
    fn leaf_present_different_value_returns_already_present_value_unchanged() {
        let toml_in = "[views]\nagent_graph_enabled = false\n";
        let mut doc: DocumentMut = toml_in.parse().unwrap();
        let outcome =
            ensure_field(&mut doc, "views.agent_graph_enabled", Value::from(true)).unwrap();
        assert_eq!(outcome, EnsureOutcome::AlreadyPresent);
        let rendered = doc.to_string();
        assert_eq!(rendered, toml_in, "opt-out must survive byte-identical");
        assert!(rendered.contains("agent_graph_enabled = false"));
    }

    #[test]
    fn missing_intermediate_table_is_created_on_insert() {
        let mut doc: DocumentMut = "[sessions]\ndefault_model = \"opus\"\n".parse().unwrap();
        let outcome =
            ensure_field(&mut doc, "views.agent_graph_enabled", Value::from(true)).unwrap();
        assert_eq!(outcome, EnsureOutcome::Inserted);
        let rendered = doc.to_string();
        let reparsed: DocumentMut = rendered.parse().unwrap();
        assert_eq!(
            reparsed["views"]["agent_graph_enabled"]
                .as_value()
                .and_then(Value::as_bool),
            Some(true),
        );
    }

    #[test]
    fn three_level_path_with_missing_middle_creates_all_intermediate_tables() {
        let mut doc: DocumentMut = "[sessions]\ndefault_model = \"opus\"\n".parse().unwrap();
        let outcome = ensure_field(&mut doc, "agents.claude.kind", Value::from("claude")).unwrap();
        assert_eq!(outcome, EnsureOutcome::Inserted);
        let rendered = doc.to_string();
        let reparsed: DocumentMut = rendered.parse().unwrap();
        assert_eq!(
            reparsed["agents"]["claude"]["kind"]
                .as_value()
                .and_then(Value::as_str),
            Some("claude"),
        );
    }

    #[test]
    fn intermediate_exists_as_non_table_returns_err() {
        let mut doc: DocumentMut = "views = \"foo\"\n".parse().unwrap();
        let result = ensure_field(&mut doc, "views.agent_graph_enabled", Value::from(true));
        assert!(result.is_err(), "non-table intermediate must return Err");
    }

    #[test]
    fn second_call_returns_already_present_idempotency() {
        let mut doc: DocumentMut = "".parse().unwrap();
        let first = ensure_field(&mut doc, "views.agent_graph_enabled", Value::from(true)).unwrap();
        assert_eq!(first, EnsureOutcome::Inserted);
        let snapshot = doc.to_string();
        let second =
            ensure_field(&mut doc, "views.agent_graph_enabled", Value::from(true)).unwrap();
        assert_eq!(second, EnsureOutcome::AlreadyPresent);
        assert_eq!(doc.to_string(), snapshot);
    }

    #[test]
    fn empty_path_returns_err() {
        let mut doc: DocumentMut = "".parse().unwrap();
        let result = ensure_field(&mut doc, "", Value::from(true));
        assert!(result.is_err());
    }

    #[test]
    fn leading_dot_path_returns_err() {
        let mut doc: DocumentMut = "".parse().unwrap();
        let result = ensure_field(&mut doc, ".foo", Value::from(true));
        assert!(result.is_err());
    }

    #[test]
    fn trailing_dot_path_returns_err() {
        let mut doc: DocumentMut = "".parse().unwrap();
        let result = ensure_field(&mut doc, "foo.", Value::from(true));
        assert!(result.is_err());
    }

    #[test]
    fn existing_lines_byte_identical_after_insert_into_commented_doc() {
        let toml_in = concat!(
            "# top-level comment\n",
            "\n",
            "[sessions]\n",
            "# session comment\n",
            "default_model = \"opus\"\n",
            "\n",
            "[budget]\n",
            "per_session_usd = 5.0\n",
        );
        let mut doc: DocumentMut = toml_in.parse().unwrap();
        let outcome =
            ensure_field(&mut doc, "views.agent_graph_enabled", Value::from(true)).unwrap();
        assert_eq!(outcome, EnsureOutcome::Inserted);
        let rendered = doc.to_string();
        assert_original_lines_untouched(toml_in, &rendered);
        assert!(rendered.contains("agent_graph_enabled = true"));
        assert!(rendered.parse::<DocumentMut>().is_ok());
    }
}
