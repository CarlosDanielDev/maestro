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
fn preserves_user_content_and_adds_all_entries_missing_from_the_block() {
    // Done-when #1151: an existing .gitignore keeps its content and gets
    // the entries its maestro block lacked. The user's own exact line
    // (the QA repo already ignored maestro-state.json) does NOT count as
    // coverage — only lines inside maestro's block do (security
    // follow-up: a foreign line may sit before a later negation and lose
    // last-match-wins) — so the entry is added inside the block too.
    let dir = TempDir::new().expect("create tempdir");
    write_gitignore(&dir, "__pycache__/\n*.pyc\nmaestro-state.json\n");
    let update = ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    assert!(!update.created);
    assert_eq!(
        update.added,
        RUNTIME_ENTRIES.to_vec(),
        "every entry missing from the (absent) block is added"
    );
    let content = read_gitignore(&dir);
    assert!(
        content.starts_with("__pycache__/\n*.pyc\nmaestro-state.json\n"),
        "user content must survive verbatim at the top:\n{content}"
    );
    let inside = block_lines(&content);
    for entry in RUNTIME_ENTRIES {
        assert!(
            inside.contains(entry),
            "entry {entry} must live inside the appended block:\n{content}"
        );
    }
    // The user's own line stays; the block's copy lands after it, so the
    // ignore holds under last-match-wins.
    assert_eq!(
        count_line(&content, "maestro-state.json"),
        2,
        "user's own line plus maestro's block entry:\n{content}"
    );
    assert_eq!(count_line(&content, BLOCK_START), 1, "single block");
    assert_eq!(count_line(&content, BLOCK_END), 1, "single block");
}

#[test]
fn foreign_exact_line_and_negation_outside_block_cannot_suppress_append() {
    // Security follow-up #1151: outside maestro's block, an exact entry
    // line can sit before a later `!entry` negation — gitignore's
    // last-match-wins then re-includes the path while whole-file dedup
    // would believe it covered. Only lines inside maestro's own block
    // count as coverage, and the appended block lands after every foreign
    // line, so maestro's entry wins the last-match comparison.
    let dir = TempDir::new().expect("create tempdir");
    write_gitignore(
        &dir,
        "__pycache__/\nmaestro-state.json\n!maestro-state.json\n",
    );
    let update = ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    assert_eq!(
        update.added,
        RUNTIME_ENTRIES.to_vec(),
        "foreign lines are not coverage:\n{update:?}"
    );
    let content = read_gitignore(&dir);
    assert!(
        content.starts_with("__pycache__/\nmaestro-state.json\n!maestro-state.json\n"),
        "user content must survive verbatim at the top:\n{content}"
    );
    let inside = block_lines(&content);
    for entry in RUNTIME_ENTRIES {
        assert!(
            inside.contains(entry),
            "entry {entry} must live inside maestro's block:\n{content}"
        );
    }
    assert!(
        content.ends_with(&format!("{BLOCK_END}\n")),
        "block must land after the foreign lines:\n{content}"
    );
}

#[test]
fn refuses_symlinked_gitignore_and_leaves_victim_untouched() {
    // Security follow-up #1151 (CWE-59): a repo-planted `.gitignore`
    // symlink must be refused, never followed on read or write.
    let dir = TempDir::new().expect("create tempdir");
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, "ORIGINAL").expect("write victim");
    use std::os::unix::fs::symlink;
    symlink(&victim, dir.path().join(".gitignore")).expect("create symlink");

    let err = ensure_runtime_gitignore(dir.path()).expect_err("symlink must be refused");

    assert!(
        err.to_string().contains("symlink"),
        "error should name the symlink refusal, got: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(&victim).expect("victim readable"),
        "ORIGINAL",
        "victim file must be untouched"
    );
    assert!(
        std::fs::symlink_metadata(dir.path().join(".gitignore"))
            .expect("link still present")
            .file_type()
            .is_symlink(),
        ".gitignore must still be the symlink"
    );
}

#[test]
fn refuses_dangling_symlink_gitignore() {
    // `Path::exists` reports a dangling symlink as absent and would let the
    // create path run; exists_no_follow sees the link and the run refuses
    // instead of creating the target through it.
    let dir = TempDir::new().expect("create tempdir");
    let victim = dir.path().join("victim.txt");
    use std::os::unix::fs::symlink;
    symlink(&victim, dir.path().join(".gitignore")).expect("create dangling symlink");

    let err = ensure_runtime_gitignore(dir.path()).expect_err("dangling symlink must be refused");

    assert!(err.to_string().contains("symlink"), "got: {err}");
    assert!(
        !victim.exists(),
        "dangling symlink target must not be created"
    );
    assert!(
        std::fs::symlink_metadata(dir.path().join(".gitignore"))
            .expect("link still present")
            .file_type()
            .is_symlink(),
        ".gitignore must still be the dangling symlink"
    );
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

#[test]
fn negation_after_block_end_re_adds_entry_after_it() {
    // Security follow-up #1151: a `!entry` negation placed AFTER maestro's
    // complete block wins under gitignore's last-match-wins, so the block
    // alone no longer covers that entry. maestro must append a new block at
    // the end of the file (after the negation) carrying that entry, and a
    // re-run must then be a byte-identical no-op.
    let dir = TempDir::new().expect("create tempdir");
    let mut seeded = String::from(BLOCK_START);
    seeded.push('\n');
    for entry in RUNTIME_ENTRIES {
        seeded.push_str(entry);
        seeded.push('\n');
    }
    seeded.push_str(BLOCK_END);
    seeded.push_str("\n!maestro-state.json\n");
    write_gitignore(&dir, &seeded);

    let update = ensure_runtime_gitignore(dir.path()).expect("ensure_runtime_gitignore");
    assert_eq!(
        update.added,
        RUNTIME_ENTRIES.to_vec(),
        "any negation after the block voids coverage (globs are not parsed, #1196)"
    );
    let content = read_gitignore(&dir);
    assert!(
        content.starts_with(&seeded),
        "existing content (negation included) survives verbatim:\n{content}"
    );
    let tail = &content[seeded.len()..];
    assert!(
        tail.starts_with(&format!("\n{BLOCK_START}\n"))
            && tail.ends_with(&format!("{BLOCK_END}\n")),
        "re-add lands in a new block after the negation:\n{content}"
    );
    assert!(
        tail.lines().any(|line| line == "maestro-state.json"),
        "the negated entry is in the new block:\n{content}"
    );

    let rerun = ensure_runtime_gitignore(dir.path()).expect("second run");
    assert!(rerun.added.is_empty(), "second run adds nothing: {rerun:?}");
    assert_eq!(read_gitignore(&dir), content, "second run is a no-op");
}
