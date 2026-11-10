# Workspace trust

A repo-local `maestro.toml` (or `.maestro/config.toml`) can declare values
that make maestro execute programs. Maestro will not run any of them until
you trust that workspace (#1045).

## What is gated

Any of these keys, when they declare something that is not a built-in
default binary name (`claude`, `codex`, `qwen`, `opencode`):

- `[agents.<id>].command`, `[agents.<id>].env`, `[agents.<id>].extra_args`
- `[[plugins]].run`
- `[[sessions.completion_gates.commands]].run`
- `[gates].test_command` (when it differs from the default)
- `[review].command`, `[[review.reviewers]].command`

Sessions spawned with a config-supplied `command` are also blocked, and
`maestro doctor` probes only the built-in default binaries while a
workspace is untrusted.

## What an untrusted workspace cannot do

- Plugins never execute (`fire` reports each run as skipped).
- Executing completion gates (`tests_pass`, named `command` gates) fail
  closed with a "not trusted" message; filesystem-only gates still run.
- Review dispatch is skipped (both the single command and the council).
- `maestro run` refuses to start.
- Session spawn refuses config-supplied agent commands.

## Giving consent

First encounter in an interactive terminal shows the values verbatim and
asks `Trust this workspace's config? [y/N]`. Answering `y` records trust.
You can also manage the record explicitly:

```sh
maestro trust          # show values, ask, record
maestro trust --yes    # record without the prompt (after reviewing)
maestro untrust        # revoke
```

## Where the record lives

`trusted.toml` in your maestro config directory (for example
`~/.config/maestro/trusted.toml` on Linux), keyed by the canonicalized
config path plus the SHA-256 of the file content. The record:

- lives **outside the repo**, so the repo's own config can never grant
  trust to itself;
- is re-checked on every load — changing the config file re-prompts;
- is removed by `maestro untrust`.

This is the same model direnv uses for `.envrc` allowances.
