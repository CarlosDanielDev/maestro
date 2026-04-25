# Idea Triage Foundation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the upstream "idea inbox" funnel into maestro's orchestration loop — promote the drafted `subagent-idea-triager`, add a `/triage-idea` slash command, and ship the JSON-report parser hook that validates the triager's output contract.

**Architecture:** Mirrors the existing gatekeeper pattern — consultive subagent emits a fenced `json idea-triager` block, a Python hook validates and re-emits compact JSON, the orchestrator drives all GitHub side effects from the parsed report. No new runtime dependencies; pure `.claude/` infrastructure plus one new slash command.

**Tech Stack:** Markdown subagent specs, Python 3 stdlib (no new deps), `gh` CLI (read-only), existing `.github/ISSUE_TEMPLATE/idea.yml` (already drafted).

**Scope boundaries:** This plan is **Foundation only** — it ships the triager loop end-to-end for a single idea. The four downstream sub-projects (Park-lift, Spike pipeline, Spike→DOR handoff via `superpowers:writing-plans`, Auto-archive Action) are **out of scope** and will each get their own plan after Foundation merges. This plan must produce a working, demonstrable triager flow on its own.

**Pre-execution requirement:** This plan should be executed in a clean worktree. The current branch `maestro/unified-321-329-327-328` has unrelated in-flight work. Create a worktree before Step 1: `git worktree add ../maestro-idea-foundation -b idea/foundation main`.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `.claude/hooks/parse_idea_triager_report.py` | Create | Validate the `json idea-triager` fence, enforce `report_version: 1`, enforce enum fields, re-emit compact JSON on stdout. Exit 1 on any contract violation. |
| `.claude/hooks/fixtures/idea_triager_pass.txt` | Create | Golden-path fixture: a valid triager report wrapped in prose + fence. Used by parser tests + as docs example. |
| `.claude/hooks/fixtures/idea_triager_fail_missing_fence.txt` | Create | Negative fixture: prose only, no fence. |
| `.claude/hooks/fixtures/idea_triager_fail_bad_enum.txt` | Create | Negative fixture: valid JSON but `recommendation: "maybe"` (not in enum). |
| `drafts/agents/subagent-idea-triager.md` | Move (`git mv`) | Promote from drafts to `.claude/agents/subagent-idea-triager.md`. No content edits — already reviewed. |
| `.claude/commands/triage-idea.md` | Create | Slash command: parse `$ARGUMENTS` as issue number, fetch idea via `gh issue view`, dispatch `subagent-idea-triager`, pipe response through the parser, render digest. **Does not mutate GitHub state** in this plan — that's downstream. |
| `.claude/CLAUDE.md` | Modify | Two places: (a) Subagent Registry table — add row for `subagent-idea-triager`. (b) Delegation Rules table — add row mapping "Idea triage" → `subagent-idea-triager`. |
| `directory-tree.md` | Modify (via docs-analyst) | Reflect new files. Handled by `subagent-docs-analyst` per CLAUDE.md, NOT by hand. |

---

## Chunk 1: Parser Hook (TDD core)

The parser is the only piece with real logic. Everything else is config/markdown. So this chunk is the only one with proper RED→GREEN→REFACTOR cycles. The other chunks verify by inspection + smoke test.

### Task 1: Failing parser test (golden path)

**Files:**
- Create: `.claude/hooks/fixtures/idea_triager_pass.txt`
- Test: invoked via shell — no Python test framework exists in this Rust project, so we use fixture-driven smoke tests (same pattern as the gatekeeper parser, which has no Python tests today).

- [ ] **Step 1: Create the golden-path fixture**

Write `.claude/hooks/fixtures/idea_triager_pass.txt`:

````
Triager findings for idea #999:

Q1 names me as the user, daily during sprint pushes — passes.
Q2 proposes adding tracing spans only — passes.
Q3 has a measurable threshold — passes.
Q4 honestly says "nothing today, revisit at 20+ sessions" — passes.
Q5 picks "Adjacent" — passes.

