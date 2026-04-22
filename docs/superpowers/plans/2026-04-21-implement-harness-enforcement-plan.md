# /implement Harness Enforcement Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden the `/implement` slash command by replacing prose-only instructions with three enforceable gates — a pre-check shell hook, a new consultive `subagent-gatekeeper`, and a rewritten command body with inline RED/GREEN checkpoints.

**Architecture:** Three cooperating pieces. The pre-check hook does cheap mechanical checks (`gh` auth, dirty tree, baseline-green `cargo test`, preflight bridge). The gatekeeper subagent does semantic checks (DOR sections, `Blocked By` resolution, API contracts) and returns a JSON report in a fenced code block. The command's prose is rewritten as literal bash gates plus subagent invocations with structured output parsing.

**Tech Stack:** Rust (project code, `cargo test`), bash (hooks + inline gates, `bats` for tests), Python 3 stdlib (gatekeeper report parser, `unittest`), markdown (subagent + command prose), GitHub CLI (`gh issue view`, `gh api`).

**Spec reference:** `docs/superpowers/specs/2026-04-21-implement-harness-enforcement-design.md` (committed `442f8c2`, amended `c9b62cd`).

---

## File Structure

### New files

| Path | Responsibility |
|------|----------------|
| `.claude/hooks/parse_gatekeeper_report.py` | Extract and validate gatekeeper JSON report from subagent response. Stdlib-only. |
| `.claude/hooks/implement-gates.sh` | Pre-check hook. Mechanical checks + issue-JSON cache + preflight bridge. |
| `.claude/agents/subagent-gatekeeper.md` | Consultive subagent. DOR/blockers/contracts/task-type classification. Emits fenced JSON report. |
| `tests/hooks/parse_gatekeeper_report_test.py` | Python unittests for the parser. |
| `tests/hooks/implement-gates.bats` | Bats tests for the pre-check hook. Uses PATH-shim fixtures. |
| `tests/hooks/fixtures/fake-gh.sh` | PATH-shim `gh` CLI for bats tests. |
| `tests/hooks/fixtures/fake-cargo.sh` | PATH-shim `cargo` for bats tests. |
| `tests/gatekeeper/fixtures/*.json` | 10 issue JSON fixtures spanning the DOR permutation space. |
| `tests/gatekeeper/run-conformance.sh` | Runner that invokes `subagent-gatekeeper` against each fixture. |
| `docs/harness-acceptance.md` | Manual E2E acceptance checklist. |

### Modified files

| Path | Change |
|------|--------|
| `.claude/commands/implement.md` | Full rewrite — inline bash gates, structured subagent invocations, idempotency prompt. |
| `.claude/CLAUDE.md` | Add `subagent-gatekeeper` to the subagent registry table. |

### Deviations from spec (noted during planning)

1. **Parser filename uses underscores** (`parse_gatekeeper_report.py`) instead of hyphens (`parse-gatekeeper-report.py` per spec). Reason: Python module import convention — hyphens break `import`. Test files can load it with a direct `import parse_gatekeeper_report` instead of `importlib.util` gymnastics. Minor implementation detail, not worth a spec amendment.

2. **Gatekeeper output format switched from YAML to JSON** during design review (spec committed `c9b62cd`). Reason: Python stdlib has no YAML parser; `pyyaml` would break the "stdlib only" promise. JSON is stdlib-native.

### Pre-execution note

The `tests/` directory does not exist at the start of this plan. Task 1.1 creates `tests/hooks/` as a new top-level directory. This is intentional — Rust's `tests/*.rs` convention only triggers when `.rs` files are present; our `.bats` + `.py` files won't conflict with `cargo test`. No changes to `Cargo.toml` are needed.

### Not modified (out of scope — confirmed during design)

- `.claude/commands/pushup.md` — already owns milestone-graph updates.
- `.claude/hooks/preflight.sh` — owned by the CI-gates spec (separate project).
- `.claude/hooks/notify.sh` — unrelated (desktop notifications).

---

## Chunk Boundaries and Dependencies

```
Chunk 1: Parser Utility        (independent, foundation)
    ↓
Chunk 2: Gatekeeper Subagent   (depends on Chunk 1 — uses parser)
    ↓
Chunk 3: Pre-check Hook        (independent of 1 + 2 logically, but merged sequentially per PR-isolation)
    ↓
Chunk 4: Command Rewrite       (depends on 1 + 2 + 3)
    ↓
Chunk 5: Acceptance Checklist  (depends on 4)
```

Each chunk = one PR. Merge before starting the next (per user's PR-isolation rule).

---

## Chunk 1: Parser Utility

**Goal:** Create `parse_gatekeeper_report.py`, a stdlib-only Python script that extracts the fenced `json gatekeeper` block from a subagent response and validates `report_version: 1`. Build it TDD with `unittest`.

**Files:**
- Create: `.claude/hooks/parse_gatekeeper_report.py`
- Create: `tests/hooks/parse_gatekeeper_report_test.py`
- Create: `tests/hooks/__init__.py` (empty, enables package discovery)

### Task 1.1: Scaffold test directory and harness

**Files:**
- Create: `tests/hooks/__init__.py`
- Create: `tests/hooks/parse_gatekeeper_report_test.py`

- [ ] **Step 1: Create empty package marker**

```bash
mkdir -p tests/hooks
touch tests/hooks/__init__.py
```

- [ ] **Step 2: Write test harness skeleton**

File: `tests/hooks/parse_gatekeeper_report_test.py`

```python
"""Unit tests for .claude/hooks/parse_gatekeeper_report.py."""
import importlib.util
import unittest
from pathlib import Path

HOOK_PATH = Path(__file__).resolve().parents[2] / ".claude" / "hooks" / "parse_gatekeeper_report.py"

def _load_parser():
    spec = importlib.util.spec_from_file_location("parse_gatekeeper_report", HOOK_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

class ParserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.parser = _load_parser()

if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 3: Verify the test file runs (and errors because the parser doesn't exist yet)**

Run: `python3 tests/hooks/parse_gatekeeper_report_test.py`
Expected: `FileNotFoundError` or similar — the parser file doesn't exist yet. This confirms the harness correctly reaches for the parser. No commit yet.

---

### Task 1.2: TDD — Parse a valid PASS report

**Files:**
- Modify: `tests/hooks/parse_gatekeeper_report_test.py`
- Create: `.claude/hooks/parse_gatekeeper_report.py`

- [ ] **Step 1: Write the failing test**

Add to `tests/hooks/parse_gatekeeper_report_test.py` inside the `ParserTests` class:

```python
    def test_parses_valid_pass_report(self):
        text = """
Prose above the fence.

```json gatekeeper
{
  "report_version": 1,
  "status": "PASS",
  "task_type": "implementation",
  "dor": {"passed": true, "missing_sections": [], "weak_sections": []},
  "blockers": {"passed": true, "open": []},
  "contracts": {"passed": true, "missing": []},
  "remediation": {"comment_body": "", "labels_to_add": []},
  "reasons": []
}
```

Prose below the fence.
"""
        report = self.parser.extract_report(text)
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["task_type"], "implementation")
        self.assertTrue(report["dor"]["passed"])
```

- [ ] **Step 2: Run to confirm it fails**

Run: `python3 -m unittest tests.hooks.parse_gatekeeper_report_test.ParserTests.test_parses_valid_pass_report`
Expected: Error loading the module (`FileNotFoundError`) — parser file missing.

- [ ] **Step 3: Create the minimal parser**

File: `.claude/hooks/parse_gatekeeper_report.py`

```python
#!/usr/bin/env python3
"""Extract and validate gatekeeper JSON reports from subagent responses.

Usage:
    python3 parse_gatekeeper_report.py < input.txt
    echo "<text>" | python3 parse_gatekeeper_report.py

Exit codes:
    0 — valid report extracted, re-emitted as compact JSON on stdout
    1 — parse error (no fence, malformed JSON, wrong version)
"""
import json
import re
import sys

FENCE_PATTERN = re.compile(
    r"```json\s+gatekeeper\s*\n(.*?)\n```",
    re.DOTALL,
)

SUPPORTED_VERSION = 1


class ParseError(Exception):
    """Raised when the input cannot be parsed as a valid gatekeeper report."""


def extract_report(text: str) -> dict:
    """Extract the first ```json gatekeeper fenced block and parse as JSON."""
    match = FENCE_PATTERN.search(text)
    if not match:
        raise ParseError("no ```json gatekeeper fenced block found in input")

    content = match.group(1)
    try:
        report = json.loads(content)
    except json.JSONDecodeError as exc:
        raise ParseError(f"malformed JSON in gatekeeper fence: {exc}") from exc

    if not isinstance(report, dict):
        raise ParseError("gatekeeper report must be a JSON object")

    version = report.get("report_version")
    if version is None:
        raise ParseError("gatekeeper report missing required field: report_version")
    if version != SUPPORTED_VERSION:
        raise ParseError(
            f"unsupported report_version: {version} "
            f"(this parser supports {SUPPORTED_VERSION})"
        )

    return report


def main() -> int:
    text = sys.stdin.read()
    try:
        report = extract_report(text)
    except ParseError as exc:
        print(f"parse-gatekeeper-report: {exc}", file=sys.stderr)
        return 1

    json.dump(report, sys.stdout)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: Make it executable**

```bash
chmod +x .claude/hooks/parse_gatekeeper_report.py
```

- [ ] **Step 5: Run the test — expect PASS**

Run: `python3 -m unittest tests.hooks.parse_gatekeeper_report_test.ParserTests.test_parses_valid_pass_report`
Expected output: `OK` (one test, zero failures).

- [ ] **Step 6: Commit**

```bash
git add .claude/hooks/parse_gatekeeper_report.py tests/hooks/
git commit -m "$(cat <<'EOF'
feat(hooks): add gatekeeper JSON report parser

Stdlib-only Python script that extracts the first ```json gatekeeper
fenced block from a subagent response, validates report_version, and
re-emits the parsed object as compact JSON on stdout.

First passing test: valid PASS report round-trips through the parser.
Subsequent tasks add error-case coverage (malformed fence, wrong
version, missing fields, multiple fences).
EOF
)"
```

---

### Task 1.3: TDD — Parse a valid FAIL report with populated blockers

**Files:**
- Modify: `tests/hooks/parse_gatekeeper_report_test.py`

- [ ] **Step 1: Write the failing test**

Add to `ParserTests`:

```python
    def test_parses_valid_fail_report_with_blockers(self):
        text = """
```json gatekeeper
{
  "report_version": 1,
  "status": "FAIL",
  "task_type": "implementation",
  "dor": {"passed": true, "missing_sections": [], "weak_sections": []},
  "blockers": {
    "passed": false,
    "open": [
      {"number": 42, "title": "upstream scaffolding", "state": "OPEN"},
      {"number": 43, "title": "prerequisite api", "state": "OPEN"}
    ]
  },
  "contracts": {"passed": true, "missing": []},
  "remediation": {"comment_body": "", "labels_to_add": []},
  "reasons": ["Blocker #42 is OPEN", "Blocker #43 is OPEN"]
}
```
"""
        report = self.parser.extract_report(text)
        self.assertEqual(report["status"], "FAIL")
        self.assertFalse(report["blockers"]["passed"])
        self.assertEqual(len(report["blockers"]["open"]), 2)
        self.assertEqual(report["blockers"]["open"][0]["number"], 42)
```

- [ ] **Step 2: Run — expect PASS (parser already generic)**

Run: `python3 -m unittest tests.hooks.parse_gatekeeper_report_test.ParserTests.test_parses_valid_fail_report_with_blockers`
Expected: `OK`. The first task's parser handled this case; this test locks in the contract.

- [ ] **Step 3: Commit**

```bash
git add tests/hooks/parse_gatekeeper_report_test.py
git commit -m "test(hooks): lock parser contract for FAIL report with blockers"
```

---

### Task 1.4: TDD — Reject malformed fence (opening without closing)

**Files:**
- Modify: `tests/hooks/parse_gatekeeper_report_test.py`
- No parser changes expected (re.DOTALL search already returns None for unclosed fences)

- [ ] **Step 1: Write the failing test**

Add:

```python
    def test_rejects_unclosed_fence(self):
        text = "Prose.\n```json gatekeeper\n{\"report_version\": 1}\n"  # no closing ```
        with self.assertRaises(self.parser.ParseError) as ctx:
            self.parser.extract_report(text)
        self.assertIn("no ```json gatekeeper fenced block found", str(ctx.exception))
```

- [ ] **Step 2: Run — expect PASS**

Run: `python3 -m unittest tests.hooks.parse_gatekeeper_report_test.ParserTests.test_rejects_unclosed_fence`
Expected: `OK`. The regex `(.*?)\n\`\`\`` requires a closing fence to match; unclosed input returns `None`, which the parser converts to `ParseError`.

- [ ] **Step 3: Commit**

```bash
git add tests/hooks/parse_gatekeeper_report_test.py
git commit -m "test(hooks): reject gatekeeper report with unclosed fence"
```

---

### Task 1.5: TDD — Reject malformed JSON inside a closed fence

**Files:**
- Modify: `tests/hooks/parse_gatekeeper_report_test.py`

- [ ] **Step 1: Write the failing test**

Add:

```python
    def test_rejects_malformed_json(self):
        text = """
```json gatekeeper
{"report_version": 1, "status": "PASS", "trailing": }
```
"""
        with self.assertRaises(self.parser.ParseError) as ctx:
            self.parser.extract_report(text)
        self.assertIn("malformed JSON", str(ctx.exception))
```

- [ ] **Step 2: Run — expect PASS**

Run: `python3 -m unittest tests.hooks.parse_gatekeeper_report_test.ParserTests.test_rejects_malformed_json`
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add tests/hooks/parse_gatekeeper_report_test.py
git commit -m "test(hooks): reject gatekeeper report with malformed JSON"
```

---

### Task 1.6: TDD — Use the first fence when multiple are present

**Files:**
- Modify: `tests/hooks/parse_gatekeeper_report_test.py`

- [ ] **Step 1: Write the failing test**

Add:

```python
    def test_uses_first_fence_when_multiple(self):
        text = """
```json gatekeeper
{"report_version": 1, "status": "PASS", "task_type": "implementation",
 "dor": {"passed": true}, "blockers": {"passed": true},
 "contracts": {"passed": true}, "remediation": {}, "reasons": []}
```

Some prose.

```json gatekeeper
{"report_version": 1, "status": "FAIL", "task_type": "docs"}
```
"""
        report = self.parser.extract_report(text)
        self.assertEqual(report["status"], "PASS")  # first fence wins
