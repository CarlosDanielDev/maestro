# ERRORS.md

Error log for `maestro`. When any approach needs more than 2 attempts to land, append an entry: what didn't work, what worked instead, note for next time. Check before suggesting a similar approach.

Format:

```
## YYYY-MM-DD — short title

**Context:** <what we were trying to do>
**What didn't work:**
- attempt 1: <approach + how it failed>
- attempt 2: <approach + how it failed>
**What worked:** <the approach that landed>
**Note for next time:** <one-line lesson>
```

---

## 2026-11-12 — dependabot PR green locally but red on CI after a fix push

**Context:** Fixing failing dependabot PRs (#1119 sha2 0.11, #1121 toml 1.2) via agents in worktrees at the PR branch head.
**What didn't work:**
- attempt 1: agent fixed the code breakage, passed the full gate locally in the worktree, pushed — CI still red (`Clippy`, `Test`, `Docs Autogen Drift` all failing in ~1s each).
- Root cause: CI checks out the PR **merge ref** (branch + current main). Main had moved under the PR (#1137 signing deps, #1118 directories 6, #1120 pulldown 0.13, #1117 syn 3) — the textual `Cargo.lock` merge is stale vs the merged manifest, and CI runs `cargo clippy --locked` / `cargo test --verbose --locked`, which refuse to touch the lock: `error: cannot update the lock file ... --locked was passed`. A worktree at the branch head alone never sees this.
**What worked:** `git merge origin/main` into the PR branch, let cargo minimally repair the auto-merged lock (`cargo check` without `--locked`, review delta, `cargo check --locked` must exit 0), re-run the gate **with CI's `--locked` flags**, plain push. Dual-version stacks (sha2 0.11.1 direct + 0.10.9 via scrypt/minisign) are legitimate cargo resolution, not a problem to flatten.
**Note for next time:** A green local gate on a dependabot branch proves nothing about CI when main is moving — always merge `origin/main` into the branch first and always gate with the exact CI flags (`--locked`). Second PR in a serial merge pair needs one more main-merge round after the first squash lands.

---

## 2026-11-14 — vhs demo capture took 4 recordings

**What didn't work:**
- `maestro init` now writes `[agents]`; the old capture.sh appended a second `[agents]` table.
- Fake HOME hid gh auth (`GH_CONFIG_DIR` alone not enough on macOS) → red "GitHub CLI not authenticated" banner.
- After submitting a prompt, the TUI lands on Overview; the next `r` did not open a new prompt → only 1 agent.
- vhs tapes have no `F1`–`F12` keys (`Invalid command: F4`).
- `c` on Overview copies the response to the clipboard → toast in the video.
- Recording from the scratchpad leaked `/private/tmp/claude-501/-Users-<name>-…` in the worktree log line.
**What worked:** export `GH_TOKEN="$(gh auth token)"` before swapping HOME; `Escape` after each prompt submit; letter keys (`t` Token Report) instead of F-keys; record under `/tmp/maestro-demo.*`; OCR every frame (`tesseract`) and grep for names/paths/tokens.
**Note for next time:** contact-sheet the webm (`ffmpeg -vf fps=1/2.5,tile=4x6`) after every take — one image shows the whole flow.

---

## 2026-11-14 — docs-only CI skip let a broken test reach main

**What didn't work:** `scripts/ci-code-changed.sh` (#1212) treated all of `docs/assets/**` as docs-only. #1213 changed `docs/assets/capture.sh`; Test was skipped; `tests/capture_script.rs` (which reads that file) went red on main. `AGENTS.md`/`GEMINI.md` had the same hole (`tests/templates_golden_rules.rs`).
**What worked:** media-only for `docs/assets/`, explicit code exceptions, and a self-check that fails when `src/`/`tests/`/`build.rs` names a docs-only path (#1219), mutation-checked.
**Note for next time:** before marking a path "skippable", grep the code and tests for it by path. Any skip rule needs a guard that fails when a test starts reading a skipped file.

---

## 2026-11-14 — byte-identical snapshot AC broken by a CHANGELOG bullet

**What didn't work:** adding the #1218 entry under `[Unreleased]` changed `landing_welcome_180x48` (Ref Trend sparkline counts every version, incl. `[Unreleased]`).
**What worked:** move the CHANGELOG entry into the release PR (which re-blesses version-drift snapshots anyway).
**Note for next time:** landing-touching PRs never edit CHANGELOG.md; the release PR does.

---

## 2026-11-14 — zsh does not word-split `set -- $var`

**What didn't work:** measuring vhs terminal sizes with `for cfg in "16 1280 800" …; do set -- $cfg` in the Bash tool (zsh). `$1` held the whole string; two rounds wasted.
**What worked:** run the loop under `bash -c '…'`.
**Note for next time:** the Bash tool's shell is zsh; use `bash -c` for scripts that rely on word splitting.

---

## 2026-11-14 — guard hooks scan the whole command text

**What didn't work:** a heredoc writing notes that *mentioned* a force branch delete was blocked by the forbidden-git guard; the whole command (writes, commit, push) did not run.
**What worked:** reword the note so the literal command does not appear.
**Note for next time:** never put guarded command strings inside heredocs or commit messages sent through the Bash tool.

---

## 2026-11-14 — flaky tests during gates

**What didn't work:** (1) `implement-gates.sh` baseline failed on `tui::markdown::tests::syntax_highlighting_200_lines_performance` (583 ms vs 500 ms under load); (2) release PR Coverage Tiers failed on `qwen_tests::run_streams_events_from_mock_qwen_cli` with ETXTBSY ("Text file busy").
**What worked:** rerun alone (3/3 pass) / `gh run rerun --failed`; both unrelated to the change.
**Note for next time:** both are known-flaky; worth a follow-up issue (timing budget under load; write-then-exec race in the mock CLI helper).
**Fixed (#1356):** the timing half. `syntax_highlighting_200_lines_performance` and `scan_120_transcripts_completes_under_1s` are now `#[ignore = "perf: ..."]`, so the gate's nextest skips them; CI runs them alone (shard 1, step "Run perf tests alone", `--run-ignored only --test-threads 1`). A 10x slowdown still fails that step (mutation-checked: 3759 ms vs 500 ms, 1.05 s vs 200 ms). Run locally: same command as that CI step. New wall-clock tests: follow the same pattern, do not put them in the gate.
