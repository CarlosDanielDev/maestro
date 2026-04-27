#!/usr/bin/env bash
# Verify the .md deliverables for issue #485:
#   - .claude/commands/triage-idea.md exists with required structure
#   - .claude/CLAUDE.md registers subagent-idea-triager in two tables
#
# Runs as the TDD RED/GREEN gate for a docs-only task. Exits non-zero on
# the first failed assertion with a description on stderr.

set -euo pipefail

# Pin to repo root so relative paths work from any cwd.
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

CMD=".claude/commands/triage-idea.md"
CLAUDE_MD=".claude/CLAUDE.md"
AGENT=".claude/agents/subagent-idea-triager.md"
PARSER=".claude/hooks/parse_idea_triager_report.py"

fail() {
  echo "verify-issue-485: FAIL — $1" >&2
  exit 1
}

# Extract the body of a `### Heading` block from CLAUDE.md, stopping at
# the first line matching the stop pattern. Returns lines AFTER the
# start heading (not the heading itself).
claude_md_section() {
  local start="$1" stop="$2"
  awk -v start="$start" -v stop="$stop" '
    $0 ~ stop && in_block { exit }
    in_block { print }
    $0 ~ start { in_block = 1 }
  ' "$CLAUDE_MD"
}

[ -f "$CMD" ] || fail "F1: $CMD does not exist"
[ -s "$CMD" ] || fail "F2: $CMD is empty"

grep -qE '^# Triage Idea[[:space:]]*$' "$CMD" \
  || fail "F3: H1 '# Triage Idea' missing in $CMD"
grep -q '^## Arguments$' "$CMD" \
  || fail "F4: '## Arguments' section missing in $CMD"
grep -q '^## Instructions$' "$CMD" \
  || fail "F5: '## Instructions' section missing in $CMD"
grep -q '^## Exit Codes$' "$CMD" \
  || fail "F6: '## Exit Codes' section missing in $CMD"
grep -q '^## Do Not$' "$CMD" \
  || fail "F7: '## Do Not' section missing in $CMD"

# F8-F13. Step 0 through Step 5 sub-sections.
for n in 0 1 2 3 4 5; do
  grep -q "### Step ${n}:" "$CMD" \
    || fail "F$((8 + n)): '### Step ${n}:' missing in $CMD"
done

# `set -e` would abort on grep -c returning 1 (zero matches), so guard with `|| true`.
hits=$(grep -c 'subagent-idea-triager' "$CMD" || true)
[ "$hits" -ge 2 ] \
  || fail "F14: 'subagent-idea-triager' appears $hits time(s) in $CMD (need >= 2)"

grep -q '\.claude/hooks/parse_idea_triager_report\.py' "$CMD" \
  || fail "F15: parser path '.claude/hooks/parse_idea_triager_report.py' missing in $CMD"
grep -Eiq '(does not mutate|never auto-mutate|non-mutation|do not mutate)' "$CMD" \
  || fail "F16: non-mutation guarantee phrasing missing in $CMD"
grep -qiE '(confirm|yes.*no|proceed\?|approve|y/n)' "$CMD" \
  || fail "F17: confirmation prompt phrasing missing in $CMD"

# F18-F21. Plans 2-5 explicitly deferred.
for n in 2 3 4 5; do
  grep -q "Plan $n" "$CMD" \
    || fail "F$((16 + n)): 'Plan $n' deferral missing in $CMD"
done

# F22-F23. Delegation Rules table contains the row + description.
delegation_block=$(claude_md_section \
  '^### Subagent Reference [(]Orchestrator Mode[)]' \
  '^### Subagent Registry')
[ -n "$delegation_block" ] \
  || fail "F22: Delegation Rules section heading not found — table layout in $CLAUDE_MD may have drifted"
echo "$delegation_block" | grep -q 'subagent-idea-triager' \
  || fail "F22: 'subagent-idea-triager' missing from Delegation Rules table in $CLAUDE_MD"
echo "$delegation_block" | grep -q 'Idea triage (pre-DOR funnel)' \
  || fail "F23: 'Idea triage (pre-DOR funnel)' description missing from Delegation Rules table"

# F24-F25. Subagent Registry table contains the row with a Ready/Draft status.
registry_block=$(claude_md_section \
  '^### Subagent Registry' \
  '^(---|## )')
[ -n "$registry_block" ] \
  || fail "F24: Subagent Registry section heading not found — table layout in $CLAUDE_MD may have drifted"
echo "$registry_block" | grep -q 'subagent-idea-triager' \
  || fail "F24: 'subagent-idea-triager' missing from Subagent Registry table in $CLAUDE_MD"
echo "$registry_block" \
  | grep 'subagent-idea-triager' \
  | grep -qE '\*\*(Ready|Draft)\*\*' \
  || fail "F25: registry row for 'subagent-idea-triager' missing **Ready** or **Draft** status"

[ -f "$AGENT" ] || fail "F26: $AGENT missing (blocker #483 should have shipped this)"
[ -x "$PARSER" ] || fail "F27: $PARSER missing or not executable (blocker #484)"

echo "verify-issue-485: PASS — all 27 assertions ok"