```

- [ ] **Step 2: Run — expect PASS**

Run: `python3 -m unittest tests.hooks.parse_gatekeeper_report_test.ParserTests.test_uses_first_fence_when_multiple`
Expected: `OK`. `re.search` returns the first match by default.

- [ ] **Step 3: Commit**

```bash
git add tests/hooks/parse_gatekeeper_report_test.py
git commit -m "test(hooks): lock first-fence-wins semantics for multi-fence input"
```

---

### Task 1.7: TDD — Reject missing `report_version`

**Files:**
- Modify: `tests/hooks/parse_gatekeeper_report_test.py`

- [ ] **Step 1: Write the failing test**

Add:

```python
    def test_rejects_missing_report_version(self):
        text = """
```json gatekeeper
{"status": "PASS", "task_type": "implementation"}
```
"""
        with self.assertRaises(self.parser.ParseError) as ctx:
            self.parser.extract_report(text)
        self.assertIn("missing required field: report_version", str(ctx.exception))
```

- [ ] **Step 2: Run — expect PASS**

Run: `python3 -m unittest tests.hooks.parse_gatekeeper_report_test.ParserTests.test_rejects_missing_report_version`
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add tests/hooks/parse_gatekeeper_report_test.py
git commit -m "test(hooks): require report_version in gatekeeper report"
```

---

### Task 1.8: TDD — Reject unsupported `report_version`

**Files:**
- Modify: `tests/hooks/parse_gatekeeper_report_test.py`

- [ ] **Step 1: Write the failing test**

Add:

```python
    def test_rejects_future_report_version(self):
        text = """
```json gatekeeper
{"report_version": 2, "status": "PASS"}
```
"""
        with self.assertRaises(self.parser.ParseError) as ctx:
            self.parser.extract_report(text)
        self.assertIn("unsupported report_version: 2", str(ctx.exception))
```

- [ ] **Step 2: Run — expect PASS**

Run: `python3 -m unittest tests.hooks.parse_gatekeeper_report_test.ParserTests.test_rejects_future_report_version`
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add tests/hooks/parse_gatekeeper_report_test.py
git commit -m "test(hooks): reject unsupported report_version values"
```

---

### Task 1.9: TDD — CLI entrypoint via stdin/stdout

**Files:**
- Modify: `tests/hooks/parse_gatekeeper_report_test.py`

- [ ] **Step 1: Write the failing test**

Add a new test class (the CLI tests use subprocess, separate them for clarity):

```python
import subprocess
import sys

class CliTests(unittest.TestCase):
    def _run(self, stdin_text):
        return subprocess.run(
            [sys.executable, str(HOOK_PATH)],
            input=stdin_text,
            capture_output=True,
            text=True,
        )

    def test_cli_roundtrips_valid_report(self):
        stdin_text = """
```json gatekeeper
{"report_version": 1, "status": "PASS", "task_type": "implementation",
 "dor": {"passed": true}, "blockers": {"passed": true},
 "contracts": {"passed": true}, "remediation": {}, "reasons": []}
```
"""
        result = self._run(stdin_text)
        self.assertEqual(result.returncode, 0)
        import json as _json
        parsed = _json.loads(result.stdout)
        self.assertEqual(parsed["status"], "PASS")

    def test_cli_reports_error_on_malformed_input(self):
        result = self._run("no fence here")
        self.assertEqual(result.returncode, 1)
        self.assertIn("no ```json gatekeeper", result.stderr)
```

- [ ] **Step 2: Run — expect PASS**

Run: `python3 -m unittest tests.hooks.parse_gatekeeper_report_test.CliTests`
Expected: `OK` (two tests).

- [ ] **Step 3: Run the full suite as a final check**

Run: `python3 -m unittest discover -s tests/hooks -p "*_test.py" -v`
Expected: All 8 tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/hooks/parse_gatekeeper_report_test.py
git commit -m "test(hooks): cover CLI entrypoint end-to-end via subprocess"
```

---

### Task 1.10: Chunk-close — verify, push branch, open PR

- [ ] **Step 1: Run the full parser test suite one more time**

Run: `python3 -m unittest discover -s tests/hooks -p "*_test.py" -v`
Expected: 8 passing tests, 0 failures.

- [ ] **Step 2: Run linters (optional but recommended)**

Run: `python3 -m py_compile .claude/hooks/parse_gatekeeper_report.py`
Expected: no output (exit 0). If `pyflakes` is installed: `pyflakes .claude/hooks/parse_gatekeeper_report.py`.

- [ ] **Step 3: Push the branch and open the PR**

This chunk is self-contained. Per the user's PR-isolation rule, it lands as its own PR before Chunk 2 starts.

```bash
git push -u origin refactor/improve-slash-implement-command
gh pr create --title "feat(hooks): gatekeeper JSON report parser" --body "$(cat <<'EOF'
## Summary

- Adds `.claude/hooks/parse_gatekeeper_report.py`: stdlib-only parser that extracts the first \`\`\`json gatekeeper fenced block and validates \`report_version: 1\`.
- Adds `tests/hooks/parse_gatekeeper_report_test.py`: 8 unit tests covering happy path, malformed fence, malformed JSON, multi-fence precedence, and `report_version` checks.

This is Chunk 1 of the `/implement` harness enforcement rollout (see `docs/superpowers/specs/2026-04-21-implement-harness-enforcement-design.md` and `docs/superpowers/plans/2026-04-21-implement-harness-enforcement-plan.md`).

## Test plan

- [x] `python3 -m unittest discover -s tests/hooks -p "*_test.py" -v` → 8 passing
- [x] Parser rejects every defined error case
- [x] CLI entrypoint round-trips valid input through stdin/stdout
EOF
)"
```

- [ ] **Step 4: Wait for review, merge, then start Chunk 2**

After the PR is merged to `main`, pull latest and start Chunk 2 on the same branch (or a fresh one):

```bash
git checkout main && git pull && git checkout -b refactor/improve-slash-implement-command-chunk-2
```

---

## Chunk 1 — Recap

By the end of this chunk:

- `.claude/hooks/parse_gatekeeper_report.py` is a working stdlib-only parser.
- 8 unit tests cover the parser's contract.
- A CLI entrypoint lets shell callers pipe `gh`-style subagent output into it and consume compact JSON.
- The PR has landed; the parser is ready to be imported (indirectly via the CLI) by the gatekeeper conformance runner in Chunk 2.

Total commits in this chunk: 8 (one per TDD cycle) + 1 PR. ~50 lines of production code, ~150 lines of tests.

---

## Chunk 2: Gatekeeper Subagent + Fixtures + Conformance Runner

**Goal:** Create `subagent-gatekeeper` with its system prompt, 10 issue JSON fixtures covering the DOR permutation space, and a bash conformance runner that invokes the subagent against each fixture and asserts on the returned report. Add the subagent to `.claude/CLAUDE.md`'s registry.

**Files:**
- Create: `.claude/agents/subagent-gatekeeper.md`
- Create: `tests/gatekeeper/fixtures/good-feature.json`
- Create: `tests/gatekeeper/fixtures/good-bug.json`
- Create: `tests/gatekeeper/fixtures/missing-acceptance.json`
- Create: `tests/gatekeeper/fixtures/weak-acceptance.json`
- Create: `tests/gatekeeper/fixtures/blocker-open.json`
- Create: `tests/gatekeeper/fixtures/blocker-self-ref.json`
- Create: `tests/gatekeeper/fixtures/endpoint-no-schema.json`
- Create: `tests/gatekeeper/fixtures/cross-repo-blocker.json`
- Create: `tests/gatekeeper/fixtures/docs-label.json`
- Create: `tests/gatekeeper/fixtures/refactor-label.json`
- Create: `tests/gatekeeper/expected/<matching>.json` (expected reports, one per fixture)
- Create: `tests/gatekeeper/run-conformance.sh`
- Modify: `.claude/CLAUDE.md` (registry table)

### Task 2.1: Build the fixtures — full DOR feature issue (PASS case)

**Files:**
- Create: `tests/gatekeeper/fixtures/good-feature.json`
- Create: `tests/gatekeeper/expected/good-feature.expected.json`

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/good-feature.json`

```json
{
  "title": "feat: add retry policy to session manager",
  "body": "## Overview\n\nAdd configurable retry policy for transient session failures.\n\n## Expected Behavior\n\nSession failures with transient errors (network, timeout) retry up to N times with exponential backoff.\n\n## Acceptance Criteria\n\n- [ ] Retry count is configurable via maestro.toml\n- [ ] Exponential backoff between attempts\n- [ ] Permanent errors do not retry\n- [ ] Metrics track retry count per session\n\n## Files to Modify\n\n- src/session/manager.rs\n- src/config.rs\n\n## Test Hints\n\nMock the Claude CLI subprocess via a trait-based fake. Assert retry count matches config.\n\n## Blocked By\n\n- None\n\n## Definition of Done\n\n- [ ] Tests pass\n- [ ] Docs updated\n- [ ] Feature flag added",
  "labels": [{"name": "type:feature"}, {"name": "area:session"}],
  "assignees": [],
  "milestone": {"number": 5, "title": "v0.15.0"},
  "state": "OPEN",
  "comments": []
}
```

- [ ] **Step 2: Write the expected report**

File: `tests/gatekeeper/expected/good-feature.expected.json`

```json
{
  "report_version": 1,
  "status": "PASS",
  "task_type": "implementation",
  "dor": {"passed": true, "missing_sections": [], "weak_sections": []},
  "blockers": {"passed": true, "open": []},
  "contracts": {"passed": true, "missing": []}
}
```

(Only the assertion-critical fields need to match exactly. The conformance runner tolerates extra fields in the actual report — it asserts only on the fields present in the expected file.)

- [ ] **Step 3: Commit**

```bash
mkdir -p tests/gatekeeper/fixtures tests/gatekeeper/expected
git add tests/gatekeeper/fixtures/good-feature.json tests/gatekeeper/expected/good-feature.expected.json
git commit -m "test(gatekeeper): add good-feature fixture (full DOR, PASS)"
```

---

### Task 2.2: Build the fixtures — full DOR bug issue (PASS case)

**Files:**
- Create: `tests/gatekeeper/fixtures/good-bug.json`
- Create: `tests/gatekeeper/expected/good-bug.expected.json`

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/good-bug.json`

```json
{
  "title": "bug: stream-json parser crashes on empty chunk",
  "body": "## Overview\n\nThe stream-json parser panics when the Claude CLI emits an empty chunk mid-stream.\n\n## Current Behavior\n\nPanics with `index out of bounds` at parser.rs:184.\n\n## Expected Behavior\n\nEmpty chunks are silently skipped; the parser continues on the next frame.\n\n## Steps to Reproduce\n\n1. Run a long Claude session that emits a zero-byte stdout line.\n2. Observe the panic in maestro's TUI.\n\n## Acceptance Criteria\n\n- [ ] Parser no longer panics on empty chunks\n- [ ] An integration test reproduces the original panic on the pre-fix code\n- [ ] Metrics track skipped-empty-chunk count\n\n## Blocked By\n\n- None\n\n## Definition of Done\n\n- [ ] Tests pass\n- [ ] Regression test added",
  "labels": [{"name": "type:bug"}, {"name": "area:parser"}],
  "assignees": [],
  "milestone": null,
  "state": "OPEN",
  "comments": []
}
```

- [ ] **Step 2: Write the expected report**

File: `tests/gatekeeper/expected/good-bug.expected.json`

```json
{
  "report_version": 1,
  "status": "PASS",
  "task_type": "implementation",
  "dor": {"passed": true, "missing_sections": [], "weak_sections": []},
  "blockers": {"passed": true, "open": []},
  "contracts": {"passed": true, "missing": []}
}
```

- [ ] **Step 3: Commit**

```bash
git add tests/gatekeeper/fixtures/good-bug.json tests/gatekeeper/expected/good-bug.expected.json
git commit -m "test(gatekeeper): add good-bug fixture (full DOR, PASS)"
```

---

### Task 2.3: Build the fixtures — missing `## Acceptance Criteria`

**Files:**
- Create: `tests/gatekeeper/fixtures/missing-acceptance.json`
- Create: `tests/gatekeeper/expected/missing-acceptance.expected.json`

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/missing-acceptance.json`

```json
{
  "title": "feat: add theming support",
  "body": "## Overview\n\nAllow users to customize the TUI color scheme.\n\n## Expected Behavior\n\nTheme loads from maestro.toml; defaults to dark.\n\n## Files to Modify\n\n- src/tui/theme.rs (new)\n- src/config.rs\n\n## Test Hints\n\nUnit-test theme parsing. Snapshot-test rendering.\n\n## Blocked By\n\n- None\n\n## Definition of Done\n\n- [ ] Tests pass",
  "labels": [{"name": "type:feature"}],
  "assignees": [],
  "milestone": null,
  "state": "OPEN",
  "comments": []
}
```

- [ ] **Step 2: Write the expected report**

File: `tests/gatekeeper/expected/missing-acceptance.expected.json`

```json
{
  "report_version": 1,
  "status": "FAIL",
  "task_type": "implementation",
  "dor": {"passed": false, "missing_sections": ["Acceptance Criteria"], "weak_sections": []},
  "blockers": {"passed": true, "open": []},
  "contracts": {"passed": true, "missing": []}
}
```

The `remediation.comment_body` and `remediation.labels_to_add` are also expected to be populated, but the conformance runner asserts only the structural fields above. The comment body is prose; we verify its existence (non-empty), not its exact wording.

- [ ] **Step 3: Commit**

```bash
git add tests/gatekeeper/fixtures/missing-acceptance.json tests/gatekeeper/expected/missing-acceptance.expected.json
git commit -m "test(gatekeeper): add fixture for missing ## Acceptance Criteria"
```

---

### Task 2.4: Build the fixtures — weak `## Acceptance Criteria` (prose only)

**Files:**
- Create: `tests/gatekeeper/fixtures/weak-acceptance.json`
- Create: `tests/gatekeeper/expected/weak-acceptance.expected.json`

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/weak-acceptance.json`

```json
{
  "title": "feat: add keyboard shortcut for quit",
  "body": "## Overview\n\nAdd Q keybind to quit maestro.\n\n## Expected Behavior\n\nQ exits the TUI.\n\n## Acceptance Criteria\n\nPressing Q should quit the app cleanly with no resource leaks. Should also work from any screen.\n\n## Files to Modify\n\n- src/tui/app.rs\n\n## Test Hints\n\nIntegration test on the event loop.\n\n## Blocked By\n\n- None\n\n## Definition of Done\n\n- [ ] Tests pass",
  "labels": [{"name": "type:feature"}],
  "state": "OPEN",
  "milestone": null,
  "assignees": [],
  "comments": []
}
```

Note: `## Acceptance Criteria` is free prose — no `- [ ]` checklist items.

