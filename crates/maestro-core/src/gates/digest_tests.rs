use super::*;

const CARGO_TEST: &str = "\
running 5 tests
test parser::tests::empty_input ... ok
test parser::tests::parses_header ... FAILED
test store::tests::round_trip ... FAILED
test parser::tests::keeps_order ... FAILED
test store::tests::loads ... ok

failures:

---- parser::tests::parses_header stdout ----

thread 'parser::tests::parses_header' panicked at src/parser.rs:42:9:
assertion `left == right` failed
  left: 3
 right: 4
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- store::tests::round_trip stdout ----

thread 'store::tests::round_trip' panicked at src/store.rs:88:5:
called `Result::unwrap()` on an `Err` value: NotFound

---- parser::tests::keeps_order stdout ----

thread 'parser::tests::keeps_order' panicked at src/parser.rs:57:13:
order lost

failures:
    parser::tests::keeps_order
    parser::tests::parses_header
    store::tests::round_trip

test result: FAILED. 2 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `--lib`
";

const CLIPPY: &str = "\
    Checking demo v0.1.0 (/tmp/demo)
warning: unneeded `return` statement
  --> src/lib.rs:3:5
   |
3  |     return x;
   |     ^^^^^^^^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_return
   = note: `-D clippy::needless-return` implied by `-D warnings`

error[E0308]: mismatched types
 --> src/main.rs:10:18
  |
10 |     let n: u32 = \"x\";
  |            ---   ^^^ expected `u32`, found `&str`

warning: unused variable: `y`
 --> src/lib.rs:7:9
  |
7 |     let y = 1;
  |         ^ help: if this is intentional, prefix it with an underscore: `_y`
  |
  = note: `#[warn(unused_variables)]` on by default

warning: `demo` (lib) generated 2 warnings
error: could not compile `demo` (bin \"demo\") due to 1 previous error
";

#[test]
fn digest_cargo_test() {
    let d = digest(CARGO_TEST);
    assert_eq!(d.summary, "3 failing · 2 passed");
    let paths: Vec<&str> = d.groups.iter().map(|g| g.path.as_str()).collect();
    assert_eq!(
        paths,
        ["src/parser.rs", "src/store.rs"],
        "by file, output order"
    );

    let parser = &d.groups[0].items;
    assert_eq!(parser.len(), 2);
    assert_eq!(parser[0].title, "parser::tests::parses_header");
    assert_eq!((parser[0].line, parser[0].col), (42, 9));
    assert_eq!(parser[0].kind, FailureKind::Test);
    assert_eq!(
        parser[0].body,
        ["assertion `left == right` failed", "  left: 3", " right: 4"],
        "panic message kept with its indentation; backtrace note dropped"
    );
    assert_eq!(parser[1].title, "parser::tests::keeps_order");
    assert_eq!((parser[1].line, parser[1].col), (57, 13));

    let store = &d.groups[1].items;
    assert_eq!(store.len(), 1);
    assert_eq!((store[0].line, store[0].col), (88, 5));
}

#[test]
fn digest_clippy() {
    let d = digest(CLIPPY);
    assert_eq!(d.summary, "1 error · 2 warnings");
    let paths: Vec<&str> = d.groups.iter().map(|g| g.path.as_str()).collect();
    assert_eq!(paths, ["src/lib.rs", "src/main.rs"]);

    let lib = &d.groups[0].items;
    assert_eq!(lib.len(), 2);
    assert_eq!(lib[0].title, "unneeded `return` statement");
    assert_eq!(lib[0].tag.as_deref(), Some("clippy::needless-return"));
    assert_eq!((lib[0].line, lib[0].col), (3, 5));
    assert_eq!(lib[0].kind, FailureKind::Warning);
    assert!(
        lib[0].body.iter().any(|l| l.contains("^^^^^^^^")),
        "the clippy caret is kept: {:?}",
        lib[0].body
    );
    assert_eq!(lib[1].title, "unused variable: `y`");
    assert_eq!(lib[1].tag.as_deref(), Some("unused_variables"));
    assert_eq!((lib[1].line, lib[1].col), (7, 9));

    let main = &d.groups[1].items;
    assert_eq!(main[0].title, "mismatched types");
    assert_eq!(main[0].tag.as_deref(), Some("E0308"));
    assert_eq!((main[0].line, main[0].col), (10, 18));
    assert_eq!(main[0].kind, FailureKind::Error);
}

#[test]
fn digest_drops_duplicate_diagnostics() {
    // rustc prints a lib warning again for the test build.
    let twice = format!("{CLIPPY}\n{CLIPPY}");
    let d = digest(&twice);
    assert_eq!(d.groups[0].items.len(), 2);
    assert_eq!(d.summary, "1 error · 2 warnings");
}

#[test]
fn digest_reads_ci_log_prefixes() {
    // `gh run view --log-failed` prefixes every line with job, step, time.
    let ci: String = CLIPPY
        .lines()
        .map(|l| format!("build\tClippy\t2026-11-24T10:00:00.1234567Z {l}\n"))
        .collect();
    let d = digest(&ci);
    assert_eq!(d.summary, "1 error · 2 warnings");
    assert_eq!(d.groups[0].path, "src/lib.rs");
}

#[test]
fn digest_reads_ci_log_prefixes_after_tabs_became_spaces() {
    // The CI fetch runs `sanitize_for_terminal`, which turns tabs into spaces.
    let ci: String = CLIPPY
        .lines()
        .map(|l| format!("build and test Run clippy 2026-11-24T10:00:00.1234567Z {l}\n"))
        .collect();
    let d = digest(&ci);
    assert_eq!(d.summary, "1 error · 2 warnings");
    assert_eq!(
        d.groups[0].items[0].body[0], "   |",
        "indentation after the prefix kept"
    );
}

#[test]
fn digest_of_unknown_output_is_empty() {
    let d = digest("Error: something went wrong\n");
    assert!(d.is_empty());
    assert!(d.summary.is_empty());
}
