//! Scaffolds the `.maestro/templates/` reference tree into a freshly
//! initialized project. Canonical content lives at
//! `template/.maestro/templates/` in the repo and is embedded via
//! `include_bytes!` at build time. Tests drive `scaffold_templates_dir`
//! through an `InMemoryScaffolder` (see `scaffold_tests.rs`).

use anyhow::{Context, Result};
use std::io;
use std::path::{Component, Path, PathBuf};

/// Per-file outcome of a scaffold run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScaffoldAction {
    Created,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldedFile {
    pub path: PathBuf,
    pub action: ScaffoldAction,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScaffoldReport {
    pub files: Vec<ScaffoldedFile>,
}

impl ScaffoldReport {
    pub fn count(&self, action: ScaffoldAction) -> usize {
        self.files.iter().filter(|f| f.action == action).count()
    }
}

/// I/O seam — the FS implementation writes through `std::fs`; tests use an
/// in-memory map.
pub trait Scaffolder {
    fn write(&self, rel_path: &Path, contents: &[u8]) -> io::Result<()>;
    fn exists(&self, rel_path: &Path) -> bool;
}

pub struct FsScaffolder {
    pub root: PathBuf,
}

impl FsScaffolder {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

/// Reject absolute paths, drive prefixes, and any `..` traversal segments.
/// `rel_path` MUST be a relative path with only `Normal` and `CurDir`
/// components. Returning `InvalidInput` here ensures `FsScaffolder` cannot
/// be coerced into writing outside `self.root` even if a future caller
/// passes attacker-influenced input.
fn validate_relative(rel_path: &Path) -> io::Result<()> {
    for component in rel_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "scaffolder rejects non-relative path component in {}",
                        rel_path.display()
                    ),
                ));
            }
        }
    }
    Ok(())
}

impl Scaffolder for FsScaffolder {
    fn write(&self, rel_path: &Path, contents: &[u8]) -> io::Result<()> {
        validate_relative(rel_path)?;
        let abs = self.root.join(rel_path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // create_new closes the TOCTOU between `exists()` and `write()`: if
        // a concurrent writer wins the race, we treat the file as already
        // present (Skipped) and do NOT clobber it.
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&abs)
        {
            Ok(mut f) => {
                use std::io::Write;
                f.write_all(contents)
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn exists(&self, rel_path: &Path) -> bool {
        if validate_relative(rel_path).is_err() {
            return false;
        }
        self.root.join(rel_path).exists()
    }
}

const TEMPLATES_ROOT: &str = ".maestro/templates";

struct Embedded {
    rel_path: &'static str,
    contents: &'static [u8],
}

const FILES: &[Embedded] = &[
    Embedded {
        rel_path: "README.md",
        contents: include_bytes!("../../template/.maestro/templates/README.md"),
    },
    Embedded {
        rel_path: "manifest.toml",
        contents: include_bytes!("../../template/.maestro/templates/manifest.toml"),
    },
    Embedded {
        rel_path: "core/premises.md",
        contents: include_bytes!("../../template/.maestro/templates/core/premises.md"),
    },
    Embedded {
        rel_path: "core/tdd-cycle.md",
        contents: include_bytes!("../../template/.maestro/templates/core/tdd-cycle.md"),
    },
    Embedded {
        rel_path: "core/dependency-graph.md",
        contents: include_bytes!("../../template/.maestro/templates/core/dependency-graph.md"),
    },
    Embedded {
        rel_path: "commands/.gitkeep",
        contents: include_bytes!("../../template/.maestro/templates/commands/.gitkeep"),
    },
    Embedded {
        rel_path: "commands/implement.md",
        contents: include_bytes!("../../template/.maestro/templates/commands/implement.md"),
    },
    Embedded {
        rel_path: "commands/plan-feature.md",
        contents: include_bytes!("../../template/.maestro/templates/commands/plan-feature.md"),
    },
    Embedded {
        rel_path: "commands/pushup.md",
        contents: include_bytes!("../../template/.maestro/templates/commands/pushup.md"),
    },
    Embedded {
        rel_path: "commands/simplify.md",
        contents: include_bytes!("../../template/.maestro/templates/commands/simplify.md"),
    },
];

/// Walk the embedded file list, write each into `s` under
/// `.maestro/templates/`, skipping any file that already exists.
pub fn scaffold_templates_dir(s: &dyn Scaffolder) -> Result<ScaffoldReport> {
    let mut report = ScaffoldReport::default();
    for embed in FILES {
        let rel = PathBuf::from(TEMPLATES_ROOT).join(embed.rel_path);
        if s.exists(&rel) {
            report.files.push(ScaffoldedFile {
                path: rel,
                action: ScaffoldAction::Skipped,
            });
            continue;
        }
        s.write(&rel, embed.contents)
            .with_context(|| format!("writing scaffolded template {}", rel.display()))?;
        report.files.push(ScaffoldedFile {
            path: rel,
            action: ScaffoldAction::Created,
        });
    }
    Ok(report)
}

/// Stable list of relative paths (under the project root) that
/// `scaffold_templates_dir` writes.
pub fn template_relative_paths() -> Vec<PathBuf> {
    FILES
        .iter()
        .map(|f| PathBuf::from(TEMPLATES_ROOT).join(f.rel_path))
        .collect()
}

#[cfg(test)]
#[path = "scaffold_tests.rs"]
mod tests;