- [ ] **Step 2: Write the expected report**

File: `tests/gatekeeper/expected/weak-acceptance.expected.json`

```json
{
  "report_version": 1,
  "status": "FAIL",
  "task_type": "implementation",
  "dor": {"passed": false, "missing_sections": [], "weak_sections": ["Acceptance Criteria"]},
  "blockers": {"passed": true, "open": []},
  "contracts": {"passed": true, "missing": []}
}
```

- [ ] **Step 3: Commit**

```bash
git add tests/gatekeeper/fixtures/weak-acceptance.json tests/gatekeeper/expected/weak-acceptance.expected.json
git commit -m "test(gatekeeper): add fixture for weak ## Acceptance Criteria (prose only)"
```

---

### Task 2.5: Build the fixtures — open blocker

**Files:**
- Create: `tests/gatekeeper/fixtures/blocker-open.json`
- Create: `tests/gatekeeper/expected/blocker-open.expected.json`
- Create: `tests/gatekeeper/fixtures/blocker-open.gh-mock.json` (mock response for `gh issue view 42`)

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/blocker-open.json`

```json
{
  "title": "feat: wire retry policy into session manager",
  "body": "## Overview\n\nConsume the retry policy defined upstream.\n\n## Expected Behavior\n\nSessions apply retry policy from maestro.toml.\n\n## Acceptance Criteria\n\n- [ ] Sessions honor retry limit\n- [ ] Sessions honor backoff config\n\n## Files to Modify\n\n- src/session/manager.rs\n\n## Test Hints\n\nReuse the fake CLI trait from #42.\n\n## Blocked By\n\n- #42\n\n## Definition of Done\n\n- [ ] Tests pass",
  "labels": [{"name": "type:feature"}],
  "state": "OPEN",
  "milestone": null,
  "assignees": [],
  "comments": []
}
```

- [ ] **Step 2: Write the gh mock response**

File: `tests/gatekeeper/fixtures/blocker-open.gh-mock.json`

```json
{
  "42": {"state": "OPEN", "title": "feat: retry policy definition"}
}
```

The conformance runner (Task 2.12) will use this to mock blocker lookups.

- [ ] **Step 3: Write the expected report**

File: `tests/gatekeeper/expected/blocker-open.expected.json`

```json
{
  "report_version": 1,
  "status": "FAIL",
  "task_type": "implementation",
  "dor": {"passed": true, "missing_sections": [], "weak_sections": []},
  "blockers": {
    "passed": false,
    "open": [{"number": 42, "state": "OPEN"}]
  },
  "contracts": {"passed": true, "missing": []}
}
```

- [ ] **Step 4: Commit**

```bash
git add tests/gatekeeper/fixtures/blocker-open.json tests/gatekeeper/fixtures/blocker-open.gh-mock.json tests/gatekeeper/expected/blocker-open.expected.json
git commit -m "test(gatekeeper): add fixture for open blocker (FAIL)"
```

---

### Task 2.6: Build the fixtures — self-referential blocker

**Files:**
- Create: `tests/gatekeeper/fixtures/blocker-self-ref.json`
- Create: `tests/gatekeeper/expected/blocker-self-ref.expected.json`

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/blocker-self-ref.json`

The issue number in the JSON's `number` field is `100`, and `## Blocked By: - #100` points to itself:

```json
{
  "number": 100,
  "title": "feat: stuck in a loop",
  "body": "## Overview\n\n## Expected Behavior\n\nRun.\n\n## Acceptance Criteria\n\n- [ ] Run.\n\n## Files to Modify\n\n- src/lib.rs\n\n## Test Hints\n\nNone.\n\n## Blocked By\n\n- #100\n\n## Definition of Done\n\n- [ ] Tests pass",
  "labels": [{"name": "type:feature"}],
  "state": "OPEN",
  "milestone": null,
  "assignees": [],
  "comments": []
}
```

- [ ] **Step 2: Write the expected report**

File: `tests/gatekeeper/expected/blocker-self-ref.expected.json`

```json
{
  "report_version": 1,
  "status": "FAIL",
  "task_type": "implementation",
  "blockers": {"passed": false, "open": []}
}
```

The `reasons` field must include "self-referential". Conformance runner will grep for the substring.

- [ ] **Step 3: Commit**

```bash
git add tests/gatekeeper/fixtures/blocker-self-ref.json tests/gatekeeper/expected/blocker-self-ref.expected.json
git commit -m "test(gatekeeper): add fixture for self-referential blocker"
```

---

### Task 2.7: Build the fixtures — API endpoint without schema

**Files:**
- Create: `tests/gatekeeper/fixtures/endpoint-no-schema.json`
- Create: `tests/gatekeeper/expected/endpoint-no-schema.expected.json`

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/endpoint-no-schema.json`

```json
{
  "title": "feat: integrate new items API",
  "body": "## Overview\n\nConsume the new `POST /api/items` endpoint.\n\n## Expected Behavior\n\nmaestro posts items when state changes.\n\n## Acceptance Criteria\n\n- [ ] POST /api/items called on state transitions\n- [ ] Response parsed into ItemResponse struct\n\n## Files to Modify\n\n- src/api/client.rs\n\n## Test Hints\n\nUse wiremock for the HTTP layer.\n\n## Blocked By\n\n- None\n\n## Definition of Done\n\n- [ ] Tests pass",
  "labels": [{"name": "type:feature"}],
  "state": "OPEN",
  "milestone": null,
  "assignees": [],
  "comments": []
}
```

- [ ] **Step 2: Write the expected report**

File: `tests/gatekeeper/expected/endpoint-no-schema.expected.json`

```json
{
  "report_version": 1,
  "status": "FAIL",
  "task_type": "implementation",
  "contracts": {"passed": false, "missing": ["POST /api/items"]}
}
```

(Assumes `docs/api-contracts/` does not contain a schema for `POST /api/items`. The conformance runner sets up a temp `docs/api-contracts/` directory so this assumption is controlled.)

- [ ] **Step 3: Commit**

```bash
git add tests/gatekeeper/fixtures/endpoint-no-schema.json tests/gatekeeper/expected/endpoint-no-schema.expected.json
git commit -m "test(gatekeeper): add fixture for endpoint-without-schema"
```

---

### Task 2.8: Build the fixtures — cross-repo blocker

**Files:**
- Create: `tests/gatekeeper/fixtures/cross-repo-blocker.json`
- Create: `tests/gatekeeper/expected/cross-repo-blocker.expected.json`
- Create: `tests/gatekeeper/fixtures/cross-repo-blocker.gh-mock.json`

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/cross-repo-blocker.json`

```json
{
  "title": "feat: integrate external library",
  "body": "## Overview\n\nUse the external crate once it's stable.\n\n## Expected Behavior\n\nmaestro depends on external-org/external-repo#123.\n\n## Acceptance Criteria\n\n- [ ] Dep added to Cargo.toml\n- [ ] Wire calls in session/manager.rs\n\n## Files to Modify\n\n- Cargo.toml\n- src/session/manager.rs\n\n## Test Hints\n\nMock the external crate's trait.\n\n## Blocked By\n\n- external-org/external-repo#123\n\n## Definition of Done\n\n- [ ] Tests pass",
  "labels": [{"name": "type:feature"}],
  "state": "OPEN",
  "milestone": null,
  "assignees": [],
  "comments": []
}
```

- [ ] **Step 2: Write the gh mock (cross-repo variant, CLOSED)**

File: `tests/gatekeeper/fixtures/cross-repo-blocker.gh-mock.json`

```json
{
  "external-org/external-repo#123": {"state": "CLOSED", "title": "feat: stabilize external crate"}
}
```

Blocker is CLOSED — so this fixture should PASS.

- [ ] **Step 3: Write the expected report**

File: `tests/gatekeeper/expected/cross-repo-blocker.expected.json`

```json
{
  "report_version": 1,
  "status": "PASS",
  "task_type": "implementation",
  "blockers": {"passed": true, "open": []}
}
```

- [ ] **Step 4: Commit**

```bash
git add tests/gatekeeper/fixtures/cross-repo-blocker.json tests/gatekeeper/fixtures/cross-repo-blocker.gh-mock.json tests/gatekeeper/expected/cross-repo-blocker.expected.json
git commit -m "test(gatekeeper): add fixture for cross-repo blocker (PASS)"
```

---

### Task 2.9: Build the fixtures — docs-label → task_type: docs

**Files:**
- Create: `tests/gatekeeper/fixtures/docs-label.json`
- Create: `tests/gatekeeper/expected/docs-label.expected.json`

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/docs-label.json`

```json
{
  "title": "docs: update CLAUDE.md subagent registry",
  "body": "## Overview\n\nAdd new subagents to the registry table.\n\n## Expected Behavior\n\nRegistry reflects all current subagents.\n\n## Acceptance Criteria\n\n- [ ] All subagents listed\n- [ ] Status column up to date\n\n## Files to Modify\n\n- .claude/CLAUDE.md\n\n## Blocked By\n\n- None\n\n## Definition of Done\n\n- [ ] Change committed",
  "labels": [{"name": "type:docs"}],
  "state": "OPEN",
  "milestone": null,
  "assignees": [],
  "comments": []
}
```

- [ ] **Step 2: Write the expected report**

File: `tests/gatekeeper/expected/docs-label.expected.json`

```json
{
  "report_version": 1,
  "status": "PASS",
  "task_type": "docs"
}
```

- [ ] **Step 3: Commit**

```bash
git add tests/gatekeeper/fixtures/docs-label.json tests/gatekeeper/expected/docs-label.expected.json
git commit -m "test(gatekeeper): add fixture for docs label (task_type: docs)"
```

---

### Task 2.10: Build the fixtures — refactor-label → task_type: refactor

**Files:**
- Create: `tests/gatekeeper/fixtures/refactor-label.json`
- Create: `tests/gatekeeper/expected/refactor-label.expected.json`

- [ ] **Step 1: Write the fixture**

File: `tests/gatekeeper/fixtures/refactor-label.json`

```json
{
  "title": "refactor: split tui/app.rs into modules",
  "body": "## Overview\n\nThe TUI app.rs has grown past our 400-line guardrail. Split into focused modules.\n\n## Expected Behavior\n\nBehavior is unchanged; file layout is cleaner.\n\n## Acceptance Criteria\n\n- [ ] No file exceeds 400 lines\n- [ ] All existing tests pass unchanged\n\n## Files to Modify\n\n- src/tui/app.rs (split)\n- src/tui/ (new modules)\n\n## Test Hints\n\nRe-run existing test suite; no new tests needed for a pure refactor.\n\n## Blocked By\n\n- None\n\n## Definition of Done\n\n- [ ] Tests pass\n- [ ] Guardrail check passes",
  "labels": [{"name": "type:refactor"}],
  "state": "OPEN",
  "milestone": null,
  "assignees": [],
  "comments": []
}
```

- [ ] **Step 2: Write the expected report**

File: `tests/gatekeeper/expected/refactor-label.expected.json`

```json
{
  "report_version": 1,
  "status": "PASS",
  "task_type": "refactor"
}
```

- [ ] **Step 3: Commit**

```bash
git add tests/gatekeeper/fixtures/refactor-label.json tests/gatekeeper/expected/refactor-label.expected.json
git commit -m "test(gatekeeper): add fixture for refactor label (task_type: refactor)"
```

---

### Task 2.11: Draft the gatekeeper subagent system prompt

**Files:**
- Create: `.claude/agents/subagent-gatekeeper.md`

- [ ] **Step 1: Read the reference for the subagent file format**

Read the existing `subagent-qa.md` and `subagent-security-analyst.md` for frontmatter + section conventions:

```bash
cat .claude/agents/subagent-qa.md | head -40
cat .claude/agents/subagent-security-analyst.md | head -40
```

Observe: YAML frontmatter, then a system prompt in markdown. Sections for role, checks, output contract.

- [ ] **Step 2: Write the subagent file**

File: `.claude/agents/subagent-gatekeeper.md`

```markdown
---
name: subagent-gatekeeper
description: DOR, Blocked By, and API-contract gatekeeper for /implement. Emits structured JSON report in a fenced code block. Consultive only — never mutates issues.
color: yellow
model: sonnet
tools: Read, Glob, Grep, Bash
---

# Gatekeeper Subagent

You are the first gate in the `/implement` flow. You receive a GitHub issue
JSON object (produced by `gh issue view`), verify it meets the Definition of
Ready requirements in `.claude/CLAUDE.md §3`, resolve any `Blocked By`
dependencies, confirm API contract presence if endpoints are referenced, and
classify the task type.

You are **consultive only**. You read issues. You never comment on, label,
close, or otherwise mutate GitHub state. The orchestrator performs all side
effects based on your report.

## Inputs

1. Full issue JSON (`title`, `body`, `labels`, `milestone`, `state`,
   `comments`, and optionally `number`).
2. The selected mode (`orchestrator` or `vibe`).
3. The repo name (`<owner>/<repo>`) for resolving blockers via `gh api`.

## Checks

### 1. DOR section presence

Required `##` headings vary by issue type. Classify the issue using this
priority:

1. If the issue has a `type:bug` label → bug template applies.
2. Else if the issue has a `type:feature` label → feature template.
3. Else if the body contains `## Steps to Reproduce` → bug template.
4. Else → feature template (default).

**Feature issues require:**
`## Overview`, `## Expected Behavior`, `## Acceptance Criteria`,
`## Files to Modify`, `## Test Hints`, `## Blocked By`,
`## Definition of Done`.

**Bug issues require:**
`## Overview`, `## Current Behavior`, `## Expected Behavior`,
`## Steps to Reproduce`, `## Acceptance Criteria`, `## Blocked By`,
`## Definition of Done`.

Missing required headings → add to `dor.missing_sections`.

### 2. DOR semantic quality

- `## Acceptance Criteria` must contain ≥1 markdown checklist item
  (`- [ ]` or `- [x]`). Free prose only → add to `dor.weak_sections`.
