#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  RELEASE_SCRIPT="$REPO_ROOT/scripts/release.sh"

  FAKE_BIN="$(mktemp -d)"
  GH_STATE_DIR="$(mktemp -d)"
  export FAKE_BIN GH_STATE_DIR RELEASE_SCRIPT

  PATH="$FAKE_BIN:$PATH"
  create_gh_stub
}

teardown() {
  rm -rf "$FAKE_BIN" "$GH_STATE_DIR"
}

create_gh_stub() {
  cat > "$FAKE_BIN/gh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail

cmd="$1"; shift
case "$cmd" in
  pr)
    sub="$1"; shift
    case "$sub" in
      checks)
        counter_file="$GH_STATE_DIR/checks.count"
        count=$(cat "$counter_file" 2>/dev/null || echo 0)
        count=$((count + 1))
        echo "$count" > "$counter_file"
        json=$(sed -n "${count}p" "$GH_STATE_DIR/checks.json" 2>/dev/null || true)
        [[ -z "$json" ]] && json="[]"
        echo "$json"
        ;;
      merge)
        echo "merge $*" >> "$GH_STATE_DIR/merge.log"
        if echo "$*" | grep -q -- '--auto'; then
          exit "${GH_AUTO_RC:-0}"
        else
          [[ -n "${GH_POLICY_MSG:-}" ]] && echo "$GH_POLICY_MSG" >&2
          exit "${GH_MERGE_RC:-0}"
        fi
        ;;
      *)
        echo "unsupported: pr $sub" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "unsupported: $cmd" >&2
    exit 1
    ;;
  esac
STUB
  chmod +x "$FAKE_BIN/gh"
}

@test "waits for checks to appear before merging" {
  cat > "$GH_STATE_DIR/checks.json" <<'JSON'
[]
[]
[{"name":"ci","state":"pass"}]
JSON

  run env \
    MAESTRO_RELEASE_LIB_ONLY=1 \
    GH_STATE_DIR="$GH_STATE_DIR" \
    GH_MERGE_RC=0 GH_AUTO_RC=0 \
    PATH="$FAKE_BIN:$PATH" \
    PR_POLL_INTERVAL=0 RELEASE_SCRIPT="$RELEASE_SCRIPT" \
    bash -c 'source "$RELEASE_SCRIPT"; watch_and_merge_pr 42 "https://example.com/pr/42"'

  [ "$status" -eq 0 ]
  [ "$(cat "$GH_STATE_DIR/checks.count")" -eq 3 ]
  [[ "$output" != *"All 0 CI checks passed"* ]]
  [[ "$output" == *"All 1 CI checks passed on PR #42"* ]]
  grep -q -- "--merge" "$GH_STATE_DIR/merge.log"
}

@test "falls back to auto-merge when policy blocks immediate merge" {
  cat > "$GH_STATE_DIR/checks.json" <<'JSON'
[{"name":"ci","state":"pass"}]
JSON

  run env \
    MAESTRO_RELEASE_LIB_ONLY=1 \
    GH_STATE_DIR="$GH_STATE_DIR" \
    GH_MERGE_RC=1 GH_AUTO_RC=0 GH_POLICY_MSG="base branch policy prohibits the merge" \
    PATH="$FAKE_BIN:$PATH" \
    PR_POLL_INTERVAL=0 RELEASE_SCRIPT="$RELEASE_SCRIPT" \
    bash -c 'source "$RELEASE_SCRIPT"; watch_and_merge_pr 7 "https://example.com/pr/7"'

  [ "$status" -eq 0 ]
  [[ "$output" == *"Auto-merge enabled for PR #7"* ]]
  grep -q -- "--merge" "$GH_STATE_DIR/merge.log"
  grep -q -- "--auto" "$GH_STATE_DIR/merge.log"
}
