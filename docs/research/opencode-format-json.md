# OpenCode `--format json` research

Issue: #651
Date observed: 2026-05-06
CLI package observed: `opencode-ai@1.14.39`
Binary observed: `opencode`

## Install and version

Official docs list `opencode run --format json` as the non-interactive JSON event mode. The local machine did not have `opencode` installed globally, so this spike used `npm exec` to avoid changing the global toolchain:

```sh
npm exec --yes --package=opencode-ai@1.14.39 -- opencode --version
```

Observed output:

```text
1.14.39
```

`npm view opencode-ai version bin --json` reported `latest` as `1.14.39` and `bin.opencode` as `bin/opencode` on 2026-05-06.

## Captured invocations

The success fixture uses OpenCode's built-in `opencode/gpt-5-nano` model, which was listed by `opencode models` despite zero stored credentials in `~/.local/share/opencode/auth.json`.

Setup:

```sh
rm -rf /tmp/opencode-schema-capture
mkdir -p /tmp/opencode-schema-capture
printf 'Maestro coordinates multiple coding-agent sessions from one terminal UI.\n' \
  > /tmp/opencode-schema-capture/note.txt
```

Success command:

```sh
npm exec --yes --package=opencode-ai@1.14.39 -- opencode run \
  --dir /tmp/opencode-schema-capture \
  --model opencode/gpt-5-nano \
  --format json \
  --dangerously-skip-permissions \
  "Read note.txt, create result.txt containing exactly one concise sentence summarizing it, then reply with that same sentence."
```

Full stdout fixture: `tests/fixtures/opencode_output_sample.jsonl`

Failure command:

```sh
npm exec --yes --package=opencode-ai@1.14.39 -- opencode run \
  --model anthropic/claude-sonnet-4-5 \
  --format json \
  "hello world"
```

Full stdout fixture: `tests/fixtures/opencode_error_sample.jsonl`

The failure invocation emitted one JSON error row on stdout, printed a stack trace to stderr, and exited with status `0`. The fixture intentionally captures stdout JSONL only, because that is the parser input stream.

## Representative success output

Full fixture: `tests/fixtures/opencode_output_sample.jsonl`

