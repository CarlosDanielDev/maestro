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
