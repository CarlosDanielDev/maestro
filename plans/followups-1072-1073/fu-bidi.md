Parent: #1074

Found by the judge of PR #1073 (adopt picker). `sanitize_log` (`src/util/sanitize.rs:18`) replaces every char where `char::is_control()` is true with a space, which covers C0 (ESC, BEL) and C1 (CSI U+009B). Unicode **format** characters are not `is_control()`, so they pass through: bidi overrides U+202A-U+202E, isolates U+2066-U+2069, and the line/paragraph separators U+2028/U+2029. A hostile transcript `cwd` or title can therefore visually spoof the path shown in the adopt picker's detail line and confirm modal (`src/tui/screens/adopt_picker/draw.rs`), e.g. a directory name that renders reversed.

## Reprodução

1. Create a directory whose name contains U+202E (RIGHT-TO-LEFT OVERRIDE), e.g. `mkdir "$(printf 'safe\xe2\x80\xaegnp.txt')"`.
2. Pass that path through `sanitize_log` in a unit test: `assert!(!out.contains('\u{202e}'))`.
   Saída observada (a real, não a esperada): the assertion fails — U+202E is returned unchanged.

## Done when

- [ ] `sanitize_log` also replaces Unicode format/bidi controls (at least U+202A-U+202E, U+2066-U+2069, U+2028, U+2029, U+200B-U+200F) — `sanitize_log_strips_bidi_and_format_controls` unit test passes, with a mutation check (remove the new arm → test fails) — owner: implementer
- [ ] existing `sanitize_log` callers keep their behavior for ordinary text including non-ASCII letters (accents, CJK, emoji) — `sanitize_log_keeps_ordinary_unicode` test passes and `cargo test` is green — owner: implementer

## Out of scope

- The picker rendering itself (already sanitizes through this helper) and the activity-log sinks (fixed in #1073).

## Handoff

```bash
git worktree add -b fix/1074-sanitize-bidi ../maestro-1074 origin/main
```

`codegraph explore "sanitize_log"` and read `src/util/sanitize.rs`. Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1074`.
