//! Unit tests for `worktree_teardown`. Sibling file via `#[path]` so the
//! module stays under the 400-line gate cap (#1484).

use super::*;

#[test]
fn is_unsafe_root_slash_is_unsafe() {
    assert!(is_unsafe_root(Path::new("/"), None));
}

#[test]
fn is_unsafe_root_home_is_unsafe() {
    let home = Path::new("/home/user");
    assert!(is_unsafe_root(home, Some(home)));
}

#[test]
fn is_unsafe_root_normal_root_is_safe() {
    assert!(!is_unsafe_root(
        Path::new("/tmp/maestro/worktrees"),
        Some(Path::new("/home/user"))
    ));
}

#[test]
fn is_unsafe_root_home_subdir_is_safe() {
    // A subdir of HOME is fine — only HOME *exactly* is unsafe.
    assert!(!is_unsafe_root(
        Path::new("/home/user/projects/foo"),
        Some(Path::new("/home/user"))
    ));
}

#[test]
fn is_worktree_absent_matches_known_phrase() {
    assert!(is_worktree_absent("fatal: '/tmp/wt' is not a working tree"));
}

#[test]
fn is_worktree_absent_false_for_other_errors() {
    assert!(!is_worktree_absent("fatal: not a git repository"));
}

#[test]
fn is_branch_absent_matches_known_phrase() {
    assert!(is_branch_absent("error: branch 'feat/issue-740' not found"));
}

#[test]
fn is_branch_absent_false_for_other_errors() {
    assert!(!is_branch_absent(
        "error: Cannot delete branch 'main' checked out at '/repo'"
    ));
}

#[test]
fn sanity_check_out_of_root_when_path_escapes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("root");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&root).expect("mkdir root");
    std::fs::create_dir_all(&outside).expect("mkdir outside");

    let result = sanity_check_under_root(&outside, &root);
    assert!(
        matches!(result, Err(TeardownError::OutOfRoot { .. })),
        "expected OutOfRoot, got {result:?}"
    );
}

#[test]
fn sanity_check_refuses_empty_root() {
    // The interaction cwd-fallback derives an empty root; it must be
    // refused, never canonicalized against cwd (#741 sec).
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("wt");
    std::fs::create_dir_all(&path).expect("mkdir wt");
    let result = sanity_check_under_root(&path, Path::new(""));
    assert!(
        matches!(result, Err(TeardownError::UnsafeRoot(_))),
        "expected UnsafeRoot for empty root, got {result:?}"
    );
}

#[test]
fn sanity_check_refuses_relative_root() {
    let result = sanity_check_under_root(Path::new("."), Path::new(".maestro/worktrees"));
    assert!(
        matches!(result, Err(TeardownError::UnsafeRoot(_))),
        "expected UnsafeRoot for relative root, got {result:?}"
    );
}

#[test]
fn sanity_check_none_when_path_missing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("does_not_exist");
    let result = sanity_check_under_root(&path, tmp.path());
    assert!(
        matches!(result, Ok(None)),
        "expected Ok(None), got {result:?}"
    );
}

#[test]
fn sanity_check_some_when_path_inside_root() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("wt-1");
    std::fs::create_dir_all(&path).expect("mkdir wt");

    let result = sanity_check_under_root(&path, tmp.path());
    match result {
        Ok(Some(canon)) => {
            let canon_root = tmp.path().canonicalize().expect("canon root");
            assert!(
                canon.starts_with(&canon_root),
                "returned path {canon:?} must be under root {canon_root:?}"
            );
        }
        other => panic!("expected Ok(Some(_)), got {other:?}"),
    }
}
