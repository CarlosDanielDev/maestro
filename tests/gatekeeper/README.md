# Gatekeeper Conformance

This directory holds fixtures and an expected-subset matcher for the
`subagent-gatekeeper` consultive subagent.

## Running conformance

The runner (`run-conformance.sh`) expects a helper named
`invoke-gatekeeper-subagent` on `PATH` that takes a fixture JSON on
stdin and emits the subagent's raw response on stdout. Options:

1. **Interactive (v1 default):** Invoke the subagent manually in Claude
   Code for each fixture, copy-paste the response into
   `/tmp/subagent-response-<name>.txt`, and run the parser + matcher by
   hand.
2. **Scripted (v2):** Wire the Agent invocation into a small Python
   helper using the Claude Agent SDK. Deferred — see the spec's Open
   Questions section.

## Fixture conventions

- `<name>.json` — the issue JSON the subagent receives.
- `<name>.gh-mock.json` — optional mock for blocker lookups. When
  present, the conformance runner sets `GATEKEEPER_GH_MOCK=<path>` so
  the subagent's `gh` invocations can be shimmed.
- `<name>.expected.json` under `expected/` — the structural subset the
  parsed report must match.

## Adding a fixture

1. Draft a minimal GitHub issue JSON under `fixtures/`.
2. If the issue references blockers, add a `<name>.gh-mock.json` with
   canned `state`/`title` for each blocker number.
3. Hand-derive the expected report and save under `expected/`.
4. Run the conformance runner; iterate until the subagent's output
   matches.
