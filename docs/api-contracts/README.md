# docs/api-contracts/

JSON Schema files (Draft 2020-12) for every external data payload that crosses a process boundary — GitHub API responses, Claude slash-command output blocks, and any other structured JSON maestro parses.

## Convention

- One file per payload type, named `<payload-slug>.json`.
- All schemas use `"additionalProperties": false` at the top level.
- Referenced by the `/validate-contracts` slash command and by `subagent-gatekeeper` during the DOR check.
- Any new `serde` struct that deserializes an external payload requires a corresponding schema here before implementation can start (see `docs/RUST-GUARDRAILS.md` §6).

## Schemas

| File | Payload | Added |
|------|---------|-------|
| `review-comment.json` | Structured JSON block embedded in `/review` slash-command PR comments; parsed by the TUI to render the concerns panel and drive the accept/reject flow | #327 |
