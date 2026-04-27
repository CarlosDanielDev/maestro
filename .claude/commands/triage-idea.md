# Triage Idea

Run the upstream idea-inbox triage loop on a GitHub issue created from
`.github/ISSUE_TEMPLATE/idea.yml`. Dispatches `subagent-idea-triager`,
validates the structured report through `.claude/hooks/parse_idea_triager_report.py`,
and prints a digest. **Does not mutate GitHub state without explicit user
confirmation** — every comment posting and label change is gated on a
`yes/no` prompt. v0.16.1 is the foundation milestone; auto-mutation flows
ship later (see Consultive Boundaries below).

**Usage:** `/triage-idea #123` or `/triage-idea 123`

---

## Arguments

`$ARGUMENTS` contains the idea issue number with an optional `#` prefix.
Extract the first `\d+` and reject if no number is present.

---

## Instructions

### Step 0: Parse arguments

Extract the first run of digits from `$ARGUMENTS` and re-validate the
result is digits-only before exporting. Never pass `$ARGUMENTS` directly
to `gh`.

```bash
IDEA_NUMBER=$(printf '%s' "$ARGUMENTS" | grep -oE '[0-9]+' | head -n1)
[[ "$IDEA_NUMBER" =~ ^[0-9]+$ ]] || {
  echo "no issue number in arguments" >&2
  exit 1
}
export IDEA_NUMBER
```

If no number is found, ask: "Which idea issue should I triage?" and stop.

### Step 1: Confirm the issue exists and surface its labels

```bash
gh issue view "$IDEA_NUMBER" \
  --json number,title,labels,state,body \
  --jq '{number, title, state, labels: [.labels[].name], body}'
```

- If `gh` is not installed or not authenticated → exit 1 with the standard
  hint (`brew install gh` / `gh auth login`).
- If the issue does not exist → exit 1.
- If the issue is `CLOSED` → exit 1. Re-open or pick a different issue.
- If the labels do **not** include `idea`, print a warning and ask the
  user whether to proceed anyway. Some users may file ideas without the
  template; the triager can still score the body.

### Step 2: Dispatch `subagent-idea-triager`

Invoke `subagent-idea-triager` via the `Agent` tool. Pass:

- The full issue JSON from Step 1 (including title and body).
- The repo name: `gh repo view --json nameWithOwner --jq .nameWithOwner`.

The subagent's spec lives at `.claude/agents/subagent-idea-triager.md`.
It must return a fenced ```` ```json idea-triager ```` block per its
output contract.

### Step 3: Validate the report

Pipe the subagent's response through the parser hook into a single
`PARSED_JSON` buffer — Step 4 and Step 5 both read from this buffer with
no re-parsing in between.

```bash
PARSED_JSON=$(echo "$SUBAGENT_RESPONSE" | python3 .claude/hooks/parse_idea_triager_report.py)
```

If the parser exits non-zero:
- Print the parser's stderr verbatim.
- Stop. Do **NOT** post comments or change labels.
- Suggest the user re-run `/triage-idea` or inspect the subagent's raw output.
- Exit 2.

### Step 4: Render the digest to the user

Materialize the buffers Step 5 will need from `$PARSED_JSON` once, here.
Step 5 must use these exact variables — not re-parse the JSON. Derive
the pass/weak/fail counts from the `checks` block (do **not** trust
`score.*` blindly — derive at render time).

```bash
RECOMMENDATION=$(jq -r '.recommendation' <<<"$PARSED_JSON")
COMMENT_BODY=$(jq -r '.remediation.comment_body // ""' <<<"$PARSED_JSON")
mapfile -t LABELS_TO_ADD    < <(jq -r '.remediation.labels_to_add[]?    // empty' <<<"$PARSED_JSON")
mapfile -t LABELS_TO_REMOVE < <(jq -r '.remediation.labels_to_remove[]? // empty' <<<"$PARSED_JSON")
```

**Sanitize before printing.** Strip ASCII control characters
(`\x00-\x08\x0b-\x1f\x7f`) from every `note`, `comment_body`, and
`scope` string before they hit the user's terminal. The y/n prompt in
Step 5 must show the user the same bytes that will land on GitHub —
hidden escape sequences would let a hostile subagent response deceive
the operator.

Then print:

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

Suggested labels (NOT applied — Plan 3 territory):
  add:    <labels_to_add>
  remove: <labels_to_remove>
```