- `## Blocked By` must contain either a literal `- None` or one-or-more
  `- #NNN` / `- owner/repo#NNN` entries. Free prose like "none currently"
  → add to `dor.weak_sections`.

### 3. Blocker resolution

For each `#NNN` or `owner/repo#NNN` in `## Blocked By`:

- **Same-repo**: run `gh issue view NNN --json state,title --repo <owner>/<repo>`.
- **Cross-repo**: run `gh issue view owner/repo#NNN --json state,title`.
- **Self-referential** (the blocker number equals the current issue's
  number, if provided): do not resolve; add to `reasons` as
  "self-referential blocker: #NNN" and set `blockers.passed: false`.

Any blocker with `state != "CLOSED"` → add to `blockers.open` and set
`blockers.passed: false`.

**Bash usage constraint:** You may only invoke `gh` in read-only modes:
`gh issue view`, `gh api repos/.../issues/...` (GET), `gh auth status`.
**You must never run** `gh issue comment`, `gh issue edit`,
`gh issue close`, or any `gh api -X POST/PATCH/DELETE/PUT`. If a check
requires mutation, draft the text in `remediation.comment_body` and let
the orchestrator perform the mutation.

### 4. API contract presence

Regex the issue body for endpoint hints:

- `GET /`, `POST /`, `PUT /`, `PATCH /`, `DELETE /`, or any mention of
  `/api/` paths.

For each match, check `docs/api-contracts/` for a JSON schema file that
covers the endpoint (by filename heuristic or by reading each schema's
`endpoint` field). If no schema covers the endpoint, add to
`contracts.missing` and set `contracts.passed: false`.

### 5. Task type classification

Return one of: `implementation`, `docs`, `refactor`, `test-only`. Derived
from labels in this priority:

1. `type:docs` → `docs`
2. `type:refactor` → `refactor`
3. `type:test` → `test-only`
4. Otherwise → `implementation` (default).

## Output contract

Your response must contain exactly one fenced code block with the
`json gatekeeper` language tag. The block is parsed by
`.claude/hooks/parse_gatekeeper_report.py`. Prose above and below the
fence is ignored by the parser but read by humans — use it for
explanation and reasoning.

The fence must contain a JSON object with these fields:

    ```json gatekeeper
    {
      "report_version": 1,
      "status": "PASS" | "FAIL",
      "task_type": "implementation" | "docs" | "refactor" | "test-only",
      "dor": {
        "passed": boolean,
        "missing_sections": [string],
        "weak_sections": [string]
      },
      "blockers": {
        "passed": boolean,
        "open": [{"number": int, "title": string, "state": string}]
      },
      "contracts": {
        "passed": boolean,
        "missing": [string]
      },
      "remediation": {
        "comment_body": string,
        "labels_to_add": [string]
      },
      "reasons": [string]
    }
    ```

- `status` is `PASS` if and only if `dor.passed`, `blockers.passed`,
  and `contracts.passed` are all `true`.
- `remediation.comment_body` must be populated when `dor.passed` is
  `false`. Draft a markdown comment that lists the missing / weak
  sections and points to `.claude/CLAUDE.md §3` for the DOR table.
- `remediation.labels_to_add` must include `"needs-info"` when
  `dor.passed` is `false`. Empty otherwise.
- `reasons` is a human-readable list of bullets explaining every check
  that failed. Used by the orchestrator to print a digest when aborting.

## Consultive-only discipline

You **never**:
- Run `gh issue comment`, `gh issue edit`, `gh issue close`,
  `gh issue reopen`, `gh api -X POST/PATCH/DELETE/PUT`.
- Write or modify any file in the repo.
- Commit, push, or modify git state.
- Invoke other subagents.

You **always**:
- Read only.
- Draft remediation text for the orchestrator to post.
- Return a valid fenced `json gatekeeper` block with `report_version: 1`.
```

- [ ] **Step 3: Commit**

```bash
git add .claude/agents/subagent-gatekeeper.md
git commit -m "$(cat <<'EOF'
feat(agents): add subagent-gatekeeper for /implement pre-checks

Consultive subagent that verifies DOR sections, resolves Blocked By
dependencies, confirms API contract presence, and classifies task type.
Returns a structured report in a fenced ```json gatekeeper block with
report_version: 1 for the orchestrator to parse.

Departs from other consultive subagents by listing Bash as an available
tool — required for resolving blockers via `gh issue view`. The system
prompt enforces a read-only Bash discipline: no gh issue comment, edit,
close, or gh api mutating verbs. The orchestrator is the one that posts
comments and applies labels based on the gatekeeper's drafted remediation.
EOF
)"
```

---

### Task 2.12: Write the conformance runner

**Files:**
- Create: `tests/gatekeeper/run-conformance.sh`

- [ ] **Step 1: Draft the runner script**

File: `tests/gatekeeper/run-conformance.sh`

```bash
#!/usr/bin/env bash
# Gatekeeper conformance runner.
#
# For each fixture in tests/gatekeeper/fixtures/<name>.json:
#   1. If <name>.gh-mock.json exists, install a PATH-shim for `gh` that
#      returns canned JSON for issue lookups.
#   2. Invoke the subagent-gatekeeper via the Claude Code Agent tool
#      harness, passing the fixture as issue JSON.
#   3. Pipe the subagent's response through
#      .claude/hooks/parse_gatekeeper_report.py.
#   4. Compare key fields against tests/gatekeeper/expected/<name>.expected.json
#      using a structural-subset match (jq-based).
#
# Exit 0 if every fixture's parsed report matches its expected subset.
# Exit 1 if any fixture's report diverges or the subagent fails to emit
# a valid fence.

set -euo pipefail

FIXTURES_DIR="tests/gatekeeper/fixtures"
EXPECTED_DIR="tests/gatekeeper/expected"
PARSER=".claude/hooks/parse_gatekeeper_report.py"

if [ ! -d "$FIXTURES_DIR" ]; then
  echo "error: $FIXTURES_DIR not found. Run from repo root." >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for conformance runner (brew install jq)." >&2
  exit 2
fi

# Check for the subagent invocation tool.
# This runner expects a helper named `invoke-gatekeeper-subagent` that
# takes a fixture path on stdin and emits the subagent's raw response
# on stdout. See the inline stub at the bottom for the expected contract.
# For local iteration, a manually-copied subagent response can be piped
# into the parser directly.

if ! command -v invoke-gatekeeper-subagent >/dev/null 2>&1; then
  echo "info: invoke-gatekeeper-subagent not on PATH." >&2
  echo "  This runner requires a helper that invokes the subagent programmatically." >&2
  echo "  Skipping automated conformance (manual spot-check only for v1)." >&2
  echo ""
  echo "Results: 0 fixtures exercised (helper not installed; v1 manual mode)"
  exit 0
fi

passed=0
failed=0
failures=()

for fixture_file in "$FIXTURES_DIR"/*.json; do
  [ -e "$fixture_file" ] || continue
  name=$(basename "$fixture_file" .json)
  # Skip gh-mock sidecar files; they are not primary fixtures.
  if [[ "$name" == *.gh-mock ]]; then
    continue
  fi

  expected_file="$EXPECTED_DIR/${name}.expected.json"
  if [ ! -f "$expected_file" ]; then
    echo "  $name: no expected file at $expected_file" >&2
    failed=$((failed + 1))
    failures+=("$name: no expected file")
    continue
  fi

  gh_mock_file="$FIXTURES_DIR/${name}.gh-mock.json"
  if [ -f "$gh_mock_file" ]; then
    export GATEKEEPER_GH_MOCK="$gh_mock_file"
  else
    unset GATEKEEPER_GH_MOCK
  fi

  # Invoke the subagent.
  raw_response=$(invoke-gatekeeper-subagent < "$fixture_file") || {
    echo "  $name: subagent invocation failed" >&2
    failed=$((failed + 1))
    failures+=("$name: subagent error")
    continue
  }

  parsed=$(echo "$raw_response" | python3 "$PARSER") || {
    echo "  $name: parser failed on subagent output" >&2
    failed=$((failed + 1))
    failures+=("$name: parser error")
    continue
  }

  # Structural-subset match: for every key in expected_file, assert the
  # same key exists in parsed with the same value.
  if diff <(echo "$parsed" | jq -S .) <(jq -S . "$expected_file") | \
       grep -q "^<"; then
    # parsed has extra fields — fine. Check all expected fields match.
    mismatches=$(jq -n --argjson p "$parsed" --slurpfile e "$expected_file" '
      def subset(a; b): (a | keys[]) | . as $k | (a[$k] == b[$k]) or
        (a[$k] | type == "object" and subset(a[$k]; b[$k]));
      if (subset($e[0]; $p)) then "" else "mismatch" end
    ')
    if [ -n "$mismatches" ]; then
      echo "  $name: expected fields not matched in parsed report" >&2
      echo "    expected: $(jq -c . "$expected_file")" >&2
      echo "    parsed:   $parsed" >&2
      failed=$((failed + 1))
      failures+=("$name: field mismatch")
      continue
    fi
  fi

  passed=$((passed + 1))
  echo "  $name: PASS"
done

echo ""
echo "Results: $passed passed, $failed failed"

if [ $failed -gt 0 ]; then
  echo "Failures:"
  for f in "${failures[@]}"; do
    echo "  - $f"
  done
  exit 1
fi

exit 0
```

- [ ] **Step 2: Make the runner executable**

```bash
chmod +x tests/gatekeeper/run-conformance.sh
```

- [ ] **Step 3: Document how to invoke the subagent for conformance**

Because Claude Code's subagent invocation isn't a standard PATH binary, the conformance runner assumes a helper script exists. For v1, that helper is a manual wrapper the developer runs interactively. Add a short note to `tests/gatekeeper/README.md`:

File: `tests/gatekeeper/README.md`

```markdown
# Gatekeeper Conformance

This directory holds fixtures and an expected-subset matcher for the
`subagent-gatekeeper` consultive subagent.

## Running conformance

The runner (`run-conformance.sh`) expects a helper named
`invoke-gatekeeper-subagent` on `PATH` that takes a fixture JSON on
stdin and emits the subagent's raw response on stdout. Options:

1. **Interactive (v1 default):** Invoke the subagent manually in Claude
   Code for each fixture, copy-paste the response into
   `/tmp/subagent-response-<name>.txt`, and run the parser + matcher by
   hand.
2. **Scripted (v2):** Wire the Agent invocation into a small Python
   helper using the Claude Agent SDK. Deferred — see the spec's Open
   Questions section.

## Fixture conventions

- `<name>.json` — the issue JSON the subagent receives.
- `<name>.gh-mock.json` — optional mock for blocker lookups. When
  present, the conformance runner sets `GATEKEEPER_GH_MOCK=<path>` so
  the subagent's `gh` invocations can be shimmed.
- `<name>.expected.json` under `expected/` — the structural subset the
  parsed report must match.

## Adding a fixture

1. Draft a minimal GitHub issue JSON under `fixtures/`.
2. If the issue references blockers, add a `<name>.gh-mock.json` with
   canned `state`/`title` for each blocker number.
3. Hand-derive the expected report and save under `expected/`.
4. Run the conformance runner; iterate until the subagent's output
   matches.
```

- [ ] **Step 4: Commit**

```bash
git add tests/gatekeeper/run-conformance.sh tests/gatekeeper/README.md
git commit -m "$(cat <<'EOF'
test(gatekeeper): add conformance runner and documentation

Runner iterates over fixtures/*.json, invokes the subagent via an
external helper (invoke-gatekeeper-subagent, PATH-shimmed or hand-run),
parses the response through .claude/hooks/parse_gatekeeper_report.py,
and performs a structural-subset match against expected/*.expected.json.

v1 assumes interactive subagent invocation; a scripted helper using
the Agent SDK is deferred per the spec's open questions.
EOF
)"
```

---

### Task 2.13: Update `.claude/CLAUDE.md` subagent registry

**Files:**
- Modify: `.claude/CLAUDE.md`

- [ ] **Step 1: Read the current registry table**

```bash
grep -A 10 "## Subagent Registry" .claude/CLAUDE.md | head -20
```

Locate the table block.

- [ ] **Step 2: Add `subagent-gatekeeper` row**

Using `Edit`, insert a new row between `subagent-architect` and `subagent-qa` (or append at the end — chronological order matches creation):

Old:

```markdown
| Subagent | Purpose | Status |
|----------|---------|--------|
| `subagent-architect` | Architecture design and implementation planning | **Ready** |
| `subagent-qa` | QA engineering, test design, quality gates | **Ready** |
```

New:

```markdown
| Subagent | Purpose | Status |
|----------|---------|--------|
| `subagent-architect` | Architecture design and implementation planning | **Ready** |
| `subagent-gatekeeper` | DOR, Blocked By, API-contract gatekeeper for /implement | **Ready** |
| `subagent-qa` | QA engineering, test design, quality gates | **Ready** |
```

- [ ] **Step 3: Add the "Delegation Rules" row for gatekeeper**

Locate the `### Subagent Reference (Orchestrator Mode)` table and add:

```markdown
| **Pre-check gate (MANDATORY)** | `subagent-gatekeeper` |
```

as the first row, above the architect row.

- [ ] **Step 4: Commit**

```bash
git add .claude/CLAUDE.md
git commit -m "$(cat <<'EOF'
docs(claude-md): register subagent-gatekeeper in orchestrator agent

Adds gatekeeper to the subagent registry table and the delegation rules
table. Gatekeeper is the first mandatory step in /implement's flow
(before architect), per the harness enforcement spec.
EOF
)"
```

---

### Task 2.14: Chunk-close — manual conformance spot-check + PR

- [ ] **Step 1: Manually invoke the gatekeeper against `good-feature.json`**

Open a Claude Code session and invoke the subagent directly with the fixture content as input. Copy the full response. Save to `/tmp/good-feature-response.txt`.

- [ ] **Step 2: Parse the response**

```bash
cat /tmp/good-feature-response.txt | python3 .claude/hooks/parse_gatekeeper_report.py
```

Expected output: a compact JSON object with `"status": "PASS"`, `"task_type": "implementation"`, and `dor.passed: true`.

- [ ] **Step 3: Spot-check two more fixtures**

Repeat for `missing-acceptance.json` (expect `status: FAIL`, `dor.missing_sections: ["Acceptance Criteria"]`) and `blocker-open.json` (expect `status: FAIL`, `blockers.passed: false`).

- [ ] **Step 4: Document any prompt-tuning needed**

