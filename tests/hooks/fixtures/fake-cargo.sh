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
