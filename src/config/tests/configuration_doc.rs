//! Verifies that every TOML example and every relative link in
//! `docs/configuration.md` matches repo reality. AC for issue #674.

use std::fs;
use std::path::{Path, PathBuf};

fn doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/configuration.md")
}

fn read_doc() -> String {
    fs::read_to_string(doc_path()).expect("read docs/configuration.md")
}

fn fenced_toml_blocks(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    let mut buf = String::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if in_block {
                out.push(std::mem::take(&mut buf));
                in_block = false;
            } else if trimmed == "```toml" {
                in_block = true;
            }
            continue;
        }
        if in_block {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    out
}

#[test]
fn every_toml_block_parses_as_toml_value() {
    let doc = read_doc();
    let blocks = fenced_toml_blocks(&doc);
    assert!(
        blocks.len() >= 10,
        "expected the doc to contain many TOML examples, found {}",
        blocks.len()
    );
    for (idx, block) in blocks.iter().enumerate() {
        if let Err(e) = toml::from_str::<toml::Value>(block) {
            panic!("block #{idx} is not valid TOML: {e}\n---\n{block}\n---");
        }
    }
}

#[test]
fn relative_links_in_doc_resolve() {
    let doc = read_doc();
    let doc_dir = doc_path().parent().expect("docs dir").to_path_buf();

    // Match `[text](target)` where target does not start with http:// or https://
    // and does not start with `#`. Stop at the first `)` or whitespace.
    let re = regex::Regex::new(r"\]\(([^)#][^)#\s]*)").expect("valid regex");
    let mut checked = 0;
    for cap in re.captures_iter(&doc) {
        let target = &cap[1];
        if target.starts_with("http://") || target.starts_with("https://") {
            continue;
        }
        // Strip in-page anchors and query strings if any.
        let path_part = target
            .split_once(['#', '?'])
            .map(|(prefix, _)| prefix)
            .unwrap_or(target);
        if path_part.is_empty() {
            continue;
        }
        let candidate = if Path::new(path_part).is_absolute() {
            PathBuf::from(path_part)
        } else {
            doc_dir.join(path_part)
        };
        assert!(
            candidate.exists(),
            "link target does not exist on disk: {path_part}\n  resolved: {}",
            candidate.display(),
        );
        checked += 1;
    }
    assert!(
        checked >= 4,
        "expected at least 4 cross-reference links in the doc, found {checked}",
    );
}
