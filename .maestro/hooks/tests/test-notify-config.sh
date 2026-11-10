#!/usr/bin/env bash
# Tests for notify.sh config loading — the notifications.conf file must never
# be sourced as bash. Only allowlisted KEY=VALUE lines are parsed; anything
# else (commands, exports, quoted values, unknown keys) is ignored.
# Run with: bash .maestro/hooks/tests/test-notify-config.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NOTIFY="$SCRIPT_DIR/../notify.sh"

if [ ! -f "$NOTIFY" ]; then
  echo "FAIL: $NOTIFY does not exist"
  exit 1
fi

pass=0
fail=0

assert_eq() {
  local name="$1"
  local actual="$2"
  local expected="$3"
  if [ "$actual" = "$expected" ]; then
    echo "PASS: $name"
    pass=$((pass + 1))
  else
    echo "FAIL: $name — expected '$expected', got '$actual'"
    fail=$((fail + 1))
  fi
}

assert_absent() {
  local name="$1"
  local path="$2"
  if [ ! -e "$path" ]; then
    echo "PASS: $name"
    pass=$((pass + 1))
  else
    echo "FAIL: $name — '$path' exists (config file was executed)"
    fail=$((fail + 1))
  fi
}

# Copy the hook into an isolated tree so load_config resolves CONFIG_FILE
# inside the sandbox, never the real repo's notifications.conf.
make_sandbox() {
  local root
  root=$(mktemp -d)
  mkdir -p "$root/.maestro/hooks" "$root/home"
  cp "$NOTIFY" "$root/.maestro/hooks/notify.sh"
  echo "$root"
}

# Source the sandboxed copy and print the five config values as
# prefix=value lines (prefixes survive empty values; command substitution
# would strip a trailing empty line).
run_load_config() {
  local root="$1"
  env -i HOME="$root/home" bash -c "
    source '$root/.maestro/hooks/notify.sh'
    load_config
    printf 'desktop=%s\nslack=%s\nperm=%s\nidle=%s\nuser=%s\n' \"\$NOTIFY_DESKTOP\" \"\$NOTIFY_SLACK\" \"\$NOTIFY_PERMISSION_PROMPT\" \"\$NOTIFY_IDLE_PROMPT\" \"\$SLACK_USER_ID\"
  "
}

# Parse prefix=value lines into v_desktop/v_slack/v_perm/v_idle/v_user.
parse_values() {
  v_desktop=""
  v_slack=""
  v_perm=""
  v_idle=""
  v_user=""
  local line
  while IFS= read -r line; do
    case "$line" in
      desktop=*) v_desktop="${line#desktop=}" ;;
      slack=*)   v_slack="${line#slack=}" ;;
      perm=*)    v_perm="${line#perm=}" ;;
      idle=*)    v_idle="${line#idle=}" ;;
      user=*)    v_user="${line#user=}" ;;
    esac
  done <<VALUES
$1
VALUES
}

# Case 1: a conf with an embedded command must not execute it; the valid
# key on the first line must still parse.
root=$(make_sandbox)
printf 'NOTIFY_SLACK=false\ntouch "%s/pwned"\n' "$root" > "$root/.maestro/notifications.conf"
parse_values "$(run_load_config "$root")"
assert_eq "malicious_conf_still_parses_notify_slack" "$v_slack" "false"
assert_absent "malicious_conf_command_not_executed" "$root/pwned"
rm -rf "$root"

# Case 2: all five allowlisted keys parse with shape-checked values.
root=$(make_sandbox)
cat > "$root/.maestro/notifications.conf" <<'EOF'
NOTIFY_DESKTOP=false
NOTIFY_SLACK=true
NOTIFY_PERMISSION_PROMPT=false
NOTIFY_IDLE_PROMPT=false
SLACK_USER_ID=user.example@U0123-456
EOF
parse_values "$(run_load_config "$root")"
assert_eq "parses_notify_desktop" "$v_desktop" "false"
assert_eq "parses_notify_slack" "$v_slack" "true"
assert_eq "parses_notify_permission_prompt" "$v_perm" "false"
assert_eq "parses_notify_idle_prompt" "$v_idle" "false"
assert_eq "parses_slack_user_id" "$v_user" "user.example@U0123-456"
rm -rf "$root"

# Case 3: lines outside the allowlist (bad value shape, quoted value, export,
# unknown key, comments, blank lines) are ignored and defaults survive.
root=$(make_sandbox)
cat > "$root/.maestro/notifications.conf" <<'EOF'
# comment
NOTIFY_SLACK=true; curl http://evil.example
NOTIFY_DESKTOP="true; curl http://evil.example"
export NOTIFY_IDLE_PROMPT=false

SOMEOTHER=x
EOF
parse_values "$(run_load_config "$root")"
assert_eq "malformed_value_ignored_keeps_default" "$v_slack" "true"
assert_eq "quoted_value_ignored_keeps_default" "$v_desktop" "true"
assert_eq "export_line_ignored_keeps_default" "$v_idle" "true"
rm -rf "$root"

# Case 4: running the hook itself with a hostile conf and empty stdin exits
# cleanly without executing the command.
root=$(make_sandbox)
printf 'NOTIFY_DESKTOP=false\nNOTIFY_SLACK=false\ntouch "%s/pwned"\n' "$root" > "$root/.maestro/notifications.conf"
status=0
env -i HOME="$root/home" bash "$root/.maestro/hooks/notify.sh" < /dev/null > /dev/null 2>&1 || status=$?
assert_eq "hook_run_with_hostile_conf_exits_zero" "$status" "0"
assert_absent "hook_run_with_hostile_conf_not_executed" "$root/pwned"
rm -rf "$root"

echo ""
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