```jsonl
{"type":"step_start","timestamp":1778041170989,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"id":"prt_dfb832c2b0015EkRs6E70aCJo8","messageID":"msg_dfb83285d001HO3TxKY3vl73OG","sessionID":"ses_2047cd800ffeJnRns757z5nQug","type":"step-start"}}
{"type":"text","timestamp":1778041175573,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"id":"prt_dfb833e0f001OtApADp6Ok2Aie","messageID":"msg_dfb83285d001HO3TxKY3vl73OG","sessionID":"ses_2047cd800ffeJnRns757z5nQug","type":"text","text":"Reading note.txt to generate a concise summary.","time":{"start":1778041175567,"end":1778041175571},"metadata":{"openai":{"itemId":"msg_0310d5e15dd2cee70169fac157ad88819588c0a7271b7ec6d8"}}}}
{"type":"tool_use","timestamp":1778041175916,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"type":"tool","tool":"read","callID":"call_vCg9eBjtU06fBJkxkvpGD08R","state":{"status":"completed","input":{"filePath":"/private/tmp/opencode-schema-capture/note.txt","offset":1,"limit":2000},"output":"<path>/private/tmp/opencode-schema-capture/note.txt</path>\n<type>file</type>\n<content>\n1: Maestro coordinates multiple coding-agent sessions from one terminal UI.\n\n(End of file - total 1 lines)\n</content>","metadata":{"preview":"Maestro coordinates multiple coding-agent sessions from one terminal UI.","truncated":false,"loaded":[]},"title":"private/tmp/opencode-schema-capture/note.txt","time":{"start":1778041175909,"end":1778041175916}},"metadata":{"openai":{"itemId":"fc_0310d5e15dd2cee70169fac157bd508195b036ccb9d29cbc8c"}},"id":"prt_dfb833e140025iwhigiPPh6Teq","sessionID":"ses_2047cd800ffeJnRns757z5nQug","messageID":"msg_dfb83285d001HO3TxKY3vl73OG"}}
{"type":"step_finish","timestamp":1778041175936,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"id":"prt_dfb833f7f0017XP5P7BvKHC1IS","reason":"tool-calls","messageID":"msg_dfb83285d001HO3TxKY3vl73OG","sessionID":"ses_2047cd800ffeJnRns757z5nQug","type":"step-finish","tokens":{"total":10111,"input":9659,"output":68,"reasoning":384,"cache":{"write":0,"read":0}},"cost":0}}
{"type":"step_start","timestamp":1778041176695,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"id":"prt_dfb834275001zihLVNWbSV4CVP","messageID":"msg_dfb833f820021Edl9qqO3phaHj","sessionID":"ses_2047cd800ffeJnRns757z5nQug","type":"step-start"}}
{"type":"text","timestamp":1778041180278,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"id":"prt_dfb834f41001j04iBOtynzOnmB","messageID":"msg_dfb833f820021Edl9qqO3phaHj","sessionID":"ses_2047cd800ffeJnRns757z5nQug","type":"text","text":"Create result.txt with the single-sentence summary.","time":{"start":1778041179969,"end":1778041180276},"metadata":{"openai":{"itemId":"msg_00a9396a4e0c0f9e0169fac15c0ce481939f1dff7dd2722aeb"}}}}
{"type":"tool_use","timestamp":1778041180695,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"type":"tool","tool":"apply_patch","callID":"call_yW5pqQMj9mveBcPGL6EmR8HY","state":{"status":"completed","input":{"patchText":"*** Begin Patch\n*** Add File: result.txt\n+Maestro coordinates multiple coding-agent sessions from one terminal UI.\n*** End Patch"},"output":"Success. Updated the following files:\nA private/tmp/opencode-schema-capture/result.txt","metadata":{"diff":"Index: /private/tmp/opencode-schema-capture/result.txt\n===================================================================\n--- /private/tmp/opencode-schema-capture/result.txt\n+++ /private/tmp/opencode-schema-capture/result.txt\n@@ -0,0 +1,1 @@\n+Maestro coordinates multiple coding-agent sessions from one terminal UI.\n\n","files":[{"filePath":"/private/tmp/opencode-schema-capture/result.txt","relativePath":"private/tmp/opencode-schema-capture/result.txt","type":"add","patch":"Index: /private/tmp/opencode-schema-capture/result.txt\n===================================================================\n--- /private/tmp/opencode-schema-capture/result.txt\n+++ /private/tmp/opencode-schema-capture/result.txt\n@@ -0,0 +1,1 @@\n+Maestro coordinates multiple coding-agent sessions from one terminal UI.\n","additions":1,"deletions":0}],"diagnostics":{},"truncated":false},"title":"Success. Updated the following files:\nA private/tmp/opencode-schema-capture/result.txt","time":{"start":1778041180690,"end":1778041180694}},"metadata":{"openai":{"itemId":"fc_00a9396a4e0c0f9e0169fac15c34a88193b7d8e7e74eafac80"}},"id":"prt_dfb835075001H9geRjftWkd9eM","sessionID":"ses_2047cd800ffeJnRns757z5nQug","messageID":"msg_dfb833f820021Edl9qqO3phaHj"}}
{"type":"step_finish","timestamp":1778041180802,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"id":"prt_dfb835280001Fd2Z48voA4wYYi","reason":"tool-calls","messageID":"msg_dfb833f820021Edl9qqO3phaHj","sessionID":"ses_2047cd800ffeJnRns757z5nQug","type":"step-finish","tokens":{"total":10561,"input":185,"output":72,"reasoning":320,"cache":{"write":0,"read":9984}},"cost":0}}
{"type":"step_start","timestamp":1778041181378,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"id":"prt_dfb8354c1001RA1eKhMqkRI8pv","messageID":"msg_dfb835283002pxBj3e72vdt9Wi","sessionID":"ses_2047cd800ffeJnRns757z5nQug","type":"step-start"}}
{"type":"text","timestamp":1778041183032,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"id":"prt_dfb835814001a3xPim0SbgfOIv","messageID":"msg_dfb835283002pxBj3e72vdt9Wi","sessionID":"ses_2047cd800ffeJnRns757z5nQug","type":"text","text":"Maestro coordinates multiple coding-agent sessions from one terminal UI.","time":{"start":1778041182228,"end":1778041183031},"metadata":{"openai":{"itemId":"msg_0f82ffbe4e8d6f070169fac15e6b9881968be8cd7fdfea251c"}}}}
{"type":"step_finish","timestamp":1778041183109,"sessionID":"ses_2047cd800ffeJnRns757z5nQug","part":{"id":"prt_dfb835b83001JfyGjIVjRWkGcE","reason":"stop","messageID":"msg_dfb835283002pxBj3e72vdt9Wi","sessionID":"ses_2047cd800ffeJnRns757z5nQug","type":"step-finish","tokens":{"total":10601,"input":217,"output":16,"reasoning":0,"cache":{"write":0,"read":10368}},"cost":0}}
```

## Representative failure output

Full fixture: `tests/fixtures/opencode_error_sample.jsonl`

```jsonl
{"type":"error","timestamp":1778041147596,"sessionID":"ses_2047d2f9bffe35lyF8QtWwy1D1","error":{"name":"UnknownError","data":{"message":"Model not found: anthropic/claude-sonnet-4-5."}}}
```

## JSON event envelope

