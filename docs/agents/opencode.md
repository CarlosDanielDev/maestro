# OpenCode Agent

OpenCode is a subprocess provider. Maestro runs `opencode run --format json`,
uses OpenCode's provider configuration, and expects model names in
`provider/model` format.

OpenCode can route to many cloud and local AI backends through its provider
system, so cost and model availability depend on the OpenCode account and
provider credentials you configure.

## Install

Use one of OpenCode's supported install methods:

```bash
brew install anomalyco/tap/opencode
npm install -g opencode-ai
curl -fsSL https://opencode.ai/install | bash
```

Verify:

```bash
opencode --version
```

## Authenticate

Open OpenCode and run:

```text
/connect
```

Add credentials for the provider you want to use. Maestro's health check looks
for OpenCode credentials at:

```text
~/.local/share/opencode/auth.json
```

If `XDG_DATA_HOME` is set, Maestro checks:

```text
$XDG_DATA_HOME/opencode/auth.json
```

## Configuration

```toml
[agents]
default = "opencode"

[agents.opencode]
kind = "opencode"
enabled = true
command = "opencode"
model = "openrouter/deepseek/deepseek-chat"
extra_args = []
```

Low-cost examples:

```toml
[agents.opencode-openrouter]
kind = "opencode"
enabled = true
command = "opencode"
model = "openrouter/deepseek/deepseek-chat"

[agents.opencode-groq]
kind = "opencode"
enabled = true
command = "opencode"
model = "groq/llama-3.1-8b-instant"
```

The exact model ids must exist in your OpenCode provider setup. Use
`provider/model`, not only the bare model name.

## Doctor Output

Healthy:

```text
agent opencode   OK      <opencode version>
```

Auth missing:

```text
agent opencode   FAIL    opencode auth not found; run `opencode /connect` to authenticate with a provider
```

CLI missing:

```text
agent opencode   FAIL    opencode CLI not found; install with ...
```

## Usage

```bash
maestro run --prompt "Implement the issue parser" --agent opencode
```

## Troubleshooting

- **Auth not configured**: run `opencode`, enter `/connect`, and add a provider
  credential.
- **Model not found**: check the provider/model id in OpenCode and confirm the
  provider account has access.
- **Wrong provider cost**: OpenCode delegates billing to the selected backend.
  Check pricing with the backend provider before using it as the default.
- **No JSON events**: update OpenCode and confirm `opencode run --format json`
  works outside Maestro.
