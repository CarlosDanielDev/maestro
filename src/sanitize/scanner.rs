use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use syn::visit::Visit;

use super::types::{Finding, ScanResult, Severity, SmellCategory, SourceLocation};

/// Trait for static code scanning.
#[async_trait]
pub trait CodeScanner: Send + Sync {
    async fn scan(&self, path: &Path) -> anyhow::Result<ScanResult>;
}

/// Production scanner using syn AST parsing.
pub struct RustScanner;

impl RustScanner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CodeScanner for RustScanner {
    async fn scan(&self, path: &Path) -> anyhow::Result<ScanResult> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || scan_directory(&path))
            .await
            .map_err(|e| anyhow::anyhow!("Scanner task failed: {}", e))?
    }
}

fn scan_directory(root: &Path) -> anyhow::Result<ScanResult> {
    let mut findings = Vec::new();

    let rs_files = super::types::collect_rs_files(root);

    let mut all_declarations: HashMap<String, DeclInfo> = HashMap::new();
    let mut all_references: HashSet<String> = HashSet::new();
    let mut module_declarations: HashSet<String> = HashSet::new();

    for file_path in &rs_files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to read {}: {}, skipping", file_path.display(), e);
                continue;
            }
        };

        let syntax = match syn::parse_file(&source) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to parse {}: {}, skipping", file_path.display(), e);
                continue;
            }
        };

        let relative_path = file_path
            .strip_prefix(root)
            .unwrap_or(file_path)
            .to_path_buf();

        // Collect heuristic findings (Long Method, Large Class)
        findings.extend(check_heuristics(&syntax, &relative_path, &source));

        // Collect declarations and references
        let mut visitor = ItemVisitor::new(relative_path.clone(), &source);
        visitor.visit_file(&syntax);

        for decl in visitor.declarations {
            if !decl.has_allow_dead_code && !decl.is_test_only {
                all_declarations.insert(decl.qualified_name.clone(), decl);
            }
        }
        all_references.extend(visitor.references);
        module_declarations.extend(visitor.module_declarations);
    }

    // Find unreferenced declarations
    for (name, decl) in &all_declarations {
        if name == "main" || name.ends_with("::main") {
            continue;
        }
        if !all_references.contains(&decl.short_name) && !all_references.contains(name) {
            let category = match decl.kind {
                DeclKind::Function => SmellCategory::UnusedFunction,
                DeclKind::Struct => SmellCategory::UnusedStruct,
                DeclKind::Enum => SmellCategory::UnusedEnum,
                DeclKind::Import => SmellCategory::UnusedImport,
            };
            let dead_lines = decl.line_end.saturating_sub(decl.line_start) + 1;
            findings.push(Finding {
                severity: Severity::Warning,
                category,
                location: SourceLocation {
                    file: decl.file.clone(),
                    line_start: decl.line_start,
                    line_end: decl.line_end,
                },
                message: format!("'{}' is declared but never referenced", decl.short_name),
                dead_lines,
            });
        }
    }

    // Find unreferenced files
    for file_path in &rs_files {
        let relative = file_path
            .strip_prefix(root)
            .unwrap_or(file_path)
            .to_path_buf();

        if let Some(stem) = relative.file_stem().and_then(|s| s.to_str()) {
            if stem == "main" || stem == "lib" || stem == "mod" {
                continue;
            }
            if !module_declarations.contains(stem) {
                let line_count = std::fs::read_to_string(file_path)
                    .map(|s| s.lines().count() as u32)
                    .unwrap_or(0);
                let stem_owned = stem.to_string();
                findings.push(Finding {
                    severity: Severity::Info,
                    category: SmellCategory::UnusedFile,
                    location: SourceLocation {
                        file: relative,
                        line_start: 1,
                        line_end: line_count,
                    },
                    message: format!(
                        "'{}' is not declared as a module in any mod statement",
                        stem_owned
                    ),
                    dead_lines: line_count,
                });
            }
        }
    }

    Ok(ScanResult { findings })
}

// -- Line number utilities --

/// Count which line number a byte offset falls on (1-based).
fn byte_offset_to_line(source: &str, offset: usize) -> u32 {
    let clamped = offset.min(source.len());
    source[..clamped].lines().count().max(1) as u32
}

// -- Heuristic checks --

