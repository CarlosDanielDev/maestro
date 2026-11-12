use super::*;
use tempfile::TempDir;

fn write_gitignore(dir: &TempDir, content: &str) {
    std::fs::write(dir.path().join(".gitignore"), content).expect("write .gitignore");
}

fn read_gitignore(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join(".gitignore")).expect("read .gitignore")
}

fn count_line(content: &str, needle: &str) -> usize {
    content.lines().filter(|line| *line == needle).count()
}

fn block_lines(content: &str) -> Vec<&str> {
    let start = content
        .lines()
        .position(|line| line == BLOCK_START)
        .expect("block start marker");
    let end = content
        .lines()
        .position(|line| line == BLOCK_END)
        .expect("block end marker");
    content
        .lines()
        .skip(start + 1)
        .take(end - start - 1)
        .collect()
}

#[test]
fn creates_gitignore_with_all_five_entries_once() {
    // Done-when #1151: a fresh init writes the five runtime entries,
    // each exactly once, inside the marked block.
    let dir = TempDir::new().expect("create tempdir");
    let update = ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    assert!(update.created, "missing .gitignore must be created");
    assert_eq!(update.added, RUNTIME_ENTRIES.to_vec(), "all five added");
    let content = read_gitignore(&dir);
    for entry in RUNTIME_ENTRIES {
        assert_eq!(
            count_line(&content, entry),
            1,
            "entry {entry} must appear exactly once:\n{content}"
        );
    }
    let inside = block_lines(&content);
    for entry in RUNTIME_ENTRIES {
        assert!(
            inside.contains(entry),
            "entry {entry} must live inside the maestro block:\n{content}"
        );
    }
}

#[test]
fn rerun_is_idempotent() {
    // Done-when #1151: re-running writes the entries "once" — a second
    // call must not duplicate or modify anything.
    let dir = TempDir::new().expect("create tempdir");
    ensure_runtime_gitignore(dir.path()).expect("first run");
    let after_first = read_gitignore(&dir);
    let update = ensure_runtime_gitignore(dir.path()).expect("second run");
    assert!(
        update.added.is_empty(),
        "second run must add nothing: {:?}",
        update.added
    );
    assert!(!update.created, "second run must not report a create");
    assert_eq!(
        read_gitignore(&dir),
        after_first,
        "second run must leave the file byte-identical"
    );
}

#[test]
fn preserves_user_content_and_adds_only_missing_entries() {
    // Done-when #1151: an existing .gitignore keeps its content and gets
    // only the entries it lacked (the QA repo already ignored
    // maestro-state.json).
    let dir = TempDir::new().expect("create tempdir");
    write_gitignore(&dir, "__pycache__/\n*.pyc\nmaestro-state.json\n");
    let update = ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    assert!(!update.created);
    assert_eq!(
        update.added,
        vec![
            ".maestro/logs/",
            ".maestro/worktrees/",
            "maestro-state.json.lock",
            "maestro.log"
        ],
        "only the missing entries are added"
    );
    let content = read_gitignore(&dir);
    assert!(
        content.starts_with("__pycache__/\n*.pyc\nmaestro-state.json\n"),
        "user content must survive verbatim at the top:\n{content}"
    );
    assert_eq!(
        count_line(&content, "maestro-state.json"),
        1,
        "already-present entry must not be duplicated:\n{content}"
    );
    for entry in update.added.iter() {
        assert_eq!(
            count_line(&content, entry),
            1,
            "missing entry {entry} added once:\n{content}"
        );
    }
}

#[test]
fn appends_without_corrupting_unterminated_last_line() {
    // A user file without a trailing newline must not have its final
    // pattern glued to the appended block.
    let dir = TempDir::new().expect("create tempdir");
    write_gitignore(&dir, "target/\n*.pyc");
    ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    let content = read_gitignore(&dir);
    assert!(
        content.starts_with("target/\n*.pyc\n"),
        "user's final pattern must stay on its own line:\n{content}"
    );
    assert!(
        content.contains(&format!("*.pyc\n\n{BLOCK_START}")),
        "blank line separates user content from the maestro block:\n{content}"
    );
}

#[test]
fn noop_when_all_entries_already_present() {
    // Nothing missing → the file must stay byte-identical (no rewrite).
    let dir = TempDir::new().expect("create tempdir");
    let mut full = String::from("# my ignore\n");
    full.push_str(BLOCK_START);
    full.push('\n');
    for entry in RUNTIME_ENTRIES {
        full.push_str(entry);
        full.push('\n');
    }
    full.push_str(BLOCK_END);
    full.push('\n');
    write_gitignore(&dir, &full);
    let update = ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    assert!(
        update.added.is_empty() && !update.created,
        "nothing to do: {update:?}"
    );
    assert_eq!(read_gitignore(&dir), full, "file must stay byte-identical");
}

#[test]
fn partial_block_refills_missing_inside_the_block() {
    // A user who removed one entry from the block gets it back inside the
    // existing block — never as a second block.
    let dir = TempDir::new().expect("create tempdir");
    let mut partial = String::from("# runtime\n");
    partial.push_str(BLOCK_START);
    partial.push('\n');
    for entry in [
        ".maestro/logs/",
        ".maestro/worktrees/",
        "maestro-state.json",
        "maestro-state.json.lock",
    ] {
        partial.push_str(entry);
        partial.push('\n');
    }
    partial.push_str(BLOCK_END);
    partial.push('\n');
    write_gitignore(&dir, &partial);

    let update = ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    assert_eq!(
        update.added,
        vec!["maestro.log"],
        "only the removed entry is restored"
    );
    let content = read_gitignore(&dir);
    assert_eq!(
        count_line(&content, BLOCK_START),
        1,
        "no second block:\n{content}"
    );
    assert_eq!(
        count_line(&content, BLOCK_END),
        1,
        "no second block:\n{content}"
    );
    assert_eq!(count_line(&content, "maestro.log"), 1, "refilled once");
    assert!(
        block_lines(&content).contains(&"maestro.log"),
        "refill lands inside the existing block:\n{content}"
    );
    assert!(
        content.starts_with("# runtime\n"),
        "user content untouched:\n{content}"
    );
}