If the subagent's output doesn't match the expected subset for these three fixtures, iterate on `.claude/agents/subagent-gatekeeper.md`'s prompt until it does. Amend previous commits only if the change is small; otherwise add a `fix(agents)` commit.

- [ ] **Step 5: Push and open the PR**

```bash
git push
gh pr create --title "feat(agents): subagent-gatekeeper + fixtures + conformance runner" --body "$(cat <<'EOF'
## Summary

- Adds `.claude/agents/subagent-gatekeeper.md`: consultive subagent that verifies DOR, resolves `Blocked By` dependencies, confirms API contract presence, and classifies task type. Returns a fenced \`\`\`json gatekeeper report with `report_version: 1`.
- Adds 10 fixtures under `tests/gatekeeper/fixtures/` spanning the DOR permutation space (good-feature, good-bug, missing-acceptance, weak-acceptance, blocker-open, blocker-self-ref, endpoint-no-schema, cross-repo-blocker, docs-label, refactor-label).
- Adds `tests/gatekeeper/run-conformance.sh`: structural-subset matcher that invokes the subagent on each fixture and compares against `expected/*.expected.json`.
- Registers `subagent-gatekeeper` in `.claude/CLAUDE.md`'s subagent registry and delegation-rules tables.

Chunk 2 of the `/implement` harness enforcement rollout.

## Test plan

- [x] Gatekeeper emits valid fenced JSON report for `good-feature.json` fixture → `status: PASS`
- [x] Gatekeeper reports FAIL with `dor.missing_sections: ["Acceptance Criteria"]` for `missing-acceptance.json`
- [x] Gatekeeper reports FAIL with `blockers.passed: false` for `blocker-open.json`
- [ ] Full `run-conformance.sh` pass deferred until scripted subagent invocation helper is wired (v2, see spec open questions)
EOF
)"
```

---

## Chunk 2 — Recap

By the end:

- `.claude/agents/subagent-gatekeeper.md` is a working consultive subagent.
- 10 fixtures under `tests/gatekeeper/fixtures/` cover the DOR permutation space.
- `run-conformance.sh` is ready to run once a scripted subagent-invocation helper lands (v2).
- `.claude/CLAUDE.md` registers the gatekeeper in both registry tables.
- Manual spot-check of 3 fixtures confirms the subagent's prompt produces the expected report.

Total commits: ~14 (one per fixture task + subagent + runner + CLAUDE.md) + 1 PR.

---

## Chunk 3: Pre-check Hook + Bats Tests

**Goal:** Create `.claude/hooks/implement-gates.sh`, the mechanical pre-check hook that runs before anything else in `/implement`. Build it TDD with `bats`, using PATH-shim fixtures for `gh` and `cargo`. Exit-code semantics match the spec's table.

**Files:**
- Create: `.claude/hooks/implement-gates.sh`
- Create: `tests/hooks/implement-gates.bats`
- Create: `tests/hooks/fixtures/fake-gh.sh`
- Create: `tests/hooks/fixtures/fake-cargo.sh`
- Create: `tests/hooks/fixtures/fake-gh-unauthed.sh`
- Create: `tests/hooks/fixtures/init-test-repo.sh`

### Task 3.1: Install `bats` locally and verify

- [ ] **Step 1: Check if bats is available**

```bash
command -v bats && bats --version
```

If available, skip to Step 3.

- [ ] **Step 2: Install bats**

```bash
brew install bats-core
```

Verify: `bats --version` prints e.g. `Bats 1.x.x`.

- [ ] **Step 3: Smoke test**

```bash
cat > /tmp/smoke.bats <<'EOF'
@test "smoke" {
  [ 1 -eq 1 ]
}
EOF
bats /tmp/smoke.bats
```

Expected: `1 test, 0 failures`.

(No commit — this is tool setup.)

---

### Task 3.2: Scaffold bats test file + PATH-shim fixtures

**Files:**
- Create: `tests/hooks/implement-gates.bats`
- Create: `tests/hooks/fixtures/fake-gh.sh`
- Create: `tests/hooks/fixtures/fake-cargo.sh`
- Create: `tests/hooks/fixtures/init-test-repo.sh`

- [ ] **Step 1: Create fixture directory**

```bash
mkdir -p tests/hooks/fixtures
```

- [ ] **Step 2: Write fake-gh.sh (PATH shim for `gh`)**

File: `tests/hooks/fixtures/fake-gh.sh`

```bash
#!/usr/bin/env bash
# Test fixture: PATH-shim for `gh` CLI.
#
# Environment variables consumed:
#   FAKE_GH_AUTH_STATUS   — "authed" (default) or "unauthed"
#   FAKE_GH_ISSUE_STATE   — "OPEN" (default) or "CLOSED"
#   FAKE_GH_ISSUE_BODY    — body to return (default: minimal issue)
#   FAKE_GH_RETURN_CODE   — exit code for the subcommand (default: 0)
#
# Supports: gh auth status, gh issue view

set -euo pipefail

cmd="${1:-}"
sub="${2:-}"

if [ "$cmd" = "auth" ] && [ "$sub" = "status" ]; then
  if [ "${FAKE_GH_AUTH_STATUS:-authed}" = "authed" ]; then
    echo "github.com"
    echo "  ✓ Logged in to github.com as test-user"
    exit 0
  else
    echo "You are not logged into any GitHub hosts." >&2
    exit 1
  fi
fi

if [ "$cmd" = "issue" ] && [ "$sub" = "view" ]; then
  cat <<EOF
{
  "title": "test issue",
  "body": "${FAKE_GH_ISSUE_BODY:-## Overview\nTest\n## Expected Behavior\nTest\n## Acceptance Criteria\n- [ ] Test\n## Files to Modify\n- src/lib.rs\n## Test Hints\n- None\n## Blocked By\n- None\n## Definition of Done\n- [ ] Tests pass}",
  "labels": [{"name": "type:feature"}],
  "assignees": [],
  "milestone": null,
  "state": "${FAKE_GH_ISSUE_STATE:-OPEN}",
  "comments": []
}
EOF
  exit "${FAKE_GH_RETURN_CODE:-0}"
fi

echo "fake-gh: unsupported subcommand '$cmd $sub'" >&2
exit 2
```

```bash
chmod +x tests/hooks/fixtures/fake-gh.sh
```

- [ ] **Step 3: Write fake-cargo.sh (PATH shim for `cargo`)**

File: `tests/hooks/fixtures/fake-cargo.sh`

```bash
#!/usr/bin/env bash
# Test fixture: PATH-shim for `cargo`.
#
# Environment variables:
#   FAKE_CARGO_TEST_EXIT — exit code for `cargo test` (default: 0)
#   FAKE_CARGO_TEST_OUT  — stdout to print (default: "test result: ok")
#
# Supports only: cargo test (with any args).

cmd="${1:-}"

if [ "$cmd" = "test" ]; then
  echo "${FAKE_CARGO_TEST_OUT:-test result: ok. 5 passed; 0 failed; 0 ignored}"
  exit "${FAKE_CARGO_TEST_EXIT:-0}"
fi

echo "fake-cargo: unsupported subcommand '$cmd'" >&2
exit 2
```

```bash
chmod +x tests/hooks/fixtures/fake-cargo.sh
```

- [ ] **Step 4: Write init-test-repo.sh (creates a temp git repo for each test)**

File: `tests/hooks/fixtures/init-test-repo.sh`

```bash
#!/usr/bin/env bash
# Test fixture: create a scratch git repo in a temp dir.
# Prints the temp dir path to stdout.

set -euo pipefail

tmp=$(mktemp -d -t maestro-gate-test-XXXXXX)
cd "$tmp"
git init -q
git config user.email "test@example.com"
git config user.name "Test User"
# Seed with an initial commit so `git status` behaves like a real repo.
touch README.md
git add README.md
git commit -q -m "init"
echo "$tmp"
```

```bash
chmod +x tests/hooks/fixtures/init-test-repo.sh
```

- [ ] **Step 5: Write the bats skeleton**

File: `tests/hooks/implement-gates.bats`

```bash
#!/usr/bin/env bats
#
# Tests for .claude/hooks/implement-gates.sh
#
# Each test:
#   1. Creates a scratch git repo (via init-test-repo.sh).
#   2. Sets up PATH with fake-gh.sh and fake-cargo.sh in front.
#   3. Invokes the hook with environment overrides as needed.
#   4. Asserts exit code and relevant stdout/stderr.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HOOK="$REPO_ROOT/.claude/hooks/implement-gates.sh"
  FIXTURES="$REPO_ROOT/tests/hooks/fixtures"

  # Make the fakes visible as `gh` and `cargo` on PATH.
  SHIM_DIR="$(mktemp -d)"
  ln -s "$FIXTURES/fake-gh.sh" "$SHIM_DIR/gh"
  ln -s "$FIXTURES/fake-cargo.sh" "$SHIM_DIR/cargo"
  PATH="$SHIM_DIR:$PATH"
  export PATH

  # Scratch git repo.
  TEST_REPO="$("$FIXTURES/init-test-repo.sh")"
  cd "$TEST_REPO"
}

teardown() {
  cd /
  rm -rf "$TEST_REPO" "$SHIM_DIR"
}

# --- tests defined below, one per task ---
```

- [ ] **Step 6: Verify the skeleton runs (no tests yet, but bats should find the file)**

```bash
bats tests/hooks/implement-gates.bats
```

Expected: `0 tests` (no tests defined yet). This confirms bats finds the file and setup/teardown don't error.

- [ ] **Step 7: Commit**

```bash
git add tests/hooks/
git commit -m "$(cat <<'EOF'
test(hooks): scaffold bats suite for implement-gates with PATH shims

Adds fake-gh.sh, fake-cargo.sh, init-test-repo.sh fixtures and the
bats skeleton (setup/teardown). No tests yet — subsequent tasks add
tests for each exit-code path defined in the spec.
EOF
)"
```

---

### Task 3.3: TDD — Exit 1 when not in a git repo

**Files:**
- Modify: `tests/hooks/implement-gates.bats`
- Create: `.claude/hooks/implement-gates.sh`

- [ ] **Step 1: Write the failing test**

Add to `implement-gates.bats` (below the setup/teardown):

```bash
@test "exits 1 when not in a git repo" {
  cd "$(mktemp -d -t non-repo-XXXXXX)"
  run bash "$HOOK" 123
  [ "$status" -eq 1 ]
  [[ "$output" == *"not inside a git repository"* ]]
}
```

- [ ] **Step 2: Run — expect fail (hook doesn't exist)**

```bash
bats tests/hooks/implement-gates.bats
```

Expected: failure — hook script not found.

- [ ] **Step 3: Create minimal hook**

File: `.claude/hooks/implement-gates.sh`

```bash
#!/usr/bin/env bash
# Pre-check hook for /implement.
# Argument: $1 = issue number.

set -euo pipefail

issue_number="${1:-}"

if [ -z "$issue_number" ]; then
  echo "implement-gates: issue number required as first argument" >&2
  exit 1
fi

# Gate 1: must be inside a git repo.
if ! git rev-parse --git-dir >/dev/null 2>&1; then
  echo "implement-gates: not inside a git repository" >&2
  exit 1
fi
```

```bash
chmod +x .claude/hooks/implement-gates.sh
```

- [ ] **Step 4: Run — expect pass**

```bash
bats tests/hooks/implement-gates.bats
```

Expected: `1 test, 0 failures`.

- [ ] **Step 5: Commit**

```bash
git add .claude/hooks/implement-gates.sh tests/hooks/implement-gates.bats
git commit -m "feat(hooks): implement-gates.sh — git-repo gate (exit 1)"
```

---

### Task 3.4: TDD — Exit 1 when `gh` not installed

**Files:**
- Modify: `tests/hooks/implement-gates.bats`
- Modify: `.claude/hooks/implement-gates.sh`

- [ ] **Step 1: Write the failing test**

Add:

```bash
@test "exits 1 when gh CLI is not installed" {
  # Remove the shim from PATH.
  rm "$SHIM_DIR/gh"
  run bash "$HOOK" 123
  [ "$status" -eq 1 ]
  [[ "$output" == *"gh CLI not installed"* ]]
  [[ "$output" == *"brew install gh"* ]]
}
```

- [ ] **Step 2: Run — expect fail (hook doesn't check gh yet)**

Expected: exit 0 (current hook only checks git) — or passes straight through to next step and fails. Either way, our new assertion fails.

- [ ] **Step 3: Extend the hook**

Add after the git check:

```bash
# Gate 2: gh CLI must be installed.
if ! command -v gh >/dev/null 2>&1; then
  echo "implement-gates: gh CLI not installed. Install: brew install gh" >&2
  exit 1
fi
```

- [ ] **Step 4: Run — expect pass**

```bash
bats tests/hooks/implement-gates.bats
```

Expected: `2 tests, 0 failures`.

- [ ] **Step 5: Commit**

```bash
git add tests/hooks/implement-gates.bats .claude/hooks/implement-gates.sh
git commit -m "feat(hooks): gh-installed gate (exit 1)"
```

---

### Task 3.5: TDD — Exit 1 when `gh` not authed

**Files:**
- Modify: `tests/hooks/implement-gates.bats`
- Modify: `.claude/hooks/implement-gates.sh`

- [ ] **Step 1: Write the failing test**

Add:

```bash
@test "exits 1 when gh is not authenticated" {
  export FAKE_GH_AUTH_STATUS=unauthed
  run bash "$HOOK" 123
  [ "$status" -eq 1 ]
  [[ "$output" == *"gh not authenticated"* ]]
  [[ "$output" == *"gh auth login"* ]]
}
```

- [ ] **Step 2: Run — expect fail (no auth check yet)**

- [ ] **Step 3: Extend the hook**

Add after the gh-installed check:

```bash
# Gate 3: gh must be authenticated.
if ! gh auth status >/dev/null 2>&1; then
  echo "implement-gates: gh not authenticated. Run: gh auth login" >&2
  exit 1
fi
```

- [ ] **Step 4: Run — expect pass**

Expected: `3 tests, 0 failures`.

- [ ] **Step 5: Commit**

```bash
git add tests/hooks/implement-gates.bats .claude/hooks/implement-gates.sh
git commit -m "feat(hooks): gh-authed gate (exit 1)"
```

---

### Task 3.6: TDD — Fetch issue JSON and cache in `$GATE_LOG_DIR`

**Files:**
- Modify: `tests/hooks/implement-gates.bats`
- Modify: `.claude/hooks/implement-gates.sh`

- [ ] **Step 1: Write the failing test**

Add:

```bash
@test "caches issue JSON to GATE_LOG_DIR/issue.json" {
  run bash "$HOOK" 123
  # Look for the log dir path in the hook's output.
  [[ "$output" == *"/tmp/maestro-123-"* ]]

  # Extract the log dir from the output.
  log_dir=$(echo "$output" | grep -oE '/tmp/maestro-123-[0-9]+' | head -1)
  [ -f "$log_dir/issue.json" ]

  # Verify the cached JSON parses.
  python3 -c "import json; json.load(open('$log_dir/issue.json'))"
}
```

- [ ] **Step 2: Run — expect fail**

- [ ] **Step 3: Extend the hook**

Add after the gh-authed check:

```bash
# Gate 4: fetch and cache the issue JSON.
GATE_LOG_DIR="/tmp/maestro-${issue_number}-$(date +%s)"
mkdir -p "$GATE_LOG_DIR"
echo "gate log dir: $GATE_LOG_DIR"

if ! gh issue view "$issue_number" \
  --json title,body,labels,assignees,milestone,state,comments \
  > "$GATE_LOG_DIR/issue.json" 2>"$GATE_LOG_DIR/gh-error.log"; then
  echo "implement-gates: failed to fetch issue #${issue_number}" >&2
  cat "$GATE_LOG_DIR/gh-error.log" >&2
  exit 1
fi

export GATE_LOG_DIR
```

- [ ] **Step 4: Run — expect pass**

Expected: `4 tests, 0 failures`.

- [ ] **Step 5: Commit**

```bash
git add tests/hooks/implement-gates.bats .claude/hooks/implement-gates.sh
git commit -m "feat(hooks): fetch and cache issue JSON in GATE_LOG_DIR"
```

---

### Task 3.7: TDD — Exit 1 when issue is CLOSED

**Files:**
- Modify: `tests/hooks/implement-gates.bats`
- Modify: `.claude/hooks/implement-gates.sh`

- [ ] **Step 1: Write the failing test**

```bash
@test "exits 1 when issue is CLOSED" {
  export FAKE_GH_ISSUE_STATE=CLOSED
  run bash "$HOOK" 123
  [ "$status" -eq 1 ]
  [[ "$output" == *"Issue #123 is CLOSED"* ]]
}
```

- [ ] **Step 2: Run — expect fail**

- [ ] **Step 3: Extend the hook**

Add after the issue-fetch block:

```bash
# Gate 5: issue must not be CLOSED.
issue_state=$(python3 -c "import json; print(json.load(open('$GATE_LOG_DIR/issue.json'))['state'])")
if [ "$issue_state" = "CLOSED" ]; then
  echo "implement-gates: Issue #${issue_number} is CLOSED. Re-open or pick a different issue." >&2
  exit 1
fi
```

- [ ] **Step 4: Run — expect pass**

- [ ] **Step 5: Commit**

```bash
git add tests/hooks/implement-gates.bats .claude/hooks/implement-gates.sh
git commit -m "feat(hooks): closed-issue gate (exit 1)"
```

---

### Task 3.8: TDD — Dirty tree: abort on (A)

**Files:**
- Modify: `tests/hooks/implement-gates.bats`
- Modify: `.claude/hooks/implement-gates.sh`

- [ ] **Step 1: Write the failing test**

```bash
@test "exits 6 on dirty tree when user chooses (A)bort" {
  # Create an uncommitted change.
  echo "dirty" > new-file.txt
  # Simulate user typing "A\n" to the prompt.
  run bash -c "echo 'A' | bash '$HOOK' 123"
  [ "$status" -eq 6 ]
  [[ "$output" == *"Working tree has uncommitted changes"* ]]
}
```

- [ ] **Step 2: Run — expect fail**

- [ ] **Step 3: Extend the hook**

Add after the closed-issue gate:

```bash
# Gate 6: working tree must be clean, or user must confirm stash.
if [ -n "$(git status --porcelain)" ]; then
  echo "implement-gates: Working tree has uncommitted changes"
  git status --short
  echo ""
  echo "(S)tash and continue, (A)bort"
  read -r choice
  case "$choice" in
    S|s)
      git stash push -m "auto-stash before /implement #${issue_number}"
      echo "implement-gates: stashed as 'auto-stash before /implement #${issue_number}'"
      ;;
    *)
      echo "implement-gates: aborting on dirty tree"
      exit 6
      ;;
  esac
fi
```

- [ ] **Step 4: Run — expect pass**

- [ ] **Step 5: Commit**

```bash
git add tests/hooks/implement-gates.bats .claude/hooks/implement-gates.sh
git commit -m "feat(hooks): dirty-tree gate with abort option (exit 6)"
```

---

### Task 3.9: TDD — Baseline-green assertion + dirty-tree stash path

**Merged from separate (S)tash and baseline-green tasks.** The stash path can only be end-to-end-tested once baseline-green lands (because the stash success case falls through to the next gate). So we land baseline-green + the stash-pass test + the stash-abort test in a single TDD cycle — no knowingly-failing intermediate commit to `main`.

**Files:**
- Modify: `tests/hooks/implement-gates.bats`
- Modify: `.claude/hooks/implement-gates.sh`

- [ ] **Step 1: Write the failing tests (three at once)**

```bash
@test "exits 2 when baseline cargo test fails" {
  export FAKE_CARGO_TEST_EXIT=1
  export FAKE_CARGO_TEST_OUT="test result: FAILED. 3 passed; 2 failed; 0 ignored"
  run bash "$HOOK" 123
  [ "$status" -eq 2 ]
  [[ "$output" == *"BASELINE NOT GREEN"* ]]
}

@test "continues past baseline when cargo test is green" {
  export FAKE_CARGO_TEST_EXIT=0
  run bash "$HOOK" 123
  [ "$status" -eq 0 ]
  [[ "$output" == *"gate log dir"* ]]
}

@test "dirty tree with (S)tash stashes and continues past baseline" {
  echo "dirty" > new-file.txt
  git add new-file.txt
  export FAKE_CARGO_TEST_EXIT=0
  run bash -c "echo 'S' | bash '$HOOK' 123"
  [ "$status" -eq 0 ]
  [[ "$output" == *"stashed as 'auto-stash before /implement #123'"* ]]
  [ -n "$(git stash list | grep 'auto-stash before /implement #123')" ]
}
```

- [ ] **Step 2: Run — expect all three fail (no baseline gate yet)**

```bash
bats tests/hooks/implement-gates.bats
```

Expected: three new failures.

- [ ] **Step 3: Extend the hook with the baseline gate**

Add after the dirty-tree gate. Use `if !` to safely interact with `set -e`:

```bash
# Gate 7: baseline cargo test must be green.
if ! cargo test --quiet > "$GATE_LOG_DIR/baseline.log" 2>&1; then
  echo "implement-gates: BASELINE NOT GREEN — existing tests are failing before /implement ran." >&2
  echo "implement-gates: The RED gate would pass for the wrong reason. Fix baseline first." >&2
  echo "implement-gates: See $GATE_LOG_DIR/baseline.log" >&2
  exit 2
fi
```

(The `if !` pattern is intentional: under `set -e`, a bare `cargo test` would abort the script. Wrapping it in `if` disables `set -e` for that specific command so the non-zero exit is captured and handled.)

- [ ] **Step 4: Run — expect all three pass**

```bash
bats tests/hooks/implement-gates.bats
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add tests/hooks/implement-gates.bats .claude/hooks/implement-gates.sh
git commit -m "$(cat <<'EOF'
feat(hooks): baseline-green gate + dirty-tree stash end-to-end

Adds the baseline cargo test gate (exit 2 on failure) and proves the
previously-added (A)bort dirty-tree path now has a fully-tested (S)tash
counterpart. Stash test was deliberately held until baseline-green
landed because its pass-case falls through to the baseline gate —
landing them separately would have committed a knowingly-failing test.
EOF
)"
```

---

### Task 3.10: TDD — Preflight bridge (present and absent)

**Files:**
- Modify: `tests/hooks/implement-gates.bats`
- Modify: `.claude/hooks/implement-gates.sh`

- [ ] **Step 1: Write the failing tests**

```bash
@test "calls preflight.sh when present and propagates exit code on failure" {
  mkdir -p .claude/hooks
  cat > .claude/hooks/preflight.sh <<'PFEOF'
#!/usr/bin/env bash
echo "preflight: simulated ci fail"
exit 7
PFEOF
  chmod +x .claude/hooks/preflight.sh
  export FAKE_CARGO_TEST_EXIT=0
  run bash "$HOOK" 123
  [ "$status" -eq 7 ]
  [[ "$output" == *"Pre-flight CI checks failed"* ]]
}

@test "calls preflight.sh when present and passes on exit 0" {
  mkdir -p .claude/hooks
  cat > .claude/hooks/preflight.sh <<'PFEOF'
#!/usr/bin/env bash
echo "preflight: all clear"
exit 0
PFEOF
  chmod +x .claude/hooks/preflight.sh
  export FAKE_CARGO_TEST_EXIT=0
  run bash "$HOOK" 123
  [ "$status" -eq 0 ]
  [[ "$output" == *"preflight: all clear"* ]]
}

@test "silently skips when preflight.sh is absent" {
  export FAKE_CARGO_TEST_EXIT=0
  run bash "$HOOK" 123
  [ "$status" -eq 0 ]
  [[ "$output" != *"preflight"* ]]
}
```

- [ ] **Step 2: Run — expect failures (no preflight wiring)**

- [ ] **Step 3: Extend the hook — with correct `set -e` handling**

Under `set -euo pipefail`, a bare `bash preflight.sh` would abort the outer script on non-zero before the exit-code capture runs. Use the `set +e` / `set -e` toggle so the exit code is captured cleanly and can be propagated (spec reserves codes 7+ for preflight):

Append:

```bash
# Gate 8 (optional): preflight bridge.
if [ -x .claude/hooks/preflight.sh ]; then
  set +e
  bash .claude/hooks/preflight.sh
  preflight_exit=$?
  set -e
  if [ $preflight_exit -ne 0 ]; then
    echo "implement-gates: Pre-flight CI checks failed. Fix before starting a new branch." >&2
    exit $preflight_exit
  fi
fi
```

- [ ] **Step 4: Run — expect pass**

Expected: all tests pass (the three new ones plus the previously-passing ones).

- [ ] **Step 5: Commit**

```bash
git add tests/hooks/implement-gates.bats .claude/hooks/implement-gates.sh
git commit -m "feat(hooks): preflight.sh bridge with set -e-safe exit-code capture"
```

---

### Task 3.11: Chunk-close — full bats run, PR

- [ ] **Step 1: Run the full suite**

```bash
bats tests/hooks/implement-gates.bats
```

Expected: `12 tests, 0 failures`.

- [ ] **Step 2: Optional — lint the shell script**

```bash
shellcheck .claude/hooks/implement-gates.sh tests/hooks/fixtures/*.sh
```

If `shellcheck` is installed, fix any warnings. If not installed, skip.

- [ ] **Step 3: Push and open PR**

```bash
git push
gh pr create --title "feat(hooks): implement-gates.sh pre-check hook with bats suite" --body "$(cat <<'EOF'
## Summary

- Adds `.claude/hooks/implement-gates.sh`: the mechanical pre-check hook invoked as the first step of `/implement`. Verifies git-repo state, `gh` install + auth, issue existence + open state, dirty tree (with stash option), baseline-green `cargo test`, and the optional `preflight.sh` bridge.
- Adds `tests/hooks/implement-gates.bats`: 12 tests covering every exit-code path defined in the spec (0, 1, 2, 6, 7).
- Adds `tests/hooks/fixtures/fake-gh.sh`, `fake-cargo.sh`, `init-test-repo.sh`: PATH-shim fixtures for deterministic bats runs without network.

Chunk 3 of the `/implement` harness enforcement rollout.

## Test plan

- [x] `bats tests/hooks/implement-gates.bats` → 12 passing
- [x] Hook exits with the correct code for each gate (see exit-code table in spec §Exit Code Convention)
- [x] Issue JSON is cached in `$GATE_LOG_DIR/issue.json` for downstream command use
EOF
)"
```

---

## Chunk 3 — Recap

By the end:

- `.claude/hooks/implement-gates.sh` runs 8 gates in order and exits with spec-defined codes.
- 12 bats tests cover every gate.
- PATH-shim fixtures let the bats suite run deterministically without network or a real `gh`/`cargo`.
- The hook caches issue JSON at `$GATE_LOG_DIR/issue.json` for the command to read without a second `gh` call.

Total commits: ~11 (one per TDD cycle) + 1 PR.

---

## Chunk 4: Rewrite `implement.md` + CLAUDE.md Workflow Updates

**Goal:** Replace the current `.claude/commands/implement.md` with a rewritten version that uses the hook, gatekeeper, parser, and inline RED/GREEN bash checkpoints from Chunks 1-3. Update the orchestrator workflow in `.claude/CLAUDE.md` to reference the gatekeeper as the mandatory first subagent.

**Files:**
- Modify: `.claude/commands/implement.md` (full rewrite)
- Modify: `.claude/CLAUDE.md` (workflow sections)

### Task 4.1: Preserve the current command and study it

- [ ] **Step 1: Save a backup copy**

```bash
cp .claude/commands/implement.md /tmp/implement-pre-rewrite.md
```

(No commit — this is a local safety copy that won't be tracked.)

- [ ] **Step 2: Re-read the current command and the spec's §Execution Flow section**

```bash
cat .claude/commands/implement.md
sed -n '/## Execution Flow/,/## Gatekeeper Subagent Contract/p' docs/superpowers/specs/2026-04-21-implement-harness-enforcement-design.md
```

Keep both side-by-side while writing the rewrite.

---

### Task 4.2: Rewrite `implement.md` — header, arg parsing, language/mode

**Files:**
- Modify: `.claude/commands/implement.md`

- [ ] **Step 1: Replace the top portion**

Using `Write` (full rewrite is cleaner than multiple edits), replace the file. Start with Steps 0-1:

```markdown
# Implement Issue

Fetch a GitHub issue and implement it following the enforced TDD harness.

**Usage:** `/implement #123` or `/implement 123 --english --orchestrator`

---

## Arguments

`$ARGUMENTS` contains the issue number and optional flags.

### Supported Flags

| Flag | Short | Purpose |
|------|-------|---------|
| `--english` | `-e` | Set language to English |
| `--portuguese` | `-pt` | Set language to Português do Brasil |
| `--spanish` | `-s` | Set language to Español |
| `--orchestrator` | `-o` | Use Subagents Orchestrator mode |
| `--vibe-coding` | `-vc` | Use Vibe Coding mode |

`--training` / `-t` is explicitly rejected — Training mode is for `.claude/` configuration, not implementation.

---

## Instructions

### Step 0: Parse arguments

Extract from `$ARGUMENTS`:
1. **Issue number**: first `\d+` with optional `#` prefix. Export as `$ISSUE_NUMBER` for downstream steps.
2. **Language flag** (if present).
3. **Mode flag** (if present).

```bash
export ISSUE_NUMBER="<n>"  # substitute the parsed number
```

All subsequent gate commands in this file reference `$ISSUE_NUMBER`, not bare `<n>`.

If `--training` or `-t` is detected, output:

```
Training mode is for configuring .claude/ agents, skills, and commands.
/implement is for building features from GitHub issues.
Drop --training, or use a free-form prompt for .claude/ configuration.
```

and exit 0 (not an error).

If no issue number found, ask: "Which issue should I implement?"

### Step 1: Language and mode selection

If flags provided, honor them. Otherwise, ask the user.

```

- [ ] **Step 2: Commit work-in-progress** (intermediate commits let us roll back if the rewrite gets tangled)

```bash
git add .claude/commands/implement.md
git commit -m "refactor(implement): rewrite — header, Step 0-1 (WIP)"
```

---

### Task 4.3: Rewrite `implement.md` — Step 2 hook + Step 3 cached-issue read

**Files:**
- Modify: `.claude/commands/implement.md`

- [ ] **Step 1: Append Step 2 and Step 3**

Append:

```markdown
### Step 2: Pre-check hook (GATE — MANDATORY)

Run the mechanical pre-check hook. Abort on non-zero exit, printing stderr verbatim.

```bash
bash .claude/hooks/implement-gates.sh "$ISSUE_NUMBER"
```

The hook prints `gate log dir: /tmp/maestro-$ISSUE_NUMBER-<ts>` on success; capture this path and `export GATE_LOG_DIR=<path>` for downstream steps.

Exit codes:
- `0` — proceed.
- `1` — generic failure (gh missing, not authed, not in repo, closed issue). Abort with the hook's stderr.
- `2` — baseline cargo test failing. Abort — fix the baseline before starting.
- `6` — dirty tree, user chose abort. Abort cleanly.
- `7+` — preflight.sh failure. Abort with its stderr.

### Step 3: Read cached issue JSON

The hook has cached the issue JSON at `$GATE_LOG_DIR/issue.json`. Read it directly — no second `gh` call.

```bash
cat "$GATE_LOG_DIR/issue.json"
```
```

- [ ] **Step 2: Commit**

```bash
git add .claude/commands/implement.md
git commit -m "refactor(implement): rewrite — Step 2 hook + Step 3 cached issue"
```

---

### Task 4.4: Rewrite `implement.md` — Step 4 gatekeeper + parser + side effects

**Files:**
- Modify: `.claude/commands/implement.md`

- [ ] **Step 1: Append Step 4**

Append:

```markdown
### Step 4: Gatekeeper (GATE — MANDATORY)

Invoke `subagent-gatekeeper` via the `Agent` tool. Pass the issue JSON (from `$GATE_LOG_DIR/issue.json`), selected mode, and repo name.

The subagent's response will contain a fenced `json gatekeeper` code block. Pipe its full response through the parser:

```bash
echo "$SUBAGENT_RESPONSE" | python3 .claude/hooks/parse_gatekeeper_report.py > "$GATE_LOG_DIR/gatekeeper.json"
```

Then branch on the parsed report:

```bash
status=$(jq -r .status "$GATE_LOG_DIR/gatekeeper.json")
task_type=$(jq -r .task_type "$GATE_LOG_DIR/gatekeeper.json")

if [ "$status" = "FAIL" ]; then
  dor_passed=$(jq -r .dor.passed "$GATE_LOG_DIR/gatekeeper.json")
  if [ "$dor_passed" = "false" ]; then
    # Auto-remediation for DOR failures.
    comment_body=$(jq -r .remediation.comment_body "$GATE_LOG_DIR/gatekeeper.json")
    gh issue comment "$ISSUE_NUMBER" --body "$comment_body"
    for label in $(jq -r '.remediation.labels_to_add[]' "$GATE_LOG_DIR/gatekeeper.json"); do
      gh issue edit "$ISSUE_NUMBER" --add-label "$label"
    done
  fi
  # Print reasons for the operator.
  echo "Gatekeeper FAIL:" >&2
  jq -r '.reasons[]' "$GATE_LOG_DIR/gatekeeper.json" | while read -r r; do
    echo "  - $r" >&2
  done
  exit 5
fi

# PASS — continue with the classified task_type.
echo "Gatekeeper PASS (task_type: $task_type)"
export TASK_TYPE="$task_type"
```

Exit code `5` is reserved for gatekeeper failure.
```

- [ ] **Step 2: Commit**

```bash
git add .claude/commands/implement.md
git commit -m "refactor(implement): rewrite — Step 4 gatekeeper + auto-remediation"
```

---

### Task 4.5: Rewrite `implement.md` — Step 5 branch + idempotency prompt

**Files:**
- Modify: `.claude/commands/implement.md`

- [ ] **Step 1: Append Step 5**

```markdown
### Step 5: Branch selection with idempotency

Check for an existing branch matching `feat/issue-<n>-*`:

```bash
existing=$(git branch --list "feat/issue-${ISSUE_NUMBER}-*" | head -1 | sed 's/^[ *]*//')
```

**If empty:** derive a slug from the issue title and create a new branch:

```bash
slug=$(jq -r .title "$GATE_LOG_DIR/issue.json" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-//;s/-$//' | cut -c -40)
git checkout -b "feat/issue-${ISSUE_NUMBER}-${slug}"
```

**If non-empty:** fire the idempotency prompt. Show recent commits:

```
Branch `<existing>` already exists.

Recent commits on that branch:
<git log main..HEAD --oneline -5>

  (C)ontinue on this branch
  (R)estart — delete branch and start over
  (A)bort

Choice [C/R/A]:
```

Handle each choice:

**(C)ontinue:**
- `git checkout <existing>`.
- Re-invoke the gatekeeper (idempotent).
- When delegating to architect/QA in Step 6, prepend the resumption context prompt from the spec (§Idempotency UX → Continue semantics):

  > **Context for resumption:** This branch already has commits. Here is the history since `main`:
  >
  > ```
  > $(git log --oneline main..HEAD)
  > ```
  >
  > Before producing a full blueprint, inspect these commits (via Read / Grep on the branch). If the work described by the issue appears substantially done (architecture scaffolded, tests present, or implementation in place), return a **minimal response** acknowledging the existing state and listing only what remains. Do not duplicate work already in the branch. If the branch diverges from what you would design (e.g., different module layout, different abstractions), flag the divergence and recommend either reconciling or restarting — do not silently layer a conflicting plan on top.

- **Divergence handling (spec §Idempotency UX → Divergence handling):** if the architect or QA subagent flags divergence, the orchestrator presents a secondary prompt:

  ```
  Architect detected divergence between the existing branch and the
  issue's requirements:

  <architect's divergence summary>

    (R)econcile — continue and let subagents bridge the gap
    (S)witch to Restart — delete and start over
    (A)bort — inspect manually

  Choice [R/S/A]:
  ```

  - **(R)econcile**: proceed with Step 6's subagent sequence, trusting the architect/QA to bridge the gap via follow-up edits.
  - **(S)witch to Restart**: fall through to the Restart flow below (typed `RESTART` confirmation, branch deletion).
  - **(A)bort**: exit cleanly, tell the user to inspect manually.

**(R)estart:**
- Require typed `RESTART` confirmation.
- `git checkout main && git branch -D "$existing"`.
- If the branch was pushed, prompt about remote deletion (default no).
- Create fresh branch.

**(A)bort:**
- Exit cleanly. Tell the user to `git checkout <branch>` manually to inspect.

If the user is already on the matching branch, skip the prompt and use Continue semantics.
```

- [ ] **Step 2: Commit**

```bash
git add .claude/commands/implement.md
git commit -m "refactor(implement): rewrite — Step 5 branch + idempotency prompt"
```

---

### Task 4.6: Rewrite `implement.md` — Step 6 subagent sequence + RED/GREEN

**Files:**
- Modify: `.claude/commands/implement.md`

- [ ] **Step 1: Append Step 6**

```markdown
### Step 6: Orchestrator-mode subagent sequence

Vibe mode skips 6a and 6c. All gates use `bash` (not `sh`) — `${PIPESTATUS[0]}` requires it.

#### 6a. `subagent-architect` → blueprint

Orchestrator mode only. Invoke `subagent-architect` with the issue JSON and the architecture blueprint request. If Step 5 chose Continue, prepend the resumption context prompt.

#### 6b. `/validate-contracts` (if architect blueprint touches API endpoints)

Skip if no endpoints.

#### 6c. `subagent-qa` → test blueprint

Orchestrator mode only. Invoke `subagent-qa` with the architect's blueprint. If Step 5 chose Continue, prepend the resumption context prompt.

#### 6d. Write tests from QA blueprint

You (the orchestrator) write tests. No subagent.

#### 6e. RED checkpoint (GATE)

Skipped if `TASK_TYPE` is `docs` or `refactor`.

```bash
if [ "$TASK_TYPE" != "docs" ] && [ "$TASK_TYPE" != "refactor" ]; then
  cargo test --quiet 2>&1 | tee "$GATE_LOG_DIR/red.log"
  red_exit=${PIPESTATUS[0]}
  if [ $red_exit -eq 0 ]; then
    echo "RED GATE FAILED — cargo test passed, but implementation has not started." >&2
    echo "Write a failing test for the new behavior before implementing." >&2
    exit 3
  fi
fi
```

Exit code `3` reserved for RED failure.

#### 6f. Implement

You (the orchestrator) write the minimum code to make the failing test pass.

#### 6g. GREEN checkpoint (GATE)

Skipped only if `TASK_TYPE` is `docs`.

```bash
if [ "$TASK_TYPE" != "docs" ]; then
  cargo test --quiet 2>&1 | tee "$GATE_LOG_DIR/green.log"
  green_exit=${PIPESTATUS[0]}
  if [ $green_exit -ne 0 ]; then
    echo "GREEN GATE FAILED — tests still failing after implementation." >&2
    echo "See $GATE_LOG_DIR/green.log" >&2
    exit 4
  fi
fi
```

Exit code `4` reserved for GREEN failure.

#### 6h. Refactor (if needed)

Refactor while tests stay green. Re-run the GREEN checkpoint after:

```bash
cargo test --quiet 2>&1 | tee "$GATE_LOG_DIR/green-refactor.log"
[ ${PIPESTATUS[0]} -eq 0 ] || { echo "Refactor broke tests"; exit 4; }
```

#### 6i. `subagent-security-analyst` → review

Both modes. Invoke security analyst against the newly-written code.

#### 6j. `subagent-docs-analyst` → docs + directory-tree.md

Both modes. Mandatory at task end.
```

- [ ] **Step 2: Commit**

```bash
git add .claude/commands/implement.md
git commit -m "refactor(implement): rewrite — Step 6 subagent sequence + RED/GREEN"
```

---

### Task 4.7: Rewrite `implement.md` — Step 7 handoff

**Files:**
- Modify: `.claude/commands/implement.md`

- [ ] **Step 1: Append Step 7**

```markdown
### Step 7: Handoff

Print a summary:

```
Implementation complete for Issue #<n>: <title>

Gates passed:
  - Pre-check hook (ok)
  - Gatekeeper (task_type: $TASK_TYPE)
  - RED checkpoint (verified failing → passing)
  - GREEN checkpoint (all tests pass)

Logs: $GATE_LOG_DIR

Next: run /pushup to commit, push, create PR, and close the issue.
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Generic failure (gh missing, not authed, not in repo, closed issue, training mode rejected) |
| 2 | Baseline cargo test failing |
| 3 | RED gate failed |
| 4 | GREEN gate failed |
| 5 | Gatekeeper FAIL (DOR, blockers, contracts) |
| 6 | Dirty tree, user declined stash |
| 7+ | Preflight failure (reserved for CI-gates spec) |

---

## Error Handling

- If `gh` CLI not installed → hook exits 1 with install hint.
- If `gh` not authenticated → hook exits 1 with `gh auth login` hint.
- If issue closed → hook exits 1. Re-open first or pick a different issue.
- If dirty tree → prompt (S)tash/(A)bort.
- If baseline fails → exit 2. Fix baseline first.
- If gatekeeper FAILs with DOR missing → comment posted, `needs-info` label applied, exit 5.
- If blockers open → exit 5. Wait for blockers to close.
- If RED/GREEN fails → exit 3/4. Actionable error with log path.

---

## Do Not

- Run `/implement` for the same issue concurrently in two sessions.
- Bypass the hook by invoking subagents directly.
- Skip the RED gate for `implementation` task types — write the failing test first.
```

- [ ] **Step 2: Commit**

```bash
git add .claude/commands/implement.md
git commit -m "refactor(implement): rewrite — Step 7 handoff + exit codes table"
```

---

### Task 4.8: Update `.claude/CLAUDE.md` orchestrator workflow

**Files:**
- Modify: `.claude/CLAUDE.md`

- [ ] **Step 1: Locate the Orchestrator Mode workflow section (§2)**

```bash
grep -n "Orchestrator Mode workflow is ALWAYS" .claude/CLAUDE.md
```

- [ ] **Step 2: Update the numbered workflow to include the gatekeeper step**

The current `CLAUDE.md` §2 lists 10 workflow steps. We're replacing the old step 2 (VERIFY DOR) with two steps (pre-check hook + gatekeeper), which bumps every downstream number by 1 (old 3→new 4, old 4→new 5, …, old 10→new 11). The Edit tool requires exact old/new strings, so read the current file first and use `Edit` with the complete block. Commands to run:

```bash
# Read the current workflow block to confirm exact text
sed -n '/Orchestrator Mode workflow is ALWAYS/,/^\*\*In 🎸 Vibe Coding Mode/p' .claude/CLAUDE.md
```

Then apply the Edit with the *full* old block and *full* new block. The complete replacement:

**Old (exact):**

```markdown
**Orchestrator Mode workflow is ALWAYS (TDD ENFORCED):**
1. Receive user request
2. **Verify DOR (Definition of Ready) - MANDATORY:**
   - Check the issue has all required DOR fields (see section 3)
   - If missing required fields → comment on issue, apply `needs-info` label, **STOP**
3. **Delegate to Architect for blueprint - MANDATORY:**
   - `subagent-architect` - For all architecture decisions
   - **NEVER skip architecture step - the architect MUST be called**
4. **CONTRACT VALIDATION (if task involves API endpoints) - MANDATORY:**
   - Run `/validate-contracts` to check existing models against `docs/api-contracts/` schemas
   - If no contract schema exists for the endpoint → **STOP and ask user to provide the JSON schema**
   - If contract exists but models mismatch → fix models BEFORE proceeding
5. **Delegate to QA for test blueprint - MANDATORY (TDD):**
   - `subagent-qa` - Provides test cases, mocks, and expected behaviors
   - **Tests are designed BEFORE implementation**
6. **Write tests FIRST (RED)** — verify they fail
7. **Implement minimum code (GREEN)** — make tests pass
8. **Refactor** — clean up while tests stay green
9. Delegate to Security for review of implemented code
10. Call docs-analyst at the end
```

**New (exact):**

```markdown
**Orchestrator Mode workflow is ALWAYS (TDD ENFORCED):**
1. Receive user request
2. **Pre-check hook (MANDATORY):**
   - Run `bash .claude/hooks/implement-gates.sh <issue-number>`
   - Abort on any non-zero exit (see exit-code table in `/implement`)
3. **Delegate to Gatekeeper (MANDATORY):**
   - `subagent-gatekeeper` → structured JSON report (DOR, blockers, contracts, task_type)
   - Parse via `.claude/hooks/parse_gatekeeper_report.py`
   - On DOR FAIL → orchestrator posts gatekeeper-drafted comment, applies `needs-info` label, **STOP**
   - On blocker/contract FAIL → **STOP** with reasons from the report
4. **Delegate to Architect for blueprint - MANDATORY:**
   - `subagent-architect` - For all architecture decisions
   - **NEVER skip architecture step - the architect MUST be called**
5. **CONTRACT VALIDATION (if task involves API endpoints) - MANDATORY:**
   - Run `/validate-contracts` to check existing models against `docs/api-contracts/` schemas
   - If no contract schema exists for the endpoint → **STOP and ask user to provide the JSON schema**
   - If contract exists but models mismatch → fix models BEFORE proceeding
6. **Delegate to QA for test blueprint - MANDATORY (TDD):**
   - `subagent-qa` - Provides test cases, mocks, and expected behaviors
   - **Tests are designed BEFORE implementation**
7. **Write tests FIRST (RED)** — verify they fail (enforced by `/implement` Step 6e bash gate)
8. **Implement minimum code (GREEN)** — make tests pass (enforced by `/implement` Step 6g bash gate)
9. **Refactor** — clean up while tests stay green
10. Delegate to Security for review of implemented code
11. Call docs-analyst at the end
```

- [ ] **Step 3: Update the §Mandatory Subagent Sequence list similarly**

Old:

```markdown
**Mandatory Subagent Sequence (IN THIS ORDER — TDD ENFORCED):**

1. VERIFY DOR → Check issue has all required fields (MANDATORY)
2. `subagent-architect` → Architecture Blueprint (MANDATORY)
```

New:

```markdown
**Mandatory Subagent Sequence (IN THIS ORDER — TDD ENFORCED):**

1. `bash .claude/hooks/implement-gates.sh` → Pre-check hook (MANDATORY)
2. `subagent-gatekeeper` → DOR + blockers + contracts + task_type (MANDATORY)
3. `subagent-architect` → Architecture Blueprint (MANDATORY)
```

Renumber the rest accordingly.

- [ ] **Step 4: Update the TDD flow diagram**

Locate the ASCII diagram in §5 and replace:

```
VERIFY DOR (Definition of Ready) → STOP if missing required fields
```

with:

```
Pre-check hook (implement-gates.sh) → STOP on any gate failure
    │
    ▼
subagent-gatekeeper → STOP if DOR/blockers/contracts FAIL
                      (auto-comment + needs-info on DOR FAIL)
```

Keep the rest of the diagram intact.

- [ ] **Step 5: Commit**

```bash
git add .claude/CLAUDE.md
git commit -m "$(cat <<'EOF'
docs(claude-md): wire gatekeeper + pre-check hook into orchestrator flow

Updates the §2 Orchestrator Mode workflow, the §Mandatory Subagent
Sequence list, and the §5 TDD flow diagram to reflect the enforced
harness from the /implement rewrite:

- Step 2 is now the pre-check hook (implement-gates.sh).
- Step 3 is the gatekeeper (subagent-gatekeeper).
- DOR auto-remediation (comment + needs-info label) is now explicitly
  the orchestrator's responsibility, not the subagent's.
EOF
)"
```

---

### Task 4.9: Chunk-close — smoke test + PR

- [ ] **Step 1: Spot-check the rewritten command**

Run `/implement #<some-issue>` in a sandbox session. Walk through each step, verify the hook fires, gatekeeper is invoked, and RED/GREEN checkpoints appear at the expected moments. Capture any issues.

