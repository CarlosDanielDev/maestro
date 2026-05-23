#!/usr/bin/env bash
#
# Programmatic smoke for #845 — MiniMax --force-quota end-to-end.
#
# Seeds ~/.maestro/minimax-quota.json at 95% with schema_version=1 (so the
# v1->v2 read shim is exercised too), runs `maestro run --agent minimax
# --force-quota --once "<prompt>"`, then asserts the on-disk state:
#
#   - schema_version == 2     (migration ran)
#   - forced_count   == 1     (record_forced bookkeeping)
#   - requests       == 4276  (4275 seeded + 1 forced spawn)
#
# Backs up any existing quota file and restores it on exit (even on failure).
#
# Requirements:
#   - jq and python3 on PATH
#   - MINIMAX_API_KEY exported with a real key
#   - maestro.toml in repo root declares [agents.minimax]
#   - TTY for the maestro TUI; this is NOT runnable from cron / CI
#
# Usage:
#   export MINIMAX_API_KEY=sk-...
#   bash scripts/smoke-minimax-force-quota.sh           # default prompt "ping"
#   bash scripts/smoke-minimax-force-quota.sh "my prompt"

set -euo pipefail

PROMPT="${1:-ping}"
QUOTA_PATH="${HOME}/.maestro/minimax-quota.json"
BACKUP_PATH="${QUOTA_PATH}.smoke-backup"

fail() { echo "FAIL: $*" >&2; exit 1; }

# --- 0. Prerequisites -------------------------------------------------------

command -v jq      >/dev/null || fail "jq required (brew install jq)"
command -v python3 >/dev/null || fail "python3 required"

[[ -n "${MINIMAX_API_KEY:-}" ]] \
    || fail "export MINIMAX_API_KEY=sk-... before running"

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" \
    || fail "must run from inside the maestro repo"
cd "$REPO_ROOT"

grep -q '^\[agents\.minimax\]' maestro.toml \
    || fail "no [agents.minimax] block in maestro.toml"

# --- 1. Backup existing quota file (and restore on exit) -------------------

mkdir -p "$(dirname "$QUOTA_PATH")"
if [[ -f "$QUOTA_PATH" ]]; then
  cp "$QUOTA_PATH" "$BACKUP_PATH"
  trap 'mv "$BACKUP_PATH" "$QUOTA_PATH" 2>/dev/null || true' EXIT
else
  trap 'rm -f "$QUOTA_PATH"' EXIT
fi

# --- 2. Seed at 95% (4275 / 4500) with schema_version = 1 ------------------

python3 - "$QUOTA_PATH" <<'PY'
import json, sys, datetime
path = sys.argv[1]
now = datetime.datetime.now(datetime.timezone.utc)
# 4275 requests evenly distributed across the last ~3.5h to stay well inside
# the 5h sliding window. Format matches chrono RFC3339 UTC ("Z" suffix).
requests = [
    (now - datetime.timedelta(seconds=i * 3))
        .isoformat()
        .replace("+00:00", "Z")
    for i in range(4275)
]
with open(path, "w") as f:
    json.dump({"schema_version": 1, "requests": requests}, f)
PY

echo "Seeded $QUOTA_PATH at 95% (schema_version=1, will migrate to v2):"
jq '{schema_version, count: (.requests | length), forced_count: (.forced_count // null)}' \
    "$QUOTA_PATH"

# --- 3. Build (release, quiet) + run with --force-quota --once ------------

echo
echo "Building maestro (release)..."
cargo build --release --quiet

echo
echo "Running: maestro run --agent minimax --force-quota --once \"$PROMPT\""
echo "(TUI launches; will exit automatically when the session completes.)"
echo "----------------------------------------------------------------------"

./target/release/maestro run \
    --agent minimax \
    --force-quota \
    --once \
    "$PROMPT" \
    || fail "maestro exited non-zero"

# --- 4. Assert on-disk state ----------------------------------------------

echo
echo "Post-run quota state:"
jq '{schema_version, count: (.requests | length), forced_count}' "$QUOTA_PATH"

SCHEMA=$(jq -r '.schema_version' "$QUOTA_PATH")
FORCED=$(jq -r '.forced_count'   "$QUOTA_PATH")
COUNT=$( jq -r '.requests | length' "$QUOTA_PATH")

[[ "$SCHEMA" == "2"    ]] || fail "schema_version=$SCHEMA, expected 2 (v1->v2 migration)"
[[ "$FORCED" == "1"    ]] || fail "forced_count=$FORCED, expected 1 after one forced spawn"
[[ "$COUNT"  == "4276" ]] || fail "requests=$COUNT, expected 4276 (4275 seeded + 1 spawn)"

echo
echo "PASS: schema migrated v1->v2, forced_count=1, requests=4276."
echo "      The 'QUOTA: forced 1 in window' badge should also have"
echo "      flashed in the home-screen stats bar during the TUI session."