Every stdout row observed from `opencode run --format json` is one JSON object with this top-level shape:

- `type`: event discriminator such as `step_start`, `text`, `tool_use`, `step_finish`, or `error`.
- `timestamp`: Unix timestamp in milliseconds.
- `sessionID`: OpenCode session identifier.
- `part`: present for successful run events. `part.type` uses hyphenated names such as `step-start` and `step-finish`, while the top-level `type` uses snake case.
- `error`: present for failure events instead of `part`.

Successful events are already high-level: `text` rows contain complete text chunks in `part.text`, and `tool_use` rows contain completed tool state in `part.state.status`, `part.state.input`, `part.state.output`, and optional `part.state.metadata`.

`step_finish` carries completion and accounting data:

- `part.reason`: observed values were `tool-calls` and `stop`.
- `part.tokens.total`, `input`, `output`, `reasoning`, and `cache.write/read`.
- `part.cost`: numeric cost. The captured `opencode/gpt-5-nano` run reported `0`.

## Event mapping to Maestro

| OpenCode JSONL shape | Meaning | Maestro `StreamEvent` |
| --- | --- | --- |
| `{"type":"step_start","part":{"type":"step-start",...}}` | Assistant step starts. | Usually no event or `Unknown`; Maestro does not need a start marker. |
| `{"type":"text","part":{"type":"text","text":"..."}}` | Assistant text chunk. Chunks may be interim narration or final answer text. | `AssistantMessage { text }`. |
| `{"type":"tool_use","part":{"tool":"read","state":{"status":"completed","input":{"filePath":"..."}}}}` | Completed tool call. | `ToolUse { tool, file_path, command_preview, subagent_name }`. Use `input.filePath` for file paths. |
| `{"type":"tool_use","part":{"tool":"apply_patch","state":{"input":{"patchText":"..."}}}}` | Completed edit tool call. | `ToolUse { tool: "apply_patch", file_path: inferred from patch text when practical, command_preview: None, subagent_name: None }`. |
| `{"type":"tool_use","part":{"state":{"output":"...","metadata":{...}}}}` | Tool result is embedded in the same completed tool event. | Optionally emit `ToolResult { tool, is_error }` after `ToolUse`. Treat `state.status != "completed"` as error-like if observed. |
| `{"type":"step_finish","part":{"reason":"tool-calls",...}}` | Step ended because model requested tool calls. | No completion event; token accounting may emit `TokenUpdate`. |
| `{"type":"step_finish","part":{"reason":"stop","cost":0,...}}` | Run completed normally. | `Completed { cost_usd: part.cost }` plus `TokenUpdate` from `part.tokens`. |
| `{"type":"error","error":{"data":{"message":"..."}}}` | Run failed before or during model execution. | `Error { message }`. Do not assume non-zero process exit; the observed invalid-model run exited `0`. |
| Malformed or unknown row | Parser cannot classify row. | `Unknown { raw }`. |

## Provider coverage

Configured provider state on this machine:

```text
Credentials ~/.local/share/opencode/auth.json
0 credentials
```

Observed `opencode models` output listed only `opencode/*` models:

```text
opencode/big-pickle
opencode/gpt-5-nano
opencode/hy3-preview-free
opencode/minimax-m2.5-free
opencode/nemotron-3-super-free
```

Provider-specific model listing for `anthropic`, `openrouter`, and `groq` returned provider-not-found errors in this environment, so schema stability across those providers is not confirmed by this spike. The captured success-path schema is confirmed for `opencode/gpt-5-nano`; the captured failure-path schema is confirmed for an invalid `anthropic/claude-sonnet-4-5` model reference.

## Conclusions for #552 and #617

OpenCode has a usable non-interactive JSONL stream through `opencode run --format json`. The parser should not be `Unknown`-only once these fixtures are available.

Implementation guidance:

- Invoke `opencode run --format json --model <provider/model> <prompt>` for headless sessions.
- Treat stdout as JSONL. Stderr may contain diagnostics that duplicate an error row.
- Parse top-level `type` first; use `part.type` only as a consistency check.
- Emit text directly from `type:"text"` rows.
- Emit completed tool calls from `type:"tool_use"` rows; unlike Claude/Qwen streaming formats, the observed OpenCode event already includes completed tool input and output in one row.
- Emit `Completed` only for `step_finish` rows with `part.reason:"stop"`.
- Emit `Error` for `type:"error"` rows and do not rely on process exit status to detect failure.

Known unknowns:

- Auth-missing shape for a configured but unauthenticated provider was not captured because Anthropic/OpenRouter/Groq were not configured at all.
- Cross-provider schema stability remains unconfirmed.
- No separate `tool_result` row was observed; tool output was embedded in `tool_use.part.state.output`.
- No thinking/reasoning event row was observed, although token accounting reports `reasoning`.
