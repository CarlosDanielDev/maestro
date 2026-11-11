#!/usr/bin/env bash
# Tests for implement-gates.sh — argument validation and gate-dir hygiene (#1041).
# Run with: bash .maestro/hooks/tests/test-implement-gates.sh
#
# Covers the two #1041 defects:
#   A. $1 must be digits-only before it reaches gh, file paths, or python.
#   B. $GATE_LOG_DIR must be created with mktemp -d under $SENTINEL_DIR
#      (mode 0700, unpredictable name, never through a pre-existing path)
#      and never silently fall back to /tmp.
#
# gh and cargo are stubbed so the hook runs end-to-end without network or a
# real toolchain.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../implement-gates.sh"

if [ ! -f "$HOOK" ]; then
  echo "FAIL: $HOOK does not exist"
  exit 1
fi

pass=0
fail=0

check() {
  local name="$1"
  local ok="$2"
  if [ "$ok" = "1" ]; then
    echo "PASS: $name"
    pass=$((pass + 1))
  else
    echo "FAIL: $name"
    fail=$((fail + 1))
  fi
}

# Issue JSON fixture shaped like the real gh payload (state + DOR sections for
# condense-issue.sh).
make_issue_json() {
  python3 - "$1" <<'PY'
import json
import sys

body = "\n\n".join([
    "## Overview\n\nx",
    "## Expected Behavior\n\ny",
    "## Acceptance Criteria\n\nz",
    "## Blocked By\n\nNone",
    "## Definition of Done\n\nw",
])
issue = {"title": "t", "state": "OPEN", "body": body}
with open(sys.argv[1], "w") as fh:
    fh.write(json.dumps(issue))
PY
}

# Stub gh (authenticated, issue fetch) and cargo (green baseline).
make_stub_bin() {
  local dir="$1"
  local issue_json="$2"
  mkdir -p "$dir"
  cat > "$dir/gh" <<STUB
#!/usr/bin/env bash
if [ "\$1" = "auth" ]; then exit 0; fi
if [ "\$1" = "issue" ]; then cat "$issue_json"; exit 0; fi
exit 1
STUB
  printf '#!/usr/bin/env bash\nexit 0\n' > "$dir/cargo"
  chmod +x "$dir/gh" "$dir/cargo"
}

# Throwaway repo shaped for the hook: local main with CHANGELOG.md committed
# and a clean tree, so Gate 6 skips and Gate 6.5 finds no CHANGELOG drift.
make_repo() {
  local dir
  dir=$(mktemp -d)
  (
    cd "$dir"
    git init -q -b main
    git config user.email "test@example.com"
    git config user.name "test"
    printf "# Changelog\n\n## [Unreleased]\n" > CHANGELOG.md
    git add . >/dev/null
    git commit -q -m "base"
  )
  echo "$dir"
}

stat_mode() {
  # 0700 on both macOS (BSD stat) and Linux (GNU stat).
  stat -f %Lp "$1" 2>/dev/null || stat -c %a "$1"
}

# ---------------------------------------------------------------------------
# A. Issue-number validation runs before anything else (even before the git
#    repo check, so these run from a plain temp dir).
# ---------------------------------------------------------------------------

# A1: shell-injection payload must be rejected with the digits-only message
# and must not execute (no marker file).
adir=$(mktemp -d)
marker="$adir/marker-1041"
payload="1'); import os; os.system('touch $marker'); ('"
out=$(cd "$adir" && bash "$HOOK" "$payload" 2>&1) && rc=0 || rc=$?
check "injection_payload_rejected_rc1" "$([ "$rc" -eq 1 ] && echo 1 || echo 0)"
case "$out" in
  *digits-only*) ok=1 ;; *) ok=0 ;;
esac
check "injection_payload_digits_message" "$ok"
check "injection_no_marker_file" "$([ ! -e "$marker" ] && echo 1 || echo 0)"
rm -rf "$adir"

# A2: plain non-numeric value must be rejected the same way.
adir=$(mktemp -d)
out=$(cd "$adir" && bash "$HOOK" "999abc" 2>&1) && rc=0 || rc=$?
check "nonnumeric_rejected_rc1" "$([ "$rc" -eq 1 ] && echo 1 || echo 0)"
case "$out" in
  *digits-only*) ok=1 ;; *) ok=0 ;;
esac
check "nonnumeric_digits_message" "$ok"
rm -rf "$adir"

# A3: empty issue number keeps the existing required-argument message.
adir=$(mktemp -d)
out=$(cd "$adir" && bash "$HOOK" "" 2>&1) && rc=0 || rc=$?
check "empty_arg_rejected_rc1" "$([ "$rc" -eq 1 ] && echo 1 || echo 0)"
case "$out" in
  *"issue number required"*) ok=1 ;; *) ok=0 ;;
esac
check "empty_arg_required_message" "$ok"
rm -rf "$adir"

# ---------------------------------------------------------------------------
# B. Gate-dir hygiene, with the hook stubbed green end-to-end.
# ---------------------------------------------------------------------------