If bugs surface, fix via follow-up commits before opening the PR.

- [ ] **Step 2: Push and open PR**

```bash
git push
gh pr create --title "refactor(implement): rewrite command + wire harness into CLAUDE.md" --body "$(cat <<'EOF'
## Summary

- Rewrites `.claude/commands/implement.md` to use the enforced harness from Chunks 1-3. Inline bash gates, structured subagent invocations via the parser, soft-idempotency prompt on re-run, RED/GREEN checkpoints skippable per `task_type`.
- Updates `.claude/CLAUDE.md`:
  - §2 Orchestrator Mode workflow — Step 2 is now the pre-check hook, Step 3 is the gatekeeper.
  - §5 TDD flow diagram — replaces "VERIFY DOR" with the hook + gatekeeper sequence.
  - §Mandatory Subagent Sequence — adds hook + gatekeeper at the top.

Chunk 4 of the `/implement` harness enforcement rollout.

## Test plan

- [x] Spot-check full flow against a real issue in a sandbox branch
- [x] Verify each exit code from the spec's table triggers correctly
- [x] Verify (C)/(R)/(A) prompt on re-running against an existing branch
- [x] Verify RED/GREEN gates fire for implementation task_type and skip correctly for docs/refactor
EOF
)"
```

---

## Chunk 4 — Recap