If `recommendation == "park"` or `recommendation == "archive"`, also print:

```
Draft remediation comment:
---
<comment_body>
---

Suggested label changes:
  add:    <labels_to_add>
  remove: <labels_to_remove>
```

### Step 5: Confirm before any GitHub mutation

**This command never auto-mutates GitHub state.** Always prompt; act only
on an explicit `yes`.

- For `promote`:
  > "Spike-issue creation lands in Plan 3 (Spike pipeline). For now, no
  > GitHub mutation is performed. Approve? (y/n)"
  - `y` — print `noted — Spike pipeline lands in Plan 3` and exit 0.
  - `n` — print `no changes made` and exit 3.

- For `park`:
  > "Post the draft comment and apply the label changes? (y/n)"
  - `y` — execute against the **same buffers materialized in Step 4**
    (no re-parsing of the JSON between confirm and act). Marshal the
    JSON arrays into repeated CLI flags rather than passing them as
    single comma-joined strings:
    ```bash
    gh issue comment "$IDEA_NUMBER" --body "$COMMENT_BODY"

    edit_args=()
    for lbl in "${LABELS_TO_ADD[@]}";    do edit_args+=(--add-label    "$lbl"); done
    for lbl in "${LABELS_TO_REMOVE[@]}"; do edit_args+=(--remove-label "$lbl"); done
    gh issue edit "$IDEA_NUMBER" "${edit_args[@]}"
    ```
    Then exit 0. The user-confirmation loop also covers brainstorming for
    `park` outcomes — that lands in Plan 2 (Park-lift).
  - `n` — print `no changes made` and exit 3.

- For `archive`: same prompt and flow as `park`, but the label set
  reflects archival. Auto-archive of stale ideas (>28 days) lands in
  Plan 5; this command never closes the issue, even on `archive`.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success — digest printed; mutations either declined or completed after explicit user confirmation. |
| 1 | Generic failure (`gh` missing or unauthenticated, issue not found, issue closed). |
| 2 | Parser contract violation — subagent response did not validate. No mutation performed. |
| 3 | User declined the post-digest confirmation; no mutation performed. Informational, not an error. |

---

## Error Handling

- `gh` not installed → exit 1 with the install hint.
- `gh` not authenticated → exit 1 with `gh auth login` hint.
- Issue does not exist or is closed → exit 1.
- Subagent fails to return a fenced ```` ```json idea-triager ```` block →
  the parser produces the canonical "no fence" error; orchestrator exits 2.
- Parser rejects the report (bad enum, missing required field) → print
  parser stderr verbatim, exit 2. Never act on raw subagent output.
- User answers `n` at the Step 5 prompt → exit 3 cleanly with
  `no changes made`.

---

## Consultive Boundaries (v0.16.1 scope)

- This command **never auto-mutates** GitHub state. Comments and label
  changes always require an explicit `yes` at the Step 5 prompt.
- Spike-issue creation from `recommendation == "promote"` is deferred to
  **Plan 3** (Spike pipeline). The `labels_to_add` for `promote` are
  surfaced in the digest but **not applied** by this command.
- `superpowers:brainstorming` for `park` results is deferred to
  **Plan 2** (Park-lift).
- `superpowers:writing-plans` handoff for promoted spikes is deferred to
  **Plan 4** (Spike→DOR handoff).
- Auto-archive of stale ideas (>28 days) is deferred to
  **Plan 5** (Auto-archive Action).

---

## Do Not

- Run mutating `gh` commands (`gh issue comment`, `gh issue edit`,
  `gh issue close`) without an explicit `yes` from the Step 5 prompt.
- Act on the subagent's raw response — always pipe through
  `.claude/hooks/parse_idea_triager_report.py` first.
- Create spike issues from this command (Plan 3 territory).
- Invoke `superpowers:brainstorming` from this command (Plan 2 territory).
- Close the idea issue from this command, even on `archive`
  (Plan 5 territory).
- Apply the `promote` recommendation's `labels_to_add` — they are
  surfaced for the user but applied by Plan 3, not here.
- Pass `$ARGUMENTS` directly into any `gh` invocation; route through the
  validated `$IDEA_NUMBER` from Step 0.
- Pass JSON-array label fields as a single comma-joined string to
  `gh issue edit`; always expand into repeated `--add-label` /
  `--remove-label` flags as shown in Step 5.