```json idea-triager
{
  "report_version": 1,
  "recommendation": "promote",
  "checks": {
    "whose_problem": { "verdict": "pass", "note": "named user, daily frequency" },
    "smallest_proof": { "verdict": "pass", "note": "tracing spans, ≤1 day" },
    "success_signal": { "verdict": "pass", "note": "p95 latency threshold" },
    "cost_of_skipping": { "verdict": "pass", "note": "honest 'nothing today' with revisit" },
    "vision_alignment": { "verdict": "pass", "note": "Adjacent" }
  },
  "score": { "pass": 5, "weak": 0, "fail": 0 },
  "spike_proposal": {
    "scope": "add tracing spans around Command::spawn and first-event-received; log to .maestro/metrics/",
    "time_box_days": 1,
    "exit_criteria": [
      "spans visible in tracing output",
      "metrics file written for ≥1 session",
      "p50/p95 spawn latency reported"
    ]
  },
  "remediation": {
    "comment_body": "",
    "labels_to_add": ["spike", "ready"],
    "labels_to_remove": ["needs-triage"]
  },
  "reasons": [
    "all 5 checks passed",
    "spike scope fits within 1 day"
  ]
}
```
````

- [ ] **Step 2: Run the parser to verify it fails (RED)**

Run:
```bash
cat .claude/hooks/fixtures/idea_triager_pass.txt | python3 .claude/hooks/parse_idea_triager_report.py; echo "exit=$?"
```

Expected: `python3: can't open file '...parse_idea_triager_report.py': [Errno 2]`, `exit=2`. Confirms file does not yet exist.

### Task 2: Parser implementation (GREEN)

**Files:**
- Create: `.claude/hooks/parse_idea_triager_report.py`

- [ ] **Step 1: Create the parser**

Write `.claude/hooks/parse_idea_triager_report.py`:

```python
#!/usr/bin/env python3
"""Extract and validate idea-triager JSON reports from subagent responses.

Usage:
    python3 parse_idea_triager_report.py < input.txt
    echo "<text>" | python3 parse_idea_triager_report.py

Exit codes:
    0 — valid report extracted, re-emitted as compact JSON on stdout
    1 — parse error (no fence, malformed JSON, wrong version, schema violation)
"""
import json
import re
import sys

FENCE_PATTERN = re.compile(
    r"```json\s+idea-triager\s*\n(.*?)\n```",
    re.DOTALL,
)

SUPPORTED_VERSION = 1
RECOMMENDATIONS = {"promote", "park", "archive"}
VERDICTS = {"pass", "weak", "fail"}
CHECK_KEYS = (
    "whose_problem",
    "smallest_proof",
    "success_signal",
    "cost_of_skipping",
    "vision_alignment",
)


class ParseError(Exception):
    """Raised when the input cannot be parsed as a valid triager report."""


def extract_report(text: str) -> dict:
    match = FENCE_PATTERN.search(text)
    if not match:
        raise ParseError("no ```json idea-triager fenced block found in input")

    try:
        report = json.loads(match.group(1))
    except json.JSONDecodeError as exc:
        raise ParseError(f"malformed JSON in idea-triager fence: {exc}") from exc

    if not isinstance(report, dict):
        raise ParseError("idea-triager report must be a JSON object")

    version = report.get("report_version")
    if version != SUPPORTED_VERSION:
        raise ParseError(
            f"unsupported report_version: {version!r} "
            f"(this parser supports {SUPPORTED_VERSION})"
        )

    recommendation = report.get("recommendation")
    if recommendation not in RECOMMENDATIONS:
        raise ParseError(
            f"recommendation must be one of {sorted(RECOMMENDATIONS)}, "
            f"got {recommendation!r}"
        )

    checks = report.get("checks")
    if not isinstance(checks, dict):
        raise ParseError("checks must be a JSON object")
    for key in CHECK_KEYS:
        check = checks.get(key)
        if not isinstance(check, dict):
            raise ParseError(f"checks.{key} missing or not an object")
        verdict = check.get("verdict")
        if verdict not in VERDICTS:
            raise ParseError(
                f"checks.{key}.verdict must be one of {sorted(VERDICTS)}, "
                f"got {verdict!r}"
            )

    if recommendation == "promote" and "spike_proposal" not in report:
        raise ParseError("recommendation 'promote' requires spike_proposal")

    if recommendation in {"park", "archive"}:
        remediation = report.get("remediation") or {}
        if not remediation.get("comment_body"):
            raise ParseError(
                f"recommendation '{recommendation}' requires "
                "remediation.comment_body"
            )

    return report


def main() -> int:
    text = sys.stdin.read()
    try:
        report = extract_report(text)
    except ParseError as exc:
        print(f"parse-idea-triager-report: {exc}", file=sys.stderr)
        return 1

    json.dump(report, sys.stdout)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x .claude/hooks/parse_idea_triager_report.py