By the end:

- `.claude/commands/implement.md` is a concise, gate-enforced command.
- `.claude/CLAUDE.md`'s orchestrator workflow reflects the new sequence.
- The full harness is end-to-end usable against real GitHub issues.

Total commits: ~8 (one per rewrite section) + 1 PR.

---

## Chunk 5: Acceptance Checklist + E2E Walk

**Goal:** Author `docs/harness-acceptance.md`, the manual end-to-end checklist defined in the spec's Layer-4 testing strategy. Run it against a real GitHub issue and capture any regressions.

**Files:**
- Create: `docs/harness-acceptance.md`

### Task 5.1: Author the acceptance checklist

**Files:**
- Create: `docs/harness-acceptance.md`

- [ ] **Step 1: Draft the checklist**

File: `docs/harness-acceptance.md`

```markdown
# /implement Harness Acceptance Checklist

Run this checklist before any release that modifies `.claude/commands/implement.md`, `.claude/hooks/implement-gates.sh`, `.claude/hooks/parse_gatekeeper_report.py`, or `.claude/agents/subagent-gatekeeper.md`.

Total time: ~10 minutes.

---

## Prerequisites

- A scratch GitHub issue in this repo or a test repo. Ideally two: one with full DOR, one with missing Acceptance Criteria.
- `gh` authenticated to the relevant repo.
- Working tree clean.
- Baseline `cargo test` green.

---

## Acceptance runs

### 1. Happy path — full DOR, no blockers, no API endpoints

- [ ] Seed a feature issue with all required DOR sections and `## Blocked By: - None`.
- [ ] Run `/implement #<n> --orchestrator`.
- [ ] Verify Step 2 (pre-check hook) prints `gate log dir: /tmp/maestro-<n>-<ts>` and exits cleanly.
- [ ] Verify Step 4 (gatekeeper) emits a fenced `json gatekeeper` block and the parser extracts `"status": "PASS"`.
- [ ] Verify Step 5 creates branch `feat/issue-<n>-<slug>`.
- [ ] Verify architect + QA are invoked and produce a blueprint + test blueprint.
- [ ] Write the failing tests from the QA blueprint (Step 6d).
- [ ] Verify RED gate (Step 6e) fires and exits 0 — meaning `cargo test` returned non-zero because the new test fails. (If `cargo test` returned 0, the gate exits 3 with "RED GATE FAILED — cargo test passed, but implementation has not started.")
- [ ] Write the minimum implementation (Step 6f).
- [ ] Verify GREEN gate (Step 6g) fires and passes after implementation — `cargo test` returns 0, gate exits 0.
- [ ] Verify security-analyst + docs-analyst are invoked at the end.
- [ ] Verify Step 7 prints the handoff with log paths.

