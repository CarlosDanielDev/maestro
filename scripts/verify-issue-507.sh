#!/usr/bin/env bash
# Verify the .github/workflows/release.yml deliverables for issue #507:
#   - SC2035 fix on the existing checksums step (sha256sum ./*.tar.gz)
#   - notify-discord job exists with the required structure
#   - Pre-release filter blocks alpha/beta/rc/pre/dev/canary/smoketest
#   - startsWith(github.ref, 'refs/tags/v') guard present (rejects branch refs)
#   - permissions: {} per-job lockdown (defense in depth)
#   - DISCORD_WEBHOOK_URL referenced via secrets context only, never inlined
#     in run: blocks (CWE-78 / shell injection)
#   - No hardcoded discord.com/api/webhooks URL anywhere
#   - dry_run boolean input defaults to true (fail-safe)
#   - curl uses --fail / -f (non-2xx -> non-zero exit, AC #5)
#   - jq used to build payload (no naive JSON interpolation)
#   - Empty-secret guard before curl (clear error vs curl exit 7)
#   - actionlint reports zero findings on the file (AC #7)
#
# Runs as the TDD RED/GREEN gate for a CI-only task. Exits non-zero on
# the first failed assertion with a description on stderr.

set -euo pipefail

# Pin to repo root so relative paths work from any cwd.
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

WORKFLOW=".github/workflows/release.yml"

fail() {
  echo "verify-issue-507: FAIL — $1" >&2
  exit 1
}

# Extract the body of a named top-level job block. Starts after the job
# header line; stops at the next top-level job (line at 2-space indent
# ending in ':') or EOF.
job_block() {
  local job="$1"
  awk -v job="$job" '
    in_block && /^  [a-z][a-z-]*:[[:space:]]*$/ { exit }
    in_block { print }
    $0 ~ "^  " job ":[[:space:]]*$" { in_block = 1 }
  ' "$WORKFLOW"
}

[ -f "$WORKFLOW" ] || fail "F1: $WORKFLOW does not exist"
[ -s "$WORKFLOW" ] || fail "F2: $WORKFLOW is empty"

# F3. SC2035 fix: sha256sum must use ./* glob (AC #7 demands actionlint clean).
grep -q 'sha256sum \./\*\.tar\.gz' "$WORKFLOW" \
  || fail "F3: SC2035 not fixed — 'sha256sum ./*.tar.gz' missing (got bare glob)"

# F4. notify-discord job exists.
grep -qE '^  notify-discord:[[:space:]]*$' "$WORKFLOW" \
  || fail "F4: 'notify-discord:' job missing in $WORKFLOW"

# Extract the notify-discord block once for reuse.
ND_BLOCK=$(job_block "notify-discord")
[ -n "$ND_BLOCK" ] \
  || fail "F4a: notify-discord job block could not be extracted"

# F5. needs: release for upstream success gating (AC #4).
echo "$ND_BLOCK" | grep -q 'needs: release' \
  || fail "F5: 'needs: release' missing in notify-discord block"

# F6. Pre-release filter blocks each required prefix (AC #3).
for prefix in alpha beta rc pre dev canary smoketest; do
  echo "$ND_BLOCK" | grep -qE "contains\(github\.ref, '-${prefix}'\)" \
    || fail "F6.${prefix}: !contains(github.ref, '-${prefix}') missing in notify-discord if:"
done

# F7. startsWith(github.ref, 'refs/tags/v') guard — rejects branch refs from
# workflow_dispatch reaching the curl path with a non-tag context.
echo "$ND_BLOCK" | grep -qE "startsWith\(github\.ref, 'refs/tags/v'\)" \
  || fail "F7: startsWith(github.ref, 'refs/tags/v') guard missing in notify-discord if:"

# F8. permissions: {} per-job lockdown.
echo "$ND_BLOCK" | grep -qE 'permissions:[[:space:]]*\{\}' \
  || fail "F8: 'permissions: {}' missing in notify-discord block"

# F9. DISCORD_WEBHOOK_URL referenced via secrets context (AC #6).
grep -q 'secrets\.DISCORD_WEBHOOK_URL' "$WORKFLOW" \
  || fail "F9: 'secrets.DISCORD_WEBHOOK_URL' reference missing"

# F10. Secret never interpolated into run: shell — only via env: indirection.
# Every ${{ secrets.DISCORD_WEBHOOK_URL }} expression must appear on an
# env-key assignment line (e.g.  '  DISCORD_WEBHOOK_URL: ${{ secrets... }}').
bad_inlines=$(grep -n '\${{[[:space:]]*secrets\.DISCORD_WEBHOOK_URL[[:space:]]*}}' "$WORKFLOW" \
              | grep -vE ':[[:space:]]+[A-Z][A-Z_]*:[[:space:]]*\$\{\{' || true)
[ -z "$bad_inlines" ] \
  || fail "F10: secrets.DISCORD_WEBHOOK_URL interpolated outside env: key (CWE-78). Lines: $bad_inlines"

# F11. No hardcoded Discord webhook URL.
if grep -qE 'https://discord(app)?\.com/api/webhooks/[0-9]+/' "$WORKFLOW"; then
  fail "F11: hardcoded Discord webhook URL detected — must use secrets context only"
fi

# F12. dry_run input is type: boolean.
grep -A4 '^[[:space:]]*dry_run:' "$WORKFLOW" | grep -q 'type: boolean' \
  || fail "F12: dry_run input is not 'type: boolean'"

# F13. dry_run default: true (fail-safe).
grep -A5 '^[[:space:]]*dry_run:' "$WORKFLOW" | grep -q 'default: true' \
  || fail "F13: dry_run default is not 'true' (fail-safe required)"

# F14. curl uses --fail or -f flag (AC #5).
# Match either '--fail' or any short-flag bundle containing 'f' (e.g. -fsSL).
echo "$ND_BLOCK" | grep -qE 'curl[[:space:]][^#|]*(--fail|-[a-zA-Z]*f[a-zA-Z]*)' \
  || fail "F14: curl in notify-discord lacks --fail / -f flag (HTTP errors would not propagate)"

# F15. jq used to build payload (avoids JSON quoting / injection bugs).
echo "$ND_BLOCK" | grep -q 'jq' \
  || fail "F15: 'jq' missing in notify-discord — payload must be built with jq -n --arg"

# F16. Empty-secret guard before curl (clear error vs cryptic curl exit 7).
echo "$ND_BLOCK" | grep -qE '\[[[:space:]]+-n[[:space:]]+"\$\{?DISCORD_WEBHOOK_URL\}?"[[:space:]]+\]|test[[:space:]]+-n[[:space:]]+"\$\{?DISCORD_WEBHOOK_URL\}?"' \
  || fail "F16: empty-secret guard '[ -n \"\${DISCORD_WEBHOOK_URL}\" ]' missing — curl would fail cryptically"

# F17. actionlint clean (AC #7).
if ! command -v actionlint >/dev/null 2>&1; then
  echo "verify-issue-507: SKIP F17 — actionlint not installed (brew install actionlint)" >&2
else
  actionlint "$WORKFLOW" \
    || fail "F17: actionlint reported findings on $WORKFLOW (AC #7 demands clean)"
fi

echo "verify-issue-507: PASS — all 17 assertions ok"
