# Security

## Plugin Execution Model

Maestro plugins execute arbitrary shell commands by design. When a plugin is configured in `maestro.toml`, its `run` field is passed directly to `sh -c`.

## Configuration Supply-Chain Risk

Treat `maestro.toml` as an executable runbook. Plugin `run` fields in that file are executed as shell commands the next time the matching hook runs, including hooks such as `session_completed`. The execution path is `src/plugins/runner.rs`, where `PluginRunner` creates `Command::new("sh")` and passes `["-c", &plugin.run]`.

In shared or collaborative repositories, review `maestro.toml` changes from pulls, rebases, or PR branches before running `maestro` locally. A practical check is:

```sh
git diff HEAD -- maestro.toml
```

If the branch was just pulled or checked out, inspect any added or changed plugin `run` commands before invoking `maestro`.

### Environment Variable Isolation

Plugin commands receive environment variables from `HookContext`. To prevent override of security-sensitive system variables (`PATH`, `LD_PRELOAD`, `DYLD_INSERT_LIBRARIES`, etc.), only variables with the `MAESTRO_` prefix are injected into the plugin subprocess.

- Variable names must match `^[A-Z][A-Z0-9_]*$`
- Variable names must start with `MAESTRO_`
- Variables that fail validation are silently skipped with a warning log

### Input Validation

All user-controlled inputs are validated before use:

- **Branch names**: validated against `^[a-zA-Z0-9/_.\-]+$`, `..` sequences rejected
- **Worktree slugs**: validated against `^[a-zA-Z0-9_\-]+$` (no path separators)
- **GitHub CLI arguments**: must not start with `-` or contain null bytes

### Binary Update Integrity

The auto-updater verifies SHA-256 checksums from a `sha256sums.txt` file published alongside release binaries. If verification fails, the update is aborted and the existing binary is not modified.

### State File Locking

The state store uses advisory file locks to prevent concurrent read/write races between multiple maestro processes.

## Reporting Vulnerabilities

If you find a security vulnerability, please open a GitHub issue with the `security` label.
