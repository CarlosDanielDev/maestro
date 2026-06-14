//! Per-module static analysis: public API surface, inner doc-comment, and
//! `crate::` use-edges, extracted from Rust source with `syn`.
//!
//! [`analyze_source`] is intentionally infallible — malformed source yields an
//! empty [`Module`] rather than aborting the whole scan.

use crate::insight::schema::Module;
use syn::Item;

/// Parse one Rust source string into a partial [`Module`] (no `loc`/`feature_ids`
/// yet — those are filled by the orchestrator and later phases).
pub fn analyze_source(path: &str, src: &str) -> Module {
    let Ok(file) = syn::parse_file(src) else {
        return Module {
            path: path.to_string(),
            ..Module::default()
        };
    };

    let doc_comment = file.attrs.iter().find_map(extract_doc);

    let mut public_api = Vec::new();
    let mut depends_on = Vec::new();
    for item in &file.items {
        if let Some(name) = pub_item_name(item) {
            public_api.push(name);
        }
        if let Item::Use(use_item) = item
            && let Some(dep) = crate_use_to_module(&use_item.tree)
        {
            depends_on.push(dep);
        }
    }
    depends_on.dedup();
    public_api.sort();
    depends_on.sort();

    Module {
        path: path.to_string(),
        loc: 0,
        public_api,
        doc_comment,
        depends_on,
        feature_ids: vec![],
    }
}

/// Extract the text of a doc-comment (`//!` or `///`), which `syn` lowers to a
/// `#[doc = "..."]` attribute. Shared with [`super::features`] for variant docs.
pub(crate) fn extract_doc(attr: &syn::Attribute) -> Option<String> {
    if !attr.path().is_ident("doc") {
        return None;
    }
    if let syn::Meta::NameValue(nv) = &attr.meta
        && let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = &nv.value
    {
        return Some(s.value().trim().to_string());
    }
    None
}

/// Name of a `pub` item, or `None` for private / restricted-visibility items.
fn pub_item_name(item: &Item) -> Option<String> {
    macro_rules! if_pub {
        ($v:expr, $name:expr) => {
            if matches!($v, syn::Visibility::Public(_)) {
                Some($name.to_string())
            } else {
                None
            }
        };
    }
    match item {
        Item::Fn(f) => if_pub!(&f.vis, f.sig.ident),
        Item::Struct(s) => if_pub!(&s.vis, s.ident),
        Item::Enum(e) => if_pub!(&e.vis, e.ident),
        Item::Trait(t) => if_pub!(&t.vis, t.ident),
        Item::Type(t) => if_pub!(&t.vis, t.ident),
        Item::Const(c) => if_pub!(&c.vis, c.ident),
        _ => None,
    }
}

/// `use crate::state::store;` → `src/state`. Only `crate::` paths map to
/// modules; the edge is the top-level module (leaf segments dropped).
fn crate_use_to_module(tree: &syn::UseTree) -> Option<String> {
    fn segments(tree: &syn::UseTree, segs: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(p) => {
                segs.push(p.ident.to_string());
                segments(&p.tree, segs);
            }
            syn::UseTree::Name(n) => segs.push(n.ident.to_string()),
            syn::UseTree::Rename(r) => segs.push(r.ident.to_string()),
            syn::UseTree::Group(g) => {
                if let Some(inner) = g.items.iter().next() {
                    segments(inner, segs);
                }
            }
            syn::UseTree::Glob(_) => {}
        }
    }
    let mut segs = Vec::new();
    segments(tree, &mut segs);
    if segs.first().map(String::as_str) == Some("crate") && segs.len() >= 2 {
        Some(format!("src/{}", segs[1]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_pub_api_doc_comment_and_use_edges() {
        let src = r#"
//! Session management.
use crate::state::store;
pub struct Session;
pub fn run() {}
fn private_helper() {}
"#;
        let m = analyze_source("src/session", src);
        assert_eq!(m.path, "src/session");
        assert_eq!(m.doc_comment.as_deref(), Some("Session management."));
        assert!(m.public_api.contains(&"Session".to_string()));
        assert!(m.public_api.contains(&"run".to_string()));
        assert!(!m.public_api.contains(&"private_helper".to_string()));
        assert!(m.depends_on.contains(&"src/state".to_string()));
    }

    #[test]
    fn analyze_source_malformed_input_yields_empty_module() {
        // Infallibility contract: malformed source must not panic and must
        // yield an empty Module rather than aborting the whole scan.
        let src = "not valid rust syntax @@@@ )(";
        let m = analyze_source("src/foo", src);
        assert_eq!(m.path, "src/foo");
        assert_eq!(m.doc_comment, None);
        assert!(m.public_api.is_empty());
        assert!(m.depends_on.is_empty());
    }
}
