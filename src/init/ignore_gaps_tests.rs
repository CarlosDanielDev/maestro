//! Coverage gaps closed by #1196: glob negations, a single no-follow read
//! handle, and git-exact line matching.

use super::*;
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;

fn full_block() -> String {
    let mut block = format!("{BLOCK_START}\n");
    for entry in RUNTIME_ENTRIES {
        block.push_str(entry);
        block.push('\n');
    }
    block.push_str(BLOCK_END);
    block.push('\n');
    block
}

fn gitignore(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join(".gitignore")
}

/// Run `f` on a worker thread; panic instead of hanging past 5 seconds.
fn within_timeout<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("call must not hang on a FIFO")
}

fn make_fifo(path: &Path) {
    let status = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "mkfifo failed: {status}");
}

#[test]
fn glob_negation_after_block_end_re_adds_every_entry_after_it() {
    // A glob negation such as `!*.json` after maestro's block can re-include
    // runtime files (last match wins). Globs are not parsed, so any `!` line
    // after the block voids coverage and a fresh block follows it.
    let dir = TempDir::new().expect("create tempdir");
    let seeded = format!("{}!*.json\n", full_block());
    std::fs::write(gitignore(&dir), &seeded).expect("seed");

    let update = ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    assert_eq!(update.added, RUNTIME_ENTRIES.to_vec());
    let content = std::fs::read_to_string(gitignore(&dir)).expect("read");
    assert_eq!(
        content,
        format!("{seeded}\n{}", full_block()),
        "fresh block lands after the glob negation"
    );

    let rerun = ensure_runtime_gitignore(dir.path()).expect("second run");
    assert!(rerun.added.is_empty(), "second run adds nothing: {rerun:?}");
    assert_eq!(
        std::fs::read_to_string(gitignore(&dir)).expect("read"),
        content
    );
}

#[test]
fn read_refuses_symlink_on_the_open_itself() {
    // The read must not follow a symlink even when it points at a regular
    // file, so a link swapped in after any pre-check is still refused.
    let dir = TempDir::new().expect("create tempdir");
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, "SECRET").expect("write victim");
    std::os::unix::fs::symlink(&victim, gitignore(&dir)).expect("symlink");

    let err = read_existing(&gitignore(&dir)).expect_err("symlink must be refused");
    assert!(err.to_string().contains("symlink"), "got: {err:#}");
}

#[test]
fn read_refuses_fifo_without_hanging() {
    let dir = TempDir::new().expect("create tempdir");
    let path = gitignore(&dir);
    make_fifo(&path);

    let result = within_timeout(move || read_existing(&path).map_err(|e| format!("{e:#}")));
    let err = result.expect_err("FIFO must be refused");
    assert!(err.contains("not a regular file"), "got: {err}");
}

#[test]
fn ensure_refuses_fifo_gitignore_without_hanging() {
    let dir = TempDir::new().expect("create tempdir");
    make_fifo(&gitignore(&dir));
    let root = dir.path().to_path_buf();

    let result = within_timeout(move || ensure_runtime_gitignore(&root).map(|_| ()));
    assert!(result.is_err(), "FIFO .gitignore must be refused");
}

#[test]
fn block_line_with_leading_space_is_not_coverage() {
    // Git keeps leading spaces, so ` maestro.log` does not ignore
    // `maestro.log`; maestro must add the exact line.
    let dir = TempDir::new().expect("create tempdir");
    let seeded = full_block().replace("\nmaestro.log\n", "\n maestro.log\n");
    std::fs::write(gitignore(&dir), &seeded).expect("seed");

    let update = ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    assert_eq!(update.added, vec!["maestro.log"]);
}

#[test]
fn git_pattern_strips_only_what_git_strips() {
    assert_eq!(git_pattern("maestro.log"), "maestro.log");
    assert_eq!(
        git_pattern("maestro.log  "),
        "maestro.log",
        "trailing spaces"
    );
    assert_eq!(git_pattern("maestro.log\r"), "maestro.log", "trailing CR");
    assert_eq!(
        git_pattern("maestro.log \r"),
        "maestro.log",
        "space then CR"
    );
    assert_eq!(
        git_pattern(" maestro.log"),
        " maestro.log",
        "leading space kept"
    );
    assert_eq!(
        git_pattern("\tmaestro.log"),
        "\tmaestro.log",
        "leading tab kept"
    );
    assert_eq!(
        git_pattern("maestro.log\t"),
        "maestro.log\t",
        "trailing tab kept"
    );
    assert_eq!(
        git_pattern("maestro.log\\ "),
        "maestro.log\\ ",
        "escaped space kept"
    );
}
