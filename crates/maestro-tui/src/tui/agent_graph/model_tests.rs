use std::path::PathBuf;

use uuid::Uuid;

use super::*;
use crate::session::types::SessionStatus;

fn agent(n: u128, worktree: &str, touched: &[&str]) -> Session {
    let mut s = Session::new(String::new(), String::new(), String::new(), None, None);
    s.id = Uuid::from_u128(n);
    s.status = SessionStatus::Running;
    s.worktree_path = Some(PathBuf::from(worktree));
    s.files_touched = touched.iter().map(|p| format!("{worktree}/{p}")).collect();
    s
}

/// #1292: a file two agents edit sorts first and is CONTESTED; a file one
/// agent edits is EDITED; reads never make a file contested.
#[test]
fn files_lens_contested_first() {
    let a = agent(1, "/wt/a", &["src/a.rs", "src/shared.rs", "README.md"]);
    let b = agent(2, "/wt/b", &["src/b.rs", "README.md"]);
    let mut claims = FileClaimManager::new();
    let _ = claims.claim("src/a.rs", a.id);
    let _ = claims.claim("src/shared.rs", a.id);
    let _ = claims.claim("src/b.rs", b.id);
    // b's edit of src/shared.rs clashed with a's claim.
    claims.record_conflict("src/shared.rs", a.id, b.id);

    let g = build_graph(&[&a, &b], &claims);

    let paths: Vec<&str> = g.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        ["src/shared.rs", "src/a.rs", "src/b.rs", "README.md"],
        "contested, then edited by barycenter, then read only"
    );
    let shared = &g.files[0];
    assert!(shared.contested() && shared.conflict);
    assert_eq!(shared.editors, [a.id, b.id]);
    assert_eq!(shared.holder, Some(a.id));
    assert!(!g.files[1].contested(), "one editor is not contested");
    let readme = &g.files[3];
    assert!(!readme.contested(), "two readers are not a conflict");
    assert_eq!(readme.readers, [a.id, b.id]);
    assert_eq!(readme.edge(a.id), Some(EdgeKind::Read));
    assert_eq!(g.files[1].edge(a.id), Some(EdgeKind::Edit));
    assert_eq!((g.contested(), g.edited(), g.edits_of(a.id)), (1, 3, 2));
}

/// Worktrees are merged: the same repo file read from two worktrees is
/// one row, keyed repo-relative.
#[test]
fn files_lens_keys_reads_repo_relative() {
    let a = agent(1, "/wt/a", &["src/x.rs"]);
    let b = agent(2, "/wt/b", &["src/x.rs"]);
    let g = build_graph(&[&a, &b], &FileClaimManager::new());
    assert_eq!(g.files.len(), 1);
    assert_eq!(g.files[0].path, "src/x.rs");
}

#[test]
fn agent_label_prefers_issue_number() {
    let mut s = agent(0xabcd, "/wt", &[]);
    assert!(agent_label(&s).starts_with("S-0000"));
    s.issue_number = Some(513);
    assert_eq!(agent_label(&s), "#513");
}