fn check_heuristics(file: &syn::File, path: &Path, source: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Track search offset to handle multiple items with same name
    let mut search_offset = 0;

    for item in &file.items {
        match item {
            syn::Item::Fn(func) => {
                if has_allow_dead_code(&func.attrs) || has_cfg_test(&func.attrs) {
                    continue;
                }
                let name = func.sig.ident.to_string();
                let (start_line, line_count, new_offset) =
                    find_braced_item(source, &format!("fn {}", name), search_offset);
                search_offset = new_offset;

                if line_count > 100 {
                    findings.push(Finding {
                        severity: Severity::Critical,
                        category: SmellCategory::LongMethod,
                        location: SourceLocation {
                            file: path.to_path_buf(),
                            line_start: start_line,
                            line_end: start_line + line_count - 1,
                        },
                        message: format!(
                            "Function '{}' is {} lines (threshold: 100)",
                            func.sig.ident, line_count
                        ),
                        dead_lines: 0,
                    });
                } else if line_count > 50 {
                    findings.push(Finding {
                        severity: Severity::Warning,
                        category: SmellCategory::LongMethod,
                        location: SourceLocation {
                            file: path.to_path_buf(),
                            line_start: start_line,
                            line_end: start_line + line_count - 1,
                        },
                        message: format!(
                            "Function '{}' is {} lines (threshold: 50)",
                            func.sig.ident, line_count
                        ),
                        dead_lines: 0,
                    });
                }
            }
            syn::Item::Impl(impl_block) => {
                if has_cfg_test(&impl_block.attrs) {
                    continue;
                }
                let type_name = impl_type_name(impl_block);
                let (start_line, line_count, new_offset) =
                    find_braced_item(source, &format!("impl {}", type_name), search_offset);
                search_offset = new_offset;

                if line_count > 400 {
                    findings.push(Finding {
                        severity: Severity::Critical,
                        category: SmellCategory::LargeClass,
                        location: SourceLocation {
                            file: path.to_path_buf(),
                            line_start: start_line,
                            line_end: start_line + line_count - 1,
                        },
                        message: format!(
                            "Impl block for '{}' is {} lines (threshold: 400)",
                            type_name, line_count
                        ),
                        dead_lines: 0,
                    });
                } else if line_count > 200 {
                    findings.push(Finding {
                        severity: Severity::Warning,
                        category: SmellCategory::LargeClass,
                        location: SourceLocation {
                            file: path.to_path_buf(),
                            line_start: start_line,
                            line_end: start_line + line_count - 1,
                        },
                        message: format!(
                            "Impl block for '{}' is {} lines (threshold: 200)",
                            type_name, line_count
                        ),
                        dead_lines: 0,
                    });
                }
            }
            _ => {}
        }
    }

    findings
}

/// Find a braced item in source starting from `offset`, returning (start_line, line_count, end_offset).
/// Searches for `needle` (e.g., "fn foo" or "impl Bar") from the given offset to handle
/// multiple items with the same name correctly.
fn find_braced_item(source: &str, needle: &str, offset: usize) -> (u32, u32, usize) {
    let search_from = offset.min(source.len());
    if let Some(rel_pos) = source[search_from..].find(needle) {
        let abs_pos = search_from + rel_pos;
        let start_line = byte_offset_to_line(source, abs_pos);

        // Find matching closing brace
        let rest = &source[abs_pos..];
        let mut depth = 0;
        let mut found_open = false;
        let mut block_end = rest.len();

        for (i, c) in rest.char_indices() {
            match c {
                '{' => {
                    depth += 1;
                    found_open = true;
                }
                '}' => {
                    depth -= 1;
                    if found_open && depth == 0 {
                        block_end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        let block_text = &rest[..block_end];
        let line_count = block_text.lines().count().max(1) as u32;
        (start_line, line_count, abs_pos + block_end)
    } else {
        (1, 0, offset)
    }
}

fn impl_type_name(impl_block: &syn::ItemImpl) -> String {
    if let syn::Type::Path(p) = &*impl_block.self_ty {
        p.path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_else(|| "Unknown".to_string())
    } else {
        "Unknown".to_string()
    }
}

fn has_allow_dead_code(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("allow")
            && attr
                .meta
                .require_list()
                .is_ok_and(|list| list.tokens.to_string().contains("dead_code"))
    })
}

fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .meta
                .require_list()
                .is_ok_and(|list| list.tokens.to_string().contains("test"))
    })
}

// -- AST Visitor for declarations and references --

#[derive(Debug, Clone)]
enum DeclKind {
    Function,
    Struct,
    Enum,
    Import,
}

#[derive(Debug, Clone)]
struct DeclInfo {
    qualified_name: String,
    short_name: String,
    kind: DeclKind,
    file: PathBuf,
    line_start: u32,
    line_end: u32,
    has_allow_dead_code: bool,
    is_test_only: bool,
}

struct ItemVisitor<'a> {
    file: PathBuf,
    source: &'a str,
    declarations: Vec<DeclInfo>,
    references: HashSet<String>,
    module_declarations: HashSet<String>,
    in_test_module: bool,
    search_offset: usize,
}

impl<'a> ItemVisitor<'a> {
    fn new(file: PathBuf, source: &'a str) -> Self {
        Self {
            file,
            source,
            declarations: Vec::new(),
            references: HashSet::new(),
            module_declarations: HashSet::new(),
            in_test_module: false,
            search_offset: 0,
        }
    }

