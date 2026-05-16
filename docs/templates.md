# Canonical Templates

`.maestro/templates/` is the **canonical, agent-agnostic source** for maestro
slash commands. Provider-specific rendered artifacts (e.g. `.claude/commands/`)
are generated from this tree by `maestro sync-templates` and **must not be
edited directly** — they carry an `AUTO-GENERATED` banner and CI rejects PRs
that drift.

This guide covers the placeholder vocabulary, how to add new commands and
providers, drift-detection behavior in CI, per-provider quirks, and common
troubleshooting paths.

---

## Why this layer exists

Before the templates layer (issues #700-#706), every slash-command file had
to be authored separately per provider directory (`.claude/commands/*.md`).
That meant:

- Premises like the TDD cycle and DOR rules had to be duplicated and stayed
  in sync by convention.
- New providers (Codex, HTTP-generic) needed every command re-authored.
- Edits to a single rule (e.g., "skip RED gate for docs tasks") had to be
  applied N times — and silently drifted.

The canonical layer fixes all three:

- **Single-source authoring** — write the spec once under
  `.maestro/templates/`. The render engine emits per-provider artifacts.
- **Agent-agnostic vocabulary** — `{{INVOKE_SUBAGENT}}` becomes a `Task`
  call for Claude, an inline expansion for Codex, or a runtime injection
  for an HTTP provider.
- **Drift-detected** — `maestro sync-templates --check` fails CI when a
  rendered artifact diverges from what the canonical spec would produce.

For the layout of `.maestro/templates/`, refer to `directory-tree.md` at
the repo root — never duplicate the tree here.

---

## Placeholder vocabulary

The renderer's placeholder vocabulary is hard-coded. The
`.maestro/templates/manifest.toml` file declares per-placeholder validation
hints used by tooling but does **not** define new placeholders.

| Placeholder         | Purpose                                                   | Required args              |
| ------------------- | --------------------------------------------------------- | -------------------------- |
| `{{INVOKE_SUBAGENT}}` | Render the canonical instruction for spawning a subagent. | `name`, `prompt`           |
| `{{HOOK_GATE}}`     | Render a bash invocation for a pre-/post-step gate hook.  | `script`, `args`           |
| `{{INCLUDE}}`       | Inline a shared core fragment from the templates root.    | `path` (max depth 8)       |
| `{{SUBAGENT_LIST}}` | Render the active subagent registry for the target provider. | _(none)_                |
| `{{SKILL}}`         | Reference a skill knowledge base.                         | `name`                     |

The canonical declarations live in
`.maestro/templates/manifest.toml`. Argument enforcement is implemented in
`src/templates/manifest.rs` and the renderer in
`src/commands/sync_templates/`.

---

## How to add a new command

1. Create the canonical file under `.maestro/templates/commands/<name>.md`.
   Front-matter is plain Markdown; placeholders use `{{...}}` syntax.
2. Register the command in the renderer's command registry (see
   `src/commands/sync_templates/`) so it is picked up by the sync flow.
3. Add a byte-identical regression assertion in `tests/templates_render.rs`
   so any unintentional change to the rendered output is caught.
4. Run `maestro sync-templates` to regenerate the provider artifacts; commit
   both the canonical source and the rendered outputs in the same PR.
5. CI runs `maestro sync-templates --check` and `cargo test`; both must pass.

---

## How to add a new provider

1. Implement the provider's rendering rules in
   `src/templates/provider_rules/<provider>.rs`.
2. Add a `[providers.<name>]` block to `.maestro/templates/manifest.toml`
   with `display_name`, `target_dir`, and `inline_skills`.
3. Wire the provider into the renderer registry so `sync-templates` invokes
   it for every command.
4. Choose the right strategy for your provider:
   - **Byte-identical** — emit the same text verbatim into a provider
     directory (the Claude Code pattern).
   - **Inline-expansion** — replace `{{SKILL}}` / `{{INCLUDE}}` with the
     expanded text (the Codex CLI pattern; no separate skill dir at the
     destination).
   - **Runtime-only** — never write to disk; inject the canonical text into
     the L2 prompt at build time (the HTTP-generic pattern).
5. Add per-provider tests under `tests/templates_render.rs` confirming the
   rendered output for at least one command.

---

## Drift detection in CI

CI defends three layers:

- **`maestro sync-templates --check`** — re-renders every command into a
  temp directory and diffs against the committed provider artifacts. Any
  drift fails the run. Re-run `maestro sync-templates` locally and commit
  the regenerated files.
- **`tests/templates_render.rs`** — byte-identical regression checks against
  hand-frozen snapshots; catches subtle whitespace or banner changes.
- **`tests/template_mirror_drift.rs`** (NEW in #708) — guards the
  `template/.maestro/templates/` scaffolding mirror. The `maestro init`
  scaffolder embeds the mirror via `include_bytes!`; the test asserts the
  mirror is byte-equal to the canonical `.maestro/templates/`. Fix with
  `cp -a .maestro/templates/. template/.maestro/templates/`.

---

## Per-provider quirks

- **Claude Code** — Byte-identical pass-through. Rendered files live under
  `.claude/commands/<name>.md` and carry an
  `AUTO-GENERATED by maestro sync-templates` banner.
- **Codex CLI** — Inline expansion of `{{SKILL}}` and `{{INCLUDE}}` at
  render time. No skill directory at the destination, so users don't need
  to install a parallel skills tree.
- **HTTP generic** — Runtime-only injection at L2 prompt build (no
  on-disk artifacts). The renderer prepares the canonical text; the L2
  orchestrator stitches it into the system prompt for each role.

---

## `maestro init` scaffolding (since #708)

`maestro init` mirrors `.maestro/templates/` into newly initialized
projects so the scaffolded project ships with a working canonical tree
out of the box.

- Embedded source: `template/.maestro/templates/` (kept byte-equal to
  `.maestro/templates/` by `tests/template_mirror_drift.rs`).
- Embedded via `include_bytes!` at build time — no runtime path lookup,
  so installed binaries via `cargo install` work in any project.
- Idempotent: re-running `maestro init` reports per-file `Created` /
  `Skipped` and never overwrites user-edited templates.

---

## Troubleshooting

- **"Unresolved placeholder `{{X}}`"** — the canonical file references a
  placeholder the renderer does not know. Either you typed it wrong or
  you need to add it to the vocabulary (see `src/templates/manifest.rs`).
- **"Drift detected by CI"** — re-run `maestro sync-templates` locally,
  commit the regenerated files. Do NOT edit the rendered files manually.
- **"Byte-identical mismatch"** — usually trailing whitespace or a stale
  `AUTO-GENERATED` banner; regenerate via `maestro sync-templates`.
- **"`maestro init` did not drop `.maestro/templates/`"** — verify you
  are on a binary that includes #708 (`maestro --version` ≥ 0.27). If
  the directory already existed, every file is reported as `Skipped` —
  intended idempotency.
- **"Mirror tree drift between `.maestro/templates/` and
  `template/.maestro/templates/`"** — fix with
  `cp -a .maestro/templates/. template/.maestro/templates/` and commit.
  The drift test (`tests/template_mirror_drift.rs`) will pass once both
  trees are byte-equal.
