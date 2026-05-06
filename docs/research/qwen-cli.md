# Qwen Code CLI research

Issue: #650
Date observed: 2026-05-06
CLI package observed: `@qwen-code/qwen-code@0.15.6`
Binary observed: `qwen`

## Install and version

Install command:

```sh
npm install -g @qwen-code/qwen-code@latest
```

The package exposes the `qwen` binary. This spike used `npx` to avoid changing the global toolchain:

```sh
npx -y @qwen-code/qwen-code@0.15.6 --version
```

Observed output:

```text
0.15.6
```

`npm view @qwen-code/qwen-code version name bin dist-tags --json` reported `latest` as `0.15.6` and `bin.qwen` as `cli.js` on 2026-05-06.

## Non-interactive argv

Qwen Code supports headless single-prompt sessions with positional prompt text or `-p` / `--prompt`. The CLI currently marks `--prompt` as deprecated in help text, but it remains the documented headless flag and is explicit enough for provider construction.

Recommended Maestro argv:

```sh
qwen \
  --bare \
  --auth-type openai \
  --model qwen-test \
  --output-format stream-json \
  --include-partial-messages \
  --prompt "Read the first line of README.md, then summarize it in one sentence."
```

For OpenAI-compatible endpoints, add credentials through either environment/settings or direct flags:

```sh
qwen \
  --bare \
  --auth-type openai \
  --openai-api-key "$OPENAI_API_KEY" \
  --openai-base-url "$OPENAI_BASE_URL" \
  --model "$OPENAI_MODEL" \
  --output-format stream-json \
  --include-partial-messages \
  --prompt "..."
```

The captured fixture used a local OpenAI-compatible test endpoint so no real API token was required:

```sh
npx -y @qwen-code/qwen-code@0.15.6 \
  --bare \
  --auth-type openai \
  --openai-api-key local-redacted \
  --openai-base-url http://127.0.0.1:48767/v1 \
  --model qwen-test \
  --output-format stream-json \
  --include-partial-messages \
  --approval-mode yolo \
  --prompt "Read the first line of README.md, then summarize it in one sentence."
```

The fixture is verbatim Qwen Code CLI stdout from that invocation. The model backend was controlled, but the command construction, tool execution, and emitted JSONL envelope are Qwen Code's real CLI behavior.

## Output formats

Observed help exposes:

- `--output-format text`: plain final text.
- `--output-format json`: one JSON array emitted after completion.
- `--output-format stream-json`: JSONL messages emitted as the run progresses.
- `--include-partial-messages`: when paired with `stream-json`, emits lower-level `stream_event` rows such as `message_start`, `content_block_delta`, and `message_stop`.
- `--input-format stream-json`: accepted only with `--output-format stream-json`; reserved for bidirectional protocol messages.

For Maestro, `stream-json` plus `--include-partial-messages` is the useful parser target because it exposes text deltas, tool-use blocks, tool results, and final results as separate JSONL rows.

## Representative output sample

Full fixture: `tests/fixtures/qwen_output_sample.jsonl`

