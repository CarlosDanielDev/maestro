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