# B1: happy path — dir under $SENTINEL_DIR, unpredictable suffix, mode 0700,
# issue.json written there, sentinel file carries the same path.
repo=$(make_repo)
xdg=$(mktemp -d)
home=$(mktemp -d)
stub=$(mktemp -d)
make_issue_json "$stub/issue.json"
make_stub_bin "$stub" "$stub/issue.json"
out=$(cd "$repo" && env HOME="$home" XDG_RUNTIME_DIR="$xdg" PATH="$stub:$PATH" \
  bash "$HOOK" 999 2>&1) && rc=0 || rc=$?
check "happy_path_exit_0" "$([ "$rc" -eq 0 ] && echo 1 || echo 0)"
gate_dir=$(printf '%s\n' "$out" | sed -n 's/^gate log dir: //p' | head -1)
check "happy_path_prints_gate_dir" "$([ -n "$gate_dir" ] && echo 1 || echo 0)"
case "$gate_dir" in
  "$xdg"/maestro-999-*) ok=1 ;; *) ok=0 ;;
esac
check "gate_dir_under_sentinel_dir" "$ok"
base=$(basename "$gate_dir")
case "$base" in
  maestro-999-[A-Za-z0-9][A-Za-z0-9][A-Za-z0-9][A-Za-z0-9][A-Za-z0-9][A-Za-z0-9]) ok=1 ;; *) ok=0 ;;
esac
check "gate_dir_unpredictable_suffix" "$ok"
check "gate_dir_mode_0700" "$([ "$(stat_mode "$gate_dir")" = "700" ] && echo 1 || echo 0)"
check "issue_json_in_gate_dir" "$([ -s "$gate_dir/issue.json" ] && echo 1 || echo 0)"
state=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["state"])' \
  "$gate_dir/issue.json" 2>/dev/null)
check "issue_state_readable" "$([ "$state" = "OPEN" ] && echo 1 || echo 0)"
check "sentinel_written_with_gate_dir" \
  "$([ "$(cat "$xdg/maestro-current-gate-dir" 2>/dev/null)" = "$gate_dir" ] && echo 1 || echo 0)"
rm -rf "$repo" "$xdg" "$home" "$stub" "$gate_dir"

# B2: a pre-created colliding entry in $SENTINEL_DIR is bypassed — the hook's
# dir is a different, real directory and nothing is written through the link.
repo=$(make_repo)
xdg=$(mktemp -d)
home=$(mktemp -d)
stub=$(mktemp -d)
make_issue_json "$stub/issue.json"
make_stub_bin "$stub" "$stub/issue.json"
attacker=$(mktemp -d)
victim="$attacker/vault"
mkdir -p "$victim"
trap_link="$xdg/maestro-999-AAAAAA"
ln -s "$victim" "$trap_link"
out=$(cd "$repo" && env HOME="$home" XDG_RUNTIME_DIR="$xdg" PATH="$stub:$PATH" \
  bash "$HOOK" 999 2>&1) && rc=0 || rc=$?
gate_dir=$(printf '%s\n' "$out" | sed -n 's/^gate log dir: //p' | head -1)
check "symlink_case_exit_0" "$([ "$rc" -eq 0 ] && echo 1 || echo 0)"
check "symlink_case_gate_dir_differs" \
  "$([ -n "$gate_dir" ] && [ "$gate_dir" != "$trap_link" ] && echo 1 || echo 0)"
check "precreated_link_untouched" "$([ -L "$trap_link" ] && echo 1 || echo 0)"
check "nothing_written_through_link" "$([ -z "$(ls -A "$victim")" ] && echo 1 || echo 0)"
rm -rf "$repo" "$xdg" "$home" "$stub" "$attacker" "$gate_dir"

# B3: when the gate dir cannot be created (read-only $SENTINEL_DIR), the hook
# must fail with a clear error and never fall back to /tmp.
repo=$(make_repo)
xdg=$(mktemp -d)
home=$(mktemp -d)
stub=$(mktemp -d)
make_issue_json "$stub/issue.json"
make_stub_bin "$stub" "$stub/issue.json"
chmod 500 "$xdg"
out=$(cd "$repo" && env HOME="$home" XDG_RUNTIME_DIR="$xdg" PATH="$stub:$PATH" \
  bash "$HOOK" 999 2>&1) && rc=0 || rc=$?
chmod 700 "$xdg"
check "readonly_sentinel_dir_rc1" "$([ "$rc" -eq 1 ] && echo 1 || echo 0)"
case "$out" in
  *"failed to create gate"*) ok=1 ;; *) ok=0 ;;
esac
check "readonly_sentinel_dir_clear_error" "$ok"
case "$out" in
  *"gate log dir: /tmp"*) ok=0 ;; *) ok=1 ;;
esac
check "no_silent_tmp_fallback" "$ok"
check "readonly_sentinel_no_sentinel_write" \
  "$([ ! -e "$xdg/maestro-current-gate-dir" ] && echo 1 || echo 0)"
rm -rf "$repo" "$xdg" "$home" "$stub"

echo ""
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
