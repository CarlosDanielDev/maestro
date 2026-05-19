#!/bin/bash
# Re-emit `docs/configuration.md` autogen blocks from the config schema.
#
# Drives the ignored test `integration_tests::docs_gen::docs_gen_regenerate`,
# which walks `schema_for_config()` and rewrites every
# `<!-- BEGIN AUTOGEN:NAME --> ... <!-- END AUTOGEN:NAME -->` block in place.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "Regenerating docs/configuration.md from src/config/schema/…"
cargo test --quiet docs_gen_regenerate --bin maestro -- --ignored --nocapture
echo
echo "Done. Stage the diff if non-empty:"
git diff --stat docs/configuration.md
