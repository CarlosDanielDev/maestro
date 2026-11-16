# TUI design decisions and learnings

Dated decisions about the TUI design, newest first. Add an entry in the same
PR when the code must differ from [index.html](index.html), or when a session
learns something the design did not foresee. Session-end summaries still go
to the repo `MEMORY.md`; for TUI design, `MEMORY.md` only links here.

## Entry template

Copy this block to the top of the entries (below this template), newest
first. One entry per decision.

```markdown
## YYYY-MM-DD — <short title>

- **What:** the decision or learning, in one or two sentences.
- **Why:** the reason or the evidence.
- **Section:** the `index.html` section it touches (for example `§02.14 Usage`).
- **Rejected:** the alternative not chosen, and why.
- **Source:** issue / PR / session (for example `#1238`).
```

---

## 2026-11-14 — Release Console landing and UX Spine sidebar are superseded

- **What:** the redesign wins over two logged decisions: the one-landing Release Console renderer (#1218) and the UX Spine sidebar chain (#826-#835). Both are superseded by `index.html`.
- **Why:** the redesign replaces the landing with Home: Mission Control and defines the navigation the UX Spine chain was building toward.
- **Section:** `§01 Audit of the v0.31.6 landing screen`, `§02 Home: Mission Control`, `§09 Code findings & open questions`.
- **Rejected:** keeping the Release Console as the landing, or finishing #826-#835 as specced.
- **Source:** #1238 (poka-yoke, owner-delegated).

## 2026-11-14 — D1: budget at 100% pauses, never kills

- **What:** when the total budget cap is reached, maestro blocks new spawns and pauses running agents. Paused agents resume with `maestro resume`. maestro never kills agents because of the cap.
- **Why:** killing a running agent loses its work; pausing keeps it resumable.
- **Section:** `§02.14 Usage: cost, tokens and TurboQuant`, `§05 Poka-yoke catalog`.
- **Rejected:** killing running agents at the cap.
- **Source:** #1238 (poka-yoke D1).

## 2026-11-14 — D2: a confirmed quit pauses, never kills

- **What:** a confirmed quit pauses running agents and saves them as resumable with `maestro resume`. No `kill_all` on a normal exit; kill stays an explicit, typed action.
- **Why:** quitting the TUI should not destroy agent work.
- **Section:** `§05 Poka-yoke catalog`, `§02.24 Guards, release notes and small modals`.
- **Rejected:** `kill_all` on normal exit.
- **Source:** #1238 (poka-yoke D2).

## 2026-11-14 — D3: team waves advance on finish, not on spawn

- **What:** the next wave starts only when every issue of the previous wave has finished. Today's spawn-gated advance is a bug.
- **Why:** a wave depends on the work of the wave before it; spawning is not finishing.
- **Section:** `§02.17 Teams: Launch`, `§02.22 Run loop`.
- **Rejected:** advancing the wave once its agents have spawned (current behavior).
- **Source:** #1238 (poka-yoke D3).

## 2026-11-14 — #833 `new_ux` gates only Home Mission Control

- **What:** the `new_ux` flag (#833) is narrowed to gate only the new Home Mission Control, as a safe rollback during phase 2. #833 stays open.
- **Why:** one flag over one screen keeps the rollback simple.
- **Section:** `§02 Home: Mission Control`, `§08 Rollout & verification`.
- **Rejected:** a flag that gates the whole redesign.
- **Source:** #1238.

## 2026-11-14 — Milestone "TUI Redesign" for new work

- **What:** every new epic and leaf of the redesign goes in a new milestone, "TUI Redesign". "v0.32.0 — UX Spine" and the milestones of existing issues are not changed.
- **Why:** keeps the redesign trackable without rewriting history.
- **Section:** `§08 Rollout & verification`.
- **Rejected:** moving existing issues into the new milestone.
- **Source:** #1238.

## 2026-11-14 — `github.auto_merge` is a danger setting

- **What:** `github.auto_merge` is marked ▲ danger in Settings. Turning it on needs a typed confirmation.
- **Why:** auto-merge removes the human check between "CI green" and "merged".
- **Section:** `§02.15 Settings`, `§02.12 Review PRs`.
- **Rejected:** a plain toggle.
- **Source:** #1238.

## 2026-11-14 — Radial agent graph is removed

- **What:** the radial agent graph is removed: `ConcentricLayout`, its tests, and the palette entry "Agent graph (radial)" are deleted. The files lens is the only graph.
- **Why:** one graph view instead of two.
- **Section:** `§02.20 Agent graph`.
- **Rejected:** keeping the radial view next to the files lens.
- **Source:** #1238.

## 2026-11-14 — Cache lives in `.maestro/cache/`

- **What:** the TUI cache lives in `.maestro/cache/`, listed in `.gitignore`.
- **Why:** cached data must never be committed.
- **Section:** `§02.6 Roadmap`, `§09 Code findings & open questions`.
- **Rejected:** not recorded.
- **Source:** #1238.

## 2026-11-14 — Single-pass and verdict teams allowed from the TUI

- **What:** single-pass and verdict teams can be launched from the TUI, behind a typed confirmation that states roles are not routed yet.
- **Why:** the user can run them today, but must know role routing is not live.
- **Section:** `§02.16 Teams: entry, Compose, Manage`, `§02.17 Teams: Launch`.
- **Rejected:** hiding these team types until role routing ships.
- **Source:** #1238.

## 2026-11-14 — TurboQuant toggle lives in Settings

- **What:** the TurboQuant toggle lives in Settings › TurboQuant, not in the palette. ⌃Q is quit only.
- **Why:** one key, one meaning.
- **Section:** `§02.15 Settings`, `§06 Keymap`.
- **Rejected:** a palette entry or a ⌃Q binding for TurboQuant.
- **Source:** #1238.

## 2026-11-14 — No per-edit session hook; CI design-drift check enforces

- **What:** no per-edit PreToolUse hook (the logged rejection in `MEMORY.md` stands). Enforcement is `scripts/check-design-drift.sh` in CI, which blocks the merge instead of reminding.
- **Why:** a PreToolUse hook is Claude-only and noisy per edit; the CI check is provider-neutral and runs where merges happen.
- **Section:** this directory (see [README.md](README.md)).
- **Rejected:** a Claude-only PreToolUse hook. Revisit a SessionStart pointer hook only if two consecutive PRs ship design drift the CI script missed.
- **Source:** #1238.

## 2026-11-14 — §09 remainder: undo, `maestro:ready`, `due_on`

- **What:** ⌃Z undoes only reversible local actions announced by a toast. Milestone-wizard issues are NOT labelled `maestro:ready` by default (the gatekeeper decides). `due_on` comes from GitHub REST `due_on` (Azure `finishDate`).
- **Why:** answers the open questions left in §09.
- **Section:** `§09 Code findings & open questions`, `§02.3 Create milestone (wizard)`.
- **Rejected:** not recorded.
- **Source:** #1238.