```jsonl
{"type":"system","subtype":"init","uuid":"2a056c7d-36ad-4a40-97ab-25b810700d8c","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","cwd":"/Users/carlos/projects/maestro","tools":["read_file","edit","run_shell_command"],"mcp_servers":[],"model":"qwen-test","permission_mode":"yolo","slash_commands":["bug","clear","compress","context","docs","doctor","export","init","insight","language","model","stats","status","summary","tasks"],"qwen_code_version":"0.15.6","agents":["general-purpose","Explore","statusline-setup"]}
{"type":"stream_event","uuid":"e0315574-926b-4e38-8e81-6315b644e061","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"message_start","message":{"id":"e0611a03-94c0-4e4b-a770-d9339d7e8087","role":"assistant","model":"qwen-test","content":[]}}}
{"type":"stream_event","uuid":"611330c3-dc95-48b1-8872-e2fab99111d0","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_readme","name":"read_file","input":{}}}}
{"type":"stream_event","uuid":"8e77e223-fc23-4c9d-b319-aba49e13db4c","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":\"/Users/carlos/projects/maestro/README.md\",\"offset\":0,\"limit\":1}"}}}
{"type":"stream_event","uuid":"efd69bfb-10c0-49e1-80a5-08712e662c48","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"content_block_stop","index":0}}
{"type":"assistant","uuid":"e0611a03-94c0-4e4b-a770-d9339d7e8087","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"message":{"id":"e0611a03-94c0-4e4b-a770-d9339d7e8087","type":"message","role":"assistant","model":"qwen-test","content":[{"type":"tool_use","id":"call_readme","name":"read_file","input":{"file_path":"/Users/carlos/projects/maestro/README.md","offset":0,"limit":1}}],"stop_reason":"tool_use","usage":{"input_tokens":0,"output_tokens":0}}}
{"type":"stream_event","uuid":"f0599953-f9b2-4e25-ae6e-297e69dcc867","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"message_stop"}}
{"type":"user","uuid":"1ed9c16f-8d48-4d29-8ae5-40124e8aaece","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_readme","is_error":false,"content":"Read lines 1-1 of 222 from README.md"}]}}
{"type":"stream_event","uuid":"e46823a6-dd65-4480-911d-69b18ae4db63","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"message_start","message":{"id":"45ce417e-56a8-4a8c-a589-e57080dac0cd","role":"assistant","model":"qwen-test","content":[]}}}
{"type":"stream_event","uuid":"b357b6ed-3bec-41f3-a7d1-0b120aff7fb1","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}
{"type":"stream_event","uuid":"eca0b477-eb87-4c09-bc08-a57761455d79","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The README title identifies the project as Maestro."}}}
{"type":"stream_event","uuid":"9c480841-61a8-4aeb-82bf-99b7fc542709","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"content_block_stop","index":0}}
{"type":"assistant","uuid":"45ce417e-56a8-4a8c-a589-e57080dac0cd","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"message":{"id":"45ce417e-56a8-4a8c-a589-e57080dac0cd","type":"message","role":"assistant","model":"qwen-test","content":[{"type":"text","text":"The README title identifies the project as Maestro."}],"stop_reason":null,"usage":{"input_tokens":0,"output_tokens":0}}}
{"type":"stream_event","uuid":"68e03f02-64b8-440f-b45b-a6fee9158455","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","parent_tool_use_id":null,"event":{"type":"message_stop"}}
{"type":"result","subtype":"success","uuid":"b0cf3f7d-729c-4506-9474-8bbf6f9d453b","session_id":"2a056c7d-36ad-4a40-97ab-25b810700d8c","is_error":false,"duration_ms":500,"duration_api_ms":407,"num_turns":2,"result":"The README title identifies the project as Maestro.","usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0},"permission_denials":[]}
```

## Event mapping to Maestro