    fn find_keyword_line(&mut self, keyword_and_name: &str) -> u32 {
        let search_from = self.search_offset.min(self.source.len());
        if let Some(rel_pos) = self.source[search_from..].find(keyword_and_name) {
            let abs_pos = search_from + rel_pos;
            self.search_offset = abs_pos + keyword_and_name.len();
            byte_offset_to_line(self.source, abs_pos)
        } else {
            1
        }
    }
}

impl<'ast, 'a> Visit<'ast> for ItemVisitor<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        let allow_dead = has_allow_dead_code(&node.attrs);
        let is_test = node.attrs.iter().any(|a| a.path().is_ident("test"))
            || has_cfg_test(&node.attrs)
            || self.in_test_module;

        let line_start = self.find_keyword_line(&format!("fn {}", name));

        self.declarations.push(DeclInfo {
            qualified_name: format!("{}::{}", self.file.display(), name),
            short_name: name,
            kind: DeclKind::Function,
            file: self.file.clone(),
            line_start,
            line_end: line_start,
            has_allow_dead_code: allow_dead,
            is_test_only: is_test,
        });

        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let name = node.ident.to_string();
        let allow_dead = has_allow_dead_code(&node.attrs);
        let is_test = has_cfg_test(&node.attrs) || self.in_test_module;
        let line = self.find_keyword_line(&format!("struct {}", name));

        self.declarations.push(DeclInfo {
            qualified_name: format!("{}::{}", self.file.display(), name),
            short_name: name,
            kind: DeclKind::Struct,
            file: self.file.clone(),
            line_start: line,
            line_end: line,
            has_allow_dead_code: allow_dead,
            is_test_only: is_test,
        });

        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let name = node.ident.to_string();
        let allow_dead = has_allow_dead_code(&node.attrs);
        let is_test = has_cfg_test(&node.attrs) || self.in_test_module;
        let line = self.find_keyword_line(&format!("enum {}", name));

        self.declarations.push(DeclInfo {
            qualified_name: format!("{}::{}", self.file.display(), name),
            short_name: name,
            kind: DeclKind::Enum,
            file: self.file.clone(),
            line_start: line,
            line_end: line,
            has_allow_dead_code: allow_dead,
            is_test_only: is_test,
        });

        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        if let Some(name) = extract_use_name(&node.tree) {
            let allow_dead = has_allow_dead_code(&node.attrs);
            let line = self.find_keyword_line(&name);

            self.declarations.push(DeclInfo {
                qualified_name: format!("{}::use::{}", self.file.display(), name),
                short_name: name,
                kind: DeclKind::Import,
                file: self.file.clone(),
                line_start: line,
                line_end: line,
                has_allow_dead_code: allow_dead,
                is_test_only: self.in_test_module,
            });
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let name = node.ident.to_string();
        self.module_declarations.insert(name);

        let was_in_test = self.in_test_module;
        if has_cfg_test(&node.attrs) {
            self.in_test_module = true;
        }

        syn::visit::visit_item_mod(self, node);
        self.in_test_module = was_in_test;
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if let Some(segment) = path.segments.last() {
            self.references.insert(segment.ident.to_string());
        }
        syn::visit::visit_path(self, path);
    }
}

fn extract_use_name(tree: &syn::UseTree) -> Option<String> {
    match tree {
        syn::UseTree::Path(p) => extract_use_name(&p.tree),
        syn::UseTree::Name(n) => Some(n.ident.to_string()),
        syn::UseTree::Rename(r) => Some(r.rename.to_string()),
        syn::UseTree::Glob(_) => None,
        syn::UseTree::Group(_) => None,
    }
}

/// Mock scanner for testing other modules.
#[cfg(test)]
pub struct MockCodeScanner {
    pub result: ScanResult,
}

#[cfg(test)]
#[async_trait]
impl CodeScanner for MockCodeScanner {
    async fn scan(&self, _path: &Path) -> anyhow::Result<ScanResult> {
        Ok(self.result.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[tokio::test]
    async fn empty_project_returns_no_findings() {
        let dir = tempfile::tempdir().unwrap();
        let scanner = RustScanner::new();
        let result = scanner.scan(dir.path()).await.unwrap();
        assert!(result.findings.is_empty());
    }

    #[tokio::test]
    async fn long_method_warning_over_50_lines() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = String::from("fn long_function() {\n");
        for i in 0..55 {
            body.push_str(&format!("    let _x{} = {};\n", i, i));
        }
        body.push_str("}\n");

        write_file(dir.path(), "main.rs", &body);
        let scanner = RustScanner::new();
        let result = scanner.scan(dir.path()).await.unwrap();

        let long_methods: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.category == SmellCategory::LongMethod)
            .collect();
        assert!(!long_methods.is_empty(), "should detect long method");
        assert_eq!(long_methods[0].severity, Severity::Warning);
    }

