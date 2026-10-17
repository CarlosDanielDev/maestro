#!/bin/sh
# Stub agent for tests/e2e_run_once.rs. Emits the stream-json lines a Claude-style
# headless run produces, so maestro can complete a session without a real agent.
#
# Success path only: this stub always exits 0 and emits a success result. The app marks
# sessions `completed` even for failing stubs, so do not read status as proof of success.
# The `--version` branch is not exercised by the e2e test, because --skip-doctor skips
# the health check that would call it.
if [ "$1" = "--version" ]; then
  echo "${MAESTRO_STUB_AGENT_VERSION:-stub-agent 1.0.0}"
  exit 0
fi

echo '{"type":"system","subtype":"init","session_id":"e2e00000-0000-0000-0000-000000000001"}'
echo '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"stub reply"}]}}'
echo '{"type":"result","subtype":"success","result":"stub reply","session_id":"e2e00000-0000-0000-0000-000000000001"}'
exit 0