```

- [ ] **Step 3: Run the golden-path fixture and verify it passes (GREEN)**

```bash
cat .claude/hooks/fixtures/idea_triager_pass.txt | python3 .claude/hooks/parse_idea_triager_report.py; echo "exit=$?"
```

Expected: a single line of compact JSON on stdout (the report), then `exit=0`.

- [ ] **Step 4: Commit**

```bash
git add .claude/hooks/parse_idea_triager_report.py .claude/hooks/fixtures/idea_triager_pass.txt
git commit -m "feat(hooks): add parse_idea_triager_report hook with golden fixture"
```

### Task 3: Negative-path fixtures (RED→GREEN)

- [ ] **Step 1: Create missing-fence fixture**

Write `.claude/hooks/fixtures/idea_triager_fail_missing_fence.txt`:

```
The triager forgot to emit a fenced block.
This is plain prose with no JSON anywhere.
```

- [ ] **Step 2: Verify parser rejects it**

```bash
cat .claude/hooks/fixtures/idea_triager_fail_missing_fence.txt | python3 .claude/hooks/parse_idea_triager_report.py 2>&1; echo "exit=$?"
```

Expected: stderr line `parse-idea-triager-report: no \`\`\`json idea-triager fenced block found in input`, `exit=1`.

- [ ] **Step 3: Create bad-enum fixture**

Write `.claude/hooks/fixtures/idea_triager_fail_bad_enum.txt`:

````
```json idea-triager
{
  "report_version": 1,
  "recommendation": "maybe",
  "checks": {
    "whose_problem": { "verdict": "pass", "note": "" },
    "smallest_proof": { "verdict": "pass", "note": "" },
    "success_signal": { "verdict": "pass", "note": "" },
    "cost_of_skipping": { "verdict": "pass", "note": "" },
    "vision_alignment": { "verdict": "pass", "note": "" }
  },
  "score": { "pass": 5, "weak": 0, "fail": 0 },
  "remediation": { "comment_body": "", "labels_to_add": [], "labels_to_remove": [] },
  "reasons": []
}
```
````

- [ ] **Step 4: Verify parser rejects it**

```bash
cat .claude/hooks/fixtures/idea_triager_fail_bad_enum.txt | python3 .claude/hooks/parse_idea_triager_report.py 2>&1; echo "exit=$?"
```

Expected: stderr line `parse-idea-triager-report: recommendation must be one of ['archive', 'park', 'promote'], got 'maybe'`, `exit=1`.

- [ ] **Step 5: Commit**

```bash
git add .claude/hooks/fixtures/idea_triager_fail_missing_fence.txt .claude/hooks/fixtures/idea_triager_fail_bad_enum.txt
git commit -m "test(hooks): add negative fixtures for idea-triager parser"
```

---

## Chunk 2: Subagent Promotion + Slash Command

### Task 4: Promote subagent from drafts

**Files:**
- Move: `drafts/agents/subagent-idea-triager.md` → `.claude/agents/subagent-idea-triager.md`

- [ ] **Step 1: Move the file with `git mv` to preserve history**

```bash
git mv drafts/agents/subagent-idea-triager.md .claude/agents/subagent-idea-triager.md
```

- [ ] **Step 2: Verify the move**

```bash
ls .claude/agents/subagent-idea-triager.md && ls drafts/agents/ 2>/dev/null
```

Expected: file exists in `.claude/agents/`, `drafts/agents/` is empty (or does not list this file).

- [ ] **Step 3: Commit**

```bash
git commit -m "chore(agents): promote subagent-idea-triager from drafts"
```

### Task 5: Create the `/triage-idea` slash command

**Files:**
- Create: `.claude/commands/triage-idea.md`

- [ ] **Step 1: Create the slash command spec**

Write `.claude/commands/triage-idea.md`:

````markdown
# Triage Idea

Run the upstream idea-inbox triage loop on a GitHub issue created from
`.github/ISSUE_TEMPLATE/idea.yml`. Dispatches `subagent-idea-triager`,
validates the structured report, and prints a digest. **Does not mutate
GitHub state** — comment posting and label changes are the orchestrator's
responsibility, gated on user confirmation.

**Usage:** `/triage-idea #123` or `/triage-idea 123`

---

## Arguments

`$ARGUMENTS` contains the idea issue number (with optional `#` prefix).

## Instructions

### Step 0: Parse arguments

Extract the first `\d+` from `$ARGUMENTS`. Reject if no number is present.

```bash
export IDEA_NUMBER="<n>"
```

### Step 1: Confirm the issue exists and has the `idea` label

```bash
gh issue view "$IDEA_NUMBER" --json number,title,labels,state,body \
  --jq '{number, title, state, labels: [.labels[].name], body}'
```

