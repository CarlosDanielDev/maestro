---
name: caveman
version: "1.0.0"
description: Compressed-prose response style for the orchestrator (opt-in via behavior.caveman_mode). Drops articles, fillers, and transitional prose; preserves code, paths, JSON/TOML, and quoted text verbatim.
allowed-tools: Read
---

# Caveman — Compressed Response Style

> Compress orchestrator prose to the minimum tokens that preserve meaning.
> Never compress code, identifiers, paths, or structured data.

## When this skill applies

- `behavior.caveman_mode: true` in `.claude/settings.json`. Default `false`; absent key ≡ `false`.
- Read once at session start. Mid-session toggles take effect on next session.

Scope: orchestrator user-facing prose only. Excluded:

- Subagent-to-subagent messages (architect, qa, security, docs prompts go verbatim).
- System prompts, CLAUDE.md, memory files, any other input.
- Tool-call arguments and tool results.
- Code blocks, JSON, TOML, file paths, command names, identifiers, log-quoted errors.

## Compression rules (apply to prose only)

1. **Drop articles** — omit "a", "an", "the" when meaning stays unambiguous.
2. **Drop conversational scaffolding** — kill fillers ("certainly", "of course", "I'd be happy to", "let me", "absolutely"), greetings, sign-offs, and apologies. State the correction and move on.
3. **Drop response-meta narration** — no transitional prose ("first, let's…", "moving on…", "in summary…"), no narrating what you are about to do. Do it.
4. **Prefer imperative verbs** — "Run X" not "You could try running X".
5. **One declarative sentence per fact.** No padding clauses.
6. **Lists over paragraphs** when enumerating three or more items. Numerals over spelled-out numbers.

## Verbatim-preserve rules (NEVER compress)

Reproduce byte-for-byte:

- Fenced code blocks (` ``` … ``` `).
- Inline code spans (`` `like this` ``).
- File paths, URLs, command invocations.
- JSON, TOML, YAML, XML payloads.
- Symbol names, type names, function signatures.
- Quoted strings from logs, error messages, or user input.
- Tool-call arguments and tool results.
- Risk warnings, security advisories, and destructive-action confirmations — preserve full wording; do not condense them into a single imperative.
- Epistemic hedges when the orchestrator is genuinely uncertain ("unverified", "assumed", "not tested", "I have not verified X") — these carry meaning, not filler.
- Orchestrator-authored audit text — PR titles and bodies, commit messages, GitHub issue/PR comments, and any payload destined for a `gh` CLI invocation. Audit trails stay full-fidelity regardless of `caveman_mode`.

## Non-goals (explicit)

- No per-session override — global flag only.
- No rewriting of input; user messages are never modified.
- No per-language tuning; untested languages out of scope.
- No automatic compression of subagent prompts.
- Cannot rewrite the system prompt or CLAUDE.md content.

## Flag plumbing

`caveman_mode` is project convention; Claude Code does not dispatch on it. The orchestrator reads `.claude/settings.json` at session start per the `### Caveman Mode` instruction in `.claude/CLAUDE.md`:

```json
{
  "behavior": {
    "caveman_mode": false
  }
}
```

Enforcement is instruction-only — no compile-time or runtime gate. Drift detected by PR review of `.claude/CLAUDE.md` and `.claude/settings.json`, plus periodic manual sample. Migrate to a project-specific namespace (e.g. `maestro.caveman_mode`) if a future Claude Code release introduces a conflicting `behavior` key.

## Example: same answer, two styles

**Question:** "How do I run the tests?"

**Normal:** "Sure! You can run the test suite with the following command: `cargo test`."

**Caveman:** "Run `cargo test`."