    #[tokio::test]
    async fn long_method_critical_over_100_lines() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = String::from("fn very_long_function() {\n");
        for i in 0..105 {
            body.push_str(&format!("    let _x{} = {};\n", i, i));
        }
        body.push_str("}\n");

        write_file(dir.path(), "main.rs", &body);
        let scanner = RustScanner::new();
        let result = scanner.scan(dir.path()).await.unwrap();

        let long_methods: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.category == SmellCategory::LongMethod)
            .collect();
        assert!(!long_methods.is_empty());
        assert_eq!(long_methods[0].severity, Severity::Critical);
    }

    #[tokio::test]
    async fn large_class_warning_over_200_lines() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = String::from("struct Foo;\nimpl Foo {\n");
        for i in 0..205 {
            body.push_str(&format!("    fn method_{}(&self) -> i32 {{ {} }}\n", i, i));
        }
        body.push_str("}\n");

        write_file(dir.path(), "main.rs", &body);
        let scanner = RustScanner::new();
        let result = scanner.scan(dir.path()).await.unwrap();

        let large: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.category == SmellCategory::LargeClass)
            .collect();
        assert!(!large.is_empty(), "should detect large class");
        assert_eq!(large[0].severity, Severity::Warning);
    }

    #[tokio::test]
    async fn allow_dead_code_items_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let source = r#"
#[allow(dead_code)]
fn unused_but_allowed() {}

fn main() {}
"#;

        write_file(dir.path(), "main.rs", source);
        let scanner = RustScanner::new();
        let result = scanner.scan(dir.path()).await.unwrap();

        let unused_funcs: Vec<_> = result
            .findings
            .iter()
            .filter(|f| {
                f.category == SmellCategory::UnusedFunction
                    && f.message.contains("unused_but_allowed")
            })
            .collect();
        assert!(
            unused_funcs.is_empty(),
            "allow(dead_code) items should not be flagged"
        );
    }

    #[tokio::test]
    async fn cfg_test_items_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let source = r#"
fn main() {}

#[cfg(test)]
mod tests {
    fn helper() {}

    #[test]
    fn it_works() {
        helper();
    }
}
"#;

        write_file(dir.path(), "main.rs", source);
        let scanner = RustScanner::new();
        let result = scanner.scan(dir.path()).await.unwrap();

        let test_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.message.contains("helper") || f.message.contains("it_works"))
            .collect();
        assert!(
            test_findings.is_empty(),
            "test module items should not be flagged, got: {:?}",
            test_findings
        );
    }

    #[tokio::test]
    async fn referenced_items_not_flagged_as_unused() {
        let dir = tempfile::tempdir().unwrap();
        let source = r#"
struct MyStruct;

fn main() {
    let _s = MyStruct;
}
"#;

        write_file(dir.path(), "main.rs", source);
        let scanner = RustScanner::new();
        let result = scanner.scan(dir.path()).await.unwrap();

        let unused_structs: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.category == SmellCategory::UnusedStruct && f.message.contains("MyStruct"))
            .collect();
        assert!(
            unused_structs.is_empty(),
            "referenced struct should not be flagged"
        );
    }

    #[tokio::test]
    async fn unreferenced_file_flagged_as_unused() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "main.rs", "fn main() {}");
        write_file(dir.path(), "orphan.rs", "fn orphan_fn() {}");

        let scanner = RustScanner::new();
        let result = scanner.scan(dir.path()).await.unwrap();

        let unused_files: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.category == SmellCategory::UnusedFile)
            .collect();
        assert!(
            !unused_files.is_empty(),
            "orphan.rs should be flagged as unused file"
        );
    }

    #[tokio::test]
    async fn unparseable_file_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "main.rs", "fn main() {}");
        write_file(dir.path(), "broken.rs", "this is not valid rust {{{{");

        let scanner = RustScanner::new();
        let result = scanner.scan(dir.path()).await;
        assert!(result.is_ok(), "scanner should not crash on parse failure");
    }

    #[tokio::test]
    async fn mock_scanner_returns_canned_result() {
        let mock = MockCodeScanner {
            result: ScanResult {
                findings: vec![Finding {
                    severity: Severity::Warning,
                    category: SmellCategory::UnusedFunction,
                    location: SourceLocation {
                        file: PathBuf::from("test.rs"),
                        line_start: 1,
                        line_end: 10,
                    },
                    message: "test finding".to_string(),
                    dead_lines: 10,
                }],
            },
        };
        let result = mock.scan(Path::new(".")).await.unwrap();
        assert_eq!(result.findings.len(), 1);
    }
}