If the issue lacks the `idea` label, print a warning and ask the user
whether to proceed anyway. Some users may file ideas without the
template; the triager can still score the body.

### Step 2: Dispatch `subagent-idea-triager`

Invoke the subagent with:
- The full issue JSON from Step 1.
- The repo name (`gh repo view --json nameWithOwner --jq .nameWithOwner`).

The subagent must return a fenced `json idea-triager` block per its
output contract (see `.claude/agents/subagent-idea-triager.md`).

### Step 3: Validate the report

Pipe the subagent's response through the parser hook:

```bash
echo "$SUBAGENT_RESPONSE" | python3 .claude/hooks/parse_idea_triager_report.py
```

If the parser exits non-zero:
- Print the parser's stderr verbatim.
- Stop. Do NOT post comments or change labels.
- Suggest the user re-run `/triage-idea` or inspect the subagent's raw output.

### Step 4: Render the digest to the user

From the parsed JSON, print:

```
Triage result: <recommendation>

Checks:
  whose_problem    : <verdict>  — <note>
  smallest_proof   : <verdict>  — <note>
  success_signal   : <verdict>  — <note>
  cost_of_skipping : <verdict>  — <note>
  vision_alignment : <verdict>  — <note>

Score: <pass> pass / <weak> weak / <fail> fail
```

If `recommendation == "promote"`, also print:

```
Spike proposal:
  Scope: <scope>
  Time-box: <time_box_days> day(s)
  Exit criteria:
    - <each criterion>
```

If `recommendation == "park"` or `archive`, also print:

```
Draft remediation comment:
---
<comment_body>
---

Suggested label changes:
  add:    <labels_to_add>
  remove: <labels_to_remove>
```

### Step 5: Ask the user how to proceed

**Do not mutate GitHub state without confirmation.** Ask:

