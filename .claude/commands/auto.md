<!--
  Manually-authored slash command. NOT rendered from .maestro/templates/.
  Lives only under .claude/commands/ — does not ship via `maestro init`
  scaffold. A future PR can promote this to canonical by:
    1. Moving the body back to .maestro/templates/commands/auto.md
    2. Mirroring to template/.maestro/templates/commands/auto.md
    3. Adding "auto" to COMMANDS in src/commands/sync_templates/registry.rs
    4. Bumping the locking test commands_const_lists_four_canonical_commands
    5. Bumping SLUGS in src/integration_tests/canonical_command_specs.rs
-->

---
command: auto
version: 1.0.0
description: End-to-end autonomous loop — /implement → /simplify → /pushup — for one GitHub issue. Files follow-ups with full DOR + dependency-graph wiring when scope splits. Blocks the PR from leaving Draft when src/tui/** is touched and the issue lacks an executed manual-QA matrix.
---

# Auto

Run the full ship-it loop for one GitHub issue without further prompts.

**Usage:** `/auto #<issue-number>` (e.g. `/auto #792`). Extra args pass through to `/implement`.

This command is **self-contained**. A fresh Claude Code session reading this file MUST be able to execute it without prior conversation context. Every decision is recoverable from disk + GitHub state.

---

## Premises (Maestro Orchestrator Mode)

The orchestrator (you) is the only agent that writes code. ALL subagents are consultive. The TDD cycle is mandatory: RED test → GREEN minimum implementation → REFACTOR. See `.claude/CLAUDE.md` for the canonical premises — they are non-negotiable here as well.

In `/auto`:

- Mode is **Orchestrator** (always).
- Language is **English** (always).
- Pre-check hook runs with `--dirty-tree-action=stash` (auto-stash, never abort on dirty tree).
- Gatekeeper DOR FAIL auto-posts the remediation comment + `needs-info` label.
- Existing branches are **continued**, not restarted.

---

## Default flags

Every `/auto` invocation behaves as if the user ran:

```
/implement #<n> -e -o --continue --auto-stash --auto-comment
```

These defaults are NOT optional. Do not ask the user; do not pause. If they want different behaviour, they invoke `/implement` directly.

---

## Arguments

Parse the first `\d+` (with optional leading `#`) from `$ARGUMENTS` as the issue number. Forward everything else to `/implement` verbatim.

If no issue number is present, **STOP** with: `/auto requires an issue number — e.g. /auto #123`.

---

## Steps (run in order; STOP on any non-zero exit)

### Step 0 — Sanity

1. Confirm `git rev-parse --show-toplevel` is non-empty.
2. Confirm `gh auth status` is clean.
3. Confirm `Cargo.toml` exists at the repo root (this command is Rust/maestro-specific).

Bail with one-line error on any failure.

### Step 1 — `/implement`

Invoke `/implement #<n> -e -o --continue --auto-stash --auto-comment` (plus any pass-through args). Forward its exit code as-is. If `/implement` exits non-zero, **STOP** — the user fixes the root cause and re-runs `/auto`.

`/implement` itself runs the full subagent sequence: pre-check hook → Gatekeeper → Architect (blueprint) → contract validation if APIs touched → QA (test blueprint) → write tests → RED gate → implement → GREEN gate → refactor → security analyst → docs analyst. None of those calls may be skipped.

### Step 2 — Scope-split → file follow-up issues

After `/implement` completes, read `$GATE_LOG_DIR/architect-blueprint.md` (the architect's report). If it explicitly flagged "split this scope", "out of scope for v1", "defer to follow-up", or any R* risk listed as "recommended: split":

For EACH carved-out risk, file a follow-up GitHub issue via `gh issue create` with:

- Title — `<type>(<scope>): <short description> (follow-up to #<n>)`.
- Body — full DOR template: Overview, Expected Behavior, Steps to Reproduce (if bug), Acceptance Criteria, Files to Modify, Test Hints, `## Blocked By` (pointing at `#<n>` or any sibling follow-up), Definition of Done.
- `--milestone` — same as the current issue, OR the next-version milestone if the current one needs to close on the current PR.
- `--assignee` — the user that opened the issue (`gh issue view <n> --json author --jq '.author.login'`).
- `--label` — appropriate labels from `gh label list`.

Update the target milestone's `## Dependency Graph (Implementation Order)` section so the new issue appears at the correct level. Update the `Sequence:` line. Use the canonical format documented in `.claude/CLAUDE.md` § 4.

If the new issue ships in a different milestone, use a `Cross-milestone hand-off (depends on <other-milestone>/#NNN):` block on the target milestone.

Post a comment on `#<n>` announcing each descope, linking the new issue.

### Step 3 — `/simplify`

Run `/simplify`. It re-runs `cargo fmt --check`, `cargo clippy -- -D warnings -A dead_code`, `cargo test`, calls the architect for an ETC / Demeter / Object Calisthenics quality pass, and applies low-risk dedupes only.

**Do NOT introduce new abstractions during simplify.** If the architect flags more than ~5 issues, file a separate refactor issue and revert `/simplify`'s edits.

Any simplify commits go on the same branch.

### Step 4 — Manual-QA gate (BLOCKING for UI work)

Determine whether the diff touches user-facing UI:

```bash
ui_touched=$(git diff main..HEAD --name-only | grep -E '^src/tui/' | wc -l | tr -d ' ')
```

If `ui_touched > 0`:

1. Read the issue body (`gh issue view <n> --json body --jq '.body'`).
2. Search for a `## Manual Test` or `## Manual QA Matrix` section.
3. If the section is **absent**:
   - **STOP** before `/pushup`.
   - Comment on the issue: `Auto-pause: this issue touches src/tui/** and lacks a manual-test script in its body. Add a "## Manual Test (Human)" section with the steps a human must run to validate this change, then re-run /auto #<n>.`
   - Apply label `needs-info` (or `needs-design`).
   - Exit with code `8`.
4. If the section is **present**:
   - Continue to `/pushup` — but capture the matrix verbatim into a variable.
   - After `/pushup` opens the PR, leave it in Draft (do NOT call `gh pr ready`).
   - Post a comment on the PR with the verbatim matrix prefixed by:
     > **Manual QA required before this PR can leave Draft.** Run the steps below on a local build (`cargo run`) and paste results (terminal screenshots or step-by-step ✓/✗) into a follow-up comment. Then run `gh pr ready <pr>` or click "Ready for review" to clear the gate.
   - Exit with code `8` after the comment is posted.

If `ui_touched == 0`, skip the gate. Backend / config / docs work proceeds straight through `/pushup`.

### Step 5 — `/pushup`

Invoke `/pushup #<n>` (explicit issue number — do NOT rely on auto-detection).

`/pushup` is responsible for: semantic commit via `scripts/commit-helper.sh`, push to origin with upstream, `gh pr create` (or `gh pr edit` if PR exists), milestone graph update via `python3 scripts/update-milestone-graph.py`, comment + close on the source issue, and the `~/.maestro/last-pr-created` marker for the TUI auto-review.

### Step 6 — Auto-close override (UI gate held PR in Draft)

If Step 4 exited with code `8` AND the PR is now Draft:

1. Re-open `#<n>` if `/pushup` Step 6.5 already closed it:
   ```bash
   if [ "$(gh issue view <n> --json state --jq '.state')" = "CLOSED" ]; then
     gh issue reopen <n> -c "Re-opening — PR #<pr> stays Draft pending manual QA per the gate in /auto Step 4."
   fi
   ```
2. Revert the milestone graph ✅ mark for `#<n>`:
   - Read the current description.
   - Replace `• ✅ #<n> <title>` with `• #<n> <title>`.
   - If the level was rolled up to `(COMPLETED ✅)` solely because `#<n>` was the last open issue, revert that header roll-up too.
   - Replace `✅(L<k>: #<n>)` in the `Sequence:` line with `#<n>`.
   - PATCH the milestone via `gh api repos/<owner>/<repo>/milestones/<milestone-number> -X PATCH -F description="..."`.
3. Post a comment on `#<n>` explaining the reopen.

This guarantees the dependency graph never lies about completion state when a PR is held in Draft.

### Step 7 — Summary

Print one final block the user can quote:

```
/auto complete for #<n>

  Branch:    feat/issue-<n>-*
  Commits:   <count>
  PR:        #<m> — <Title>  (<state: Draft|Ready>)
  Milestone: <name>  (graph: <updated|unchanged>)
  Follow-ups filed: <#A, #B, …>  (or "none")
  Manual QA: <NOT REQUIRED | PENDING (Draft) | RAN BY USER>
  Next:      <one-line concrete next step>
```

If `/auto` exits before Step 7, print a one-line reason + `Re-run: /auto #<n>` hint.

---

## Hard rules (non-negotiable; survive context resets)

1. **Never merge UI changes without a human-run manual-QA matrix.** Cargo tests are insufficient. The manual test in the issue body is the contract.
2. **Never close blocking issues prematurely.** `/pushup` auto-closes on PR open. If the PR returns to Draft (e.g. manual-QA gate held it), `/auto` re-opens the issue and reverts the milestone graph ✅.
3. **Always file follow-ups for descoped work with full DOR.** A descoped risk that doesn't ship in this PR MUST become a GitHub issue with `## Blocked By`, milestone, assignee, and a place in the dependency graph.
4. **Always update the milestone graph after issue closure or follow-up filing.** Both forward (mark ✅) and backward (unmark when re-opened) maintenance is required.
5. **Never introduce a new colour in a TUI change.** Reuse `theme.*`. New chrome reuses `theme.styled_block` or matches a documented sibling pattern. If you find yourself reaching for `branding_bg` as a popup fill, stop — it's the brand-strip colour, not a modal background.
6. **Never let a new keybind collide with the outer screen's chord set.** Outer Settings / Welcome / Dashboard own `Tab`, `BackTab`, `Up`, `Down`, `Enter`, `Esc`, `Ctrl+s`. Child widgets pick single-letter chords (`a`, `d`, `u`, `[`, `]`) or modifier chords distinct from terminal-level chords (`Alt+↑/↓` for reorder, NOT `Ctrl+←/→`).
7. **Always treat the architect's "split this scope" recommendations as binding.** If the architect says split, split. Do not paper over the warning with a single oversized PR.
8. **No silent stash drops.** Auto-stashed work stays in `git stash list` so the user can recover it. Never `git stash drop`.
9. **No design-spec issue is allowed to ship without a manual-QA matrix in its body.** If you draft a follow-up that lacks a matrix and the work touches `src/tui/**`, you have created tomorrow's stuck PR. Add the matrix in Step 2.

---

## Re-entry / fresh-context behaviour

`/auto` works without prior conversation context. A fresh session can run `/auto #<n>` on an existing branch and:

- Detect the existing `feat/issue-<n>-*` branch via `/implement` Step 5's `--continue` semantics.
- Re-create `$GATE_LOG_DIR` from the timestamped sentinel path written by `implement-gates.sh`.
- Read the issue body + `## Blocked By` + `## Manual Test` sections directly via `gh issue view`.
- Inspect the milestone dependency graph via `gh api repos/<owner>/<repo>/milestones/<n> --jq '.description'`.
- Find any prior follow-up issues via `gh issue list --search "follow-up to #<n> in:title,body" --json number,title`.

Nothing in this command depends on session memory.

---

## Exit codes

Inherits `/implement` (0 / 1 / 2 / 3 / 4 / 5 / 6 / 7+), plus:

- `8` — Manual-QA gate held the PR Draft (Step 4). Not an error; the user owns the next move.
- `9` — Scope split filed follow-ups. PR may still be Draft pending the design-lock issue. Same recovery as `8`.

---

## Do Not

- Run `/auto` for the same issue concurrently in two sessions (branch lock will fail; not graceful).
- Bypass any gate by invoking `/pushup` directly when `/auto` exited at Step 4 — the gate exists for a reason.
- Skip Step 6 — the dependency graph is the single source of truth for milestone progress.
- Write code or edit files outside the diff under review during Step 3 (`/simplify` is scoped).
- Auto-mark a PR Ready For Review when Step 4 returned `qa_required=1`. Only the human flips the Draft state.