| Qwen JSONL shape | Meaning | Maestro `StreamEvent` |
| --- | --- | --- |
| `{"type":"system","subtype":"init",...}` | Session metadata, available tools, model, permission mode, CLI version. | Usually `Unknown` today. Future parser may use this for provider metadata; no current `StreamEvent` variant is required. |
| `{"type":"stream_event","event":{"type":"message_start",...}}` | Assistant turn starts. | Usually no event or `Unknown`; Maestro does not need a start marker. |
| `{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"text",...}}}` | Text block starts. | No event until text appears. |
| `{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"..."}}}` | Streaming assistant text delta. | `AssistantMessage { text }` if Maestro wants live deltas. |
| `{"type":"assistant","message":{"content":[{"type":"text","text":"..."}]}}` | Completed assistant text message. | `AssistantMessage { text }` if parser chooses final-message semantics. Avoid double-emitting if deltas are already emitted. |
| `{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"...","name":"read_file","input":{}}}}` followed by `input_json_delta` | Tool call starts and its JSON args stream separately. | Buffer by content-block index/tool id until `content_block_stop`, then emit `ToolUse { tool, file_path, command_preview, subagent_name }`. |
| `{"type":"assistant","message":{"content":[{"type":"tool_use","name":"read_file","input":{...}}],"stop_reason":"tool_use"}}` | Completed tool call. | `ToolUse { tool, ... }` if parser chooses final-message semantics. Avoid double-emitting if buffered stream events already emitted. |
| `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"...","is_error":false,...}]}}` | Tool result returned to the model. | `ToolResult { tool: "unknown", is_error }` unless the parser keeps `tool_use_id -> tool name` state. With state, emit the original tool name. |
| `{"type":"result","subtype":"success","is_error":false,...}` | Run completed. | `Completed { cost_usd: 0.0 }`; Qwen output did not include cost in observed local run. |
| API connection failure in observed `stream-json` run | Qwen emitted an assistant text message containing `[API Error: ...]`, then a `result` row with `subtype:"success"` and `is_error:false`. | Parser should detect `message.content[].text` beginning with `[API Error:` and emit `Error { message }`; do not rely only on `result.is_error`. |
| Malformed or unknown row | Parser cannot classify row. | `Unknown { raw }`. |

## Authentication and credential lookup

Observed auth-related CLI surface:

- `qwen auth` subcommands: `qwen-oauth`, `coding-plan`, `openrouter`, `api-key`, and `status`.
- `--auth-type` values: `openai`, `anthropic`, `qwen-oauth`, `gemini`, `vertex-ai`.
- OpenAI-compatible direct flags: `--openai-api-key`, `--openai-base-url`, `--model`.
- Environment variables documented for OpenAI-compatible headless use: `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`; Qwen/Dashscope examples commonly use `DASHSCOPE_API_KEY` through `modelProviders`.
- Settings files include `~/.qwen/settings.json`, project `.qwen/settings.json`, system defaults, and system settings. On macOS, system defaults/settings live under `/Library/Application Support/QwenCode/`.
- `.env` lookup order for Qwen-specific environment loading: project `.qwen/.env`, project `.env`, home `~/.qwen/.env`, then home `~/.env`; shell environment variables override `.env` and settings-file `env` values.

For Maestro, prefer passing env through the provider process environment and passing only non-secret args (`--auth-type`, `--model`, `--output-format`). Use direct key flags only when a user has explicitly configured that style, because argv may be visible in process listings.

## Conclusions for #548 and #552

Qwen has a usable non-interactive JSONL mode. Do not ship a `StreamEvent::Unknown`-only parser for Qwen if this fixture is available.

Implementation guidance:

- `QwenProvider` should invoke `qwen --bare --output-format stream-json --include-partial-messages` for Maestro-managed sessions.
- Use `--approval-mode` from the resolved agent permission mode; `yolo` works for non-interactive tool execution, while default mode can require confirmations for unsafe tools.
- `QwenParser` should be stateful enough to buffer `stream_event` tool-use JSON deltas and remember `tool_use_id -> tool name` for later tool results.
- The top-level output envelope is close to Claude's `stream-json`, but not identical: Qwen wraps low-level events under `type:"stream_event"` and represents tool results as a top-level `type:"user"` message containing `tool_result` content.
- API failures may appear as assistant text plus a success result. Parser adapters should explicitly classify `[API Error:` assistant text as an error.

## Known unknowns / TODOs

- A live Qwen/Dashscope run was not captured because no Qwen/Dashscope credential was present in the environment or local Qwen config. The fixture still captures real Qwen Code CLI output using a local OpenAI-compatible endpoint.
- Token usage and cost fields were zero in the local endpoint run. Re-check against a paid provider before adding cost accounting beyond `Completed { cost_usd: 0.0 }`.
- Permission-denial event shape was not captured. `permission_denials` exists on `result`; interactive/default approval behavior should be covered by provider integration tests.
- `--prompt` is marked deprecated by the CLI help even though docs still use `-p/--prompt`; positional prompt text may be the longer-term argv choice.