- For `promote`: "Create a spike issue from this proposal?" (Spike-creation
  is in the next plan; for now, answer with "noted — Spike pipeline lands
  in plan 3.")
- For `park`: "Post the draft comment and apply the label changes?"
  - On yes: `gh issue comment $IDEA_NUMBER --body "$COMMENT"` and
    `gh issue edit $IDEA_NUMBER --add-label "<add>" --remove-label "<remove>"`.
  - On no: print "no changes made" and exit.
- For `archive`: same as `park` but the label set differs.

## Consultive boundaries

- This command never auto-mutates. Always confirm before posting comments
  or changing labels.
- This command never creates spike issues yet — that ships in plan 3.
- This command does not invoke `superpowers:brainstorming` for `park`
  recommendations yet — that ships in plan 2 (Park-lift).
````

- [ ] **Step 2: Verify the file is well-formed markdown**

```bash
head -20 .claude/commands/triage-idea.md
```

Expected: starts with `# Triage Idea`, includes `## Arguments` section.

- [ ] **Step 3: Commit**

```bash
git add .claude/commands/triage-idea.md
git commit -m "feat(commands): add /triage-idea slash command spec"
```

---

## Chunk 3: Registry Updates + Validation

### Task 6: Update CLAUDE.md registry tables

**Files:**
- Modify: `.claude/CLAUDE.md` — two table edits.

- [ ] **Step 1: Add to the Subagent Registry table**

Locate the table starting with `| Subagent | Purpose | Status |` and add a new row, in alphabetical order or at the end:

```markdown
| `subagent-idea-triager` | Idea-inbox triage gate (5-question honesty check, promote/park/archive) | **Ready** |
```

- [ ] **Step 2: Add to the Delegation Rules table**

Locate the table starting with `| Need | Delegate To |` and add a new row:

```markdown
| Idea triage (pre-DOR funnel) | `subagent-idea-triager` |
```

- [ ] **Step 3: Verify the edits**

```bash
grep -n "subagent-idea-triager" .claude/CLAUDE.md
```

Expected: at least 2 hits — one in each table.

- [ ] **Step 4: Commit**

```bash
git add .claude/CLAUDE.md
git commit -m "docs(claude-md): register subagent-idea-triager in delegation tables"
```

### Task 7: End-to-end smoke test

This is the only validation step that touches real GitHub. **Run against a throwaway test issue, not a real idea.**

- [ ] **Step 1: File a throwaway test idea**

```bash
gh issue create \
  --title "[Idea]: smoke test for /triage-idea" \
  --label "idea,needs-triage" \
  --body "$(cat <<'EOF'
### What's the itch?
Smoke testing the new triage command. Delete after verification.

### Whose problem is this, and how often do they hit it?
Me, right now, once. This is a smoke test.

### What's the smallest thing that would prove this works?
Running /triage-idea against this issue and getting a valid JSON report.

### What does success look like as a number or observation?
Parser exits 0; digest prints the 5 checks.

### What's the cost of NOT doing it?
Nothing — this is throwaway.

### Vision alignment
Sideways (nice-to-have, not on the critical path)
EOF
)"
```

Capture the issue number printed.

- [ ] **Step 2: Run the slash command against it**

In the Claude Code session, invoke `/triage-idea <NUMBER>`.

Expected: digest prints with all 5 checks. `recommendation` is likely `archive` or `park` (the answers are intentionally weak — this is a smoke test, not a real idea), but the *contract* must hold: parser exit 0, all required fields present.

- [ ] **Step 3: Close the test issue**

```bash
gh issue close <NUMBER> --comment "smoke test — closing"
```

- [ ] **Step 4: Verify no orphaned label changes**

```bash
gh issue view <NUMBER> --json labels,state
```

Expected: state `CLOSED`. Labels unchanged from creation (the slash command should NOT have mutated state without user confirmation, per its design).

### Task 8: Final docs sweep

- [ ] **Step 1: Invoke `subagent-docs-analyst`**

Per CLAUDE.md, this subagent is mandatory at task end. It:
- Updates `directory-tree.md` with the new files.
- Detects any duplicate or stale .md content.

Do NOT hand-edit `directory-tree.md` — that's the docs-analyst's job.

- [ ] **Step 2: Verify the tree update**

```bash
grep -E "(subagent-idea-triager|triage-idea|parse_idea_triager_report)" directory-tree.md
```

Expected: at least 3 hits.

- [ ] **Step 3: Commit any docs-analyst changes**

```bash
git add directory-tree.md
git commit -m "docs(directory-tree): reflect idea-triage foundation files"
```

---

## Out of Scope (Tracked for Follow-Up Plans)

These are intentionally **not** in this plan. Each becomes its own plan after Foundation merges:

| Plan | Scope |
|---|---|
| Plan 2 — Park-lift | When triager returns `park`, orchestrator offers to invoke `superpowers:brainstorming` to lift the idea, then re-triage. |
| Plan 3 — Spike pipeline | When triager returns `promote`, orchestrator creates a `spike` issue from `spike_proposal`. Defines the spike issue template. Adds `verification-before-completion` gate at spike conclusion. |
| Plan 4 — Spike→DOR handoff | When a spike returns "go", orchestrator invokes `superpowers:writing-plans` to shape a real DOR-compliant issue. |
| Plan 5 — Auto-archive | GitHub Action that closes idea issues inactive >28 days with `wontfix`. |
| Plan 6 (conditional) — Bug-flavored debug branch | When a spike is bug-flavored, route through `superpowers:systematic-debugging` during execution. Only ship if Plan 3 reveals this is a recurring need. |

## Definition of Done

- [ ] Parser hook exists, executable, all 3 fixtures behave per expectations.
- [ ] Subagent moved from `drafts/` to `.claude/agents/`, history preserved via `git mv`.
- [ ] `/triage-idea` slash command spec exists and references parser correctly.
- [ ] CLAUDE.md registry and delegation tables updated.
- [ ] End-to-end smoke test passed (test issue created, command run, parser exit 0, no unauthorized GitHub mutations).
- [ ] `directory-tree.md` reflects new files (via docs-analyst).
- [ ] All commits made; no uncommitted changes.

## Risks and Tradeoffs Acknowledged

1. **Parser is over-strict.** The contract requires `comment_body` for `park`/`archive` and `spike_proposal` for `promote`. If the subagent forgets, the parser fails loudly — this is intentional (loud failure beats silent drift), but expect at least one round of subagent prompt tuning.
2. **No automated unit tests for the parser.** The maestro repo is Rust-first; no Python test infra exists. We use fixture-driven smoke tests, which is the same convention `parse_gatekeeper_report.py` uses today. If this becomes a maintenance burden, add `pytest` to the dev dependency story in a later plan.
3. **Slash command depends on subagent prompt fidelity.** If the subagent ever drops the fence tag, the parser fails and the command halts. Mitigated by the consultive-only discipline section in the subagent spec, but worth monitoring.
4. **End-to-end smoke test creates a real GitHub issue.** Use a throwaway title and close it immediately. If the repo has automation that reacts to `idea` labels (it shouldn't yet), the smoke test could trigger noise — verify before running.