### 2. DOR auto-remediation — missing Acceptance Criteria

- [ ] Seed an issue missing `## Acceptance Criteria`.
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 4 aborts with exit 5.
- [ ] Verify a comment was posted on the issue listing the missing sections.
- [ ] Verify the `needs-info` label was applied.

### 3. Blocker enforcement — open blocker

- [ ] Seed an issue with `## Blocked By: - #<blocker>` where `<blocker>` is an OPEN issue.
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 4 aborts with exit 5 and lists the open blocker in stderr.

### 4. Closed issue — hard stop

- [ ] Pick a CLOSED issue.
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 2 (hook) aborts with exit 1 and "Issue #<n> is CLOSED".

### 5. Dirty tree — stash flow

- [ ] Create an uncommitted change.
- [ ] Run `/implement #<n>`.
- [ ] At the prompt, type `S`.
- [ ] Verify the hook stashes the change with message "auto-stash before /implement #<n>".
- [ ] Verify `git stash list` shows the entry.

### 6. Baseline-green enforcement

- [ ] Introduce a failing test on `main` (e.g., temporarily break one test).
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 2 aborts with exit 2 "BASELINE NOT GREEN".
- [ ] Restore the baseline test.

### 7. Idempotency — Continue on existing branch

- [ ] From the completed run of #1 above, do NOT delete the branch.
- [ ] Run `/implement #<n>` again.
- [ ] Verify the (C)/(R)/(A) prompt appears with the last 5 commits shown.
- [ ] Choose `C`.
- [ ] Verify the architect/QA receive the resumption context and return minimal responses.

### 8. Idempotency — Restart flow

- [ ] Re-run `/implement #<n>` on the same branch.
- [ ] Choose `R`.
- [ ] At the typed-confirmation prompt, type `RESTART`.
- [ ] Verify the branch is deleted and a fresh branch is created.

### 9. RED gate — skipped for `docs` task type

- [ ] Seed a docs-only issue with `type:docs` label.
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 4 classifies as `task_type: docs`.
- [ ] Verify Step 6e (RED gate) is skipped.
- [ ] Verify Step 6g (GREEN gate) is skipped.

### 10. GREEN gate — fires on implementation failure

- [ ] Deliberately write implementation code that breaks a test.
- [ ] Verify Step 6g aborts with exit 4.

---

## Regression notes

Record any deviations observed during the checklist. File as GitHub issues tagged `bug` + `area:harness` if they block a release; `enhancement` otherwise.
```

- [ ] **Step 2: Commit**

```bash
git add docs/harness-acceptance.md
git commit -m "docs(harness): add manual acceptance checklist for /implement"
```

---

### Task 5.2: Run the acceptance checklist

- [ ] **Step 1: Execute all 10 scenarios**

Walk the full `docs/harness-acceptance.md` checklist against real scratch issues. Check each box.

- [ ] **Step 2: Fix regressions if found**

If any scenario fails, file a follow-up issue and either:
- Fix in this PR (small regression) with an additional commit.
- Fix in a follow-up PR (significant gap).

- [ ] **Step 3: Record completion**

Either note the run in the PR description below, or if no regressions, proceed to the PR.

---

### Task 5.3: Chunk-close — final PR

- [ ] **Step 1: Push and open PR**

```bash
git push
gh pr create --title "docs(harness): add acceptance checklist for /implement" --body "$(cat <<'EOF'
## Summary

- Adds `docs/harness-acceptance.md`: manual end-to-end checklist covering 10 scenarios (happy path, DOR remediation, blocker, closed issue, dirty tree, baseline enforcement, idempotency C/R paths, task_type skip rules, GREEN gate failure).
- Completes the Layer-4 testing strategy from the spec. Run this checklist before any release that modifies the harness files.

Chunk 5 (final) of the `/implement` harness enforcement rollout.

## Test plan

- [x] Walked all 10 scenarios against scratch issues
- [x] No unexpected regressions
- [x] Spec's Layer-4 testing strategy is now fully implemented
EOF
)"
```

---

## Chunk 5 — Recap

By the end:

- `docs/harness-acceptance.md` provides a 10-minute E2E walk for every harness release.
- A full run of the checklist has validated the end-to-end flow against real issues.
- The `/implement` harness enforcement project is complete.

Total commits: 2-3 (checklist + any regression fixes) + 1 PR.

---

## Project Recap

Across the 5 chunks / 5 PRs:

- **Chunk 1** delivered the stdlib-only JSON report parser (`parse_gatekeeper_report.py`) + 8 unit tests.
- **Chunk 2** delivered the `subagent-gatekeeper` consultive subagent + 10 fixtures + conformance runner + CLAUDE.md registry update.
- **Chunk 3** delivered the pre-check shell hook (`implement-gates.sh`) + 12 bats tests + PATH-shim fixtures.
- **Chunk 4** delivered the rewritten `implement.md` + CLAUDE.md workflow updates.
- **Chunk 5** delivered the acceptance checklist + end-to-end validation.

Every gate is either a shell exit code or a subagent invocation returning structured output. No gate relies on the model "remembering to check."

Total production code: ~100 lines (parser) + ~100 lines (hook) + ~200 lines (rewritten command) = ~400 lines.
Total test code: ~200 lines (unit) + ~250 lines (bats) + ~50 lines (conformance runner) = ~500 lines.
Total prose: ~150 lines (subagent) + ~100 lines (acceptance checklist) = ~250 lines.

Ratio of test to production code: ~1.25x. Right range for infrastructure code with both happy-path and error-path gates.

