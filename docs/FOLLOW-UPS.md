# Pending Follow-Ups

Security and hardening items that are non-blocking but should be filed as GitHub issues before the next release.
Items are ordered by severity (High → Low within each section).

---

## Bracketed-Paste Hardening (post-#441, target v0.14.2+)

These items were surfaced by the security review of the bracketed-paste fix (#441).
None blocked the merge; all are Medium or lower severity.

### Medium

- **Add `MAX_PASTE_BYTES` cap in `App::handle_paste`**
  A malicious or accidental terminal paste of a very large string (multi-MiB) can stall the event loop while `TextArea::insert_str` works through the buffer. Cap incoming paste payloads at ~1 MiB (configurable) and log a `tracing::warn!` with the truncated byte count when the limit is hit.
  Affected file: `src/tui/app/event_handler.rs`

- **Add `TerminalGuard` drop + `std::panic::set_hook` for panic-safe teardown**
  If the TUI panics after bracketed paste is enabled, raw mode, alt-screen, mouse capture, and `BracketedPaste` may remain active, leaving the user's terminal in a broken state. Implement a `TerminalGuard` RAII wrapper that calls `disable_raw_mode`, `LeaveAlternateScreen`, `DisableMouseCapture`, and `DisableBracketedPaste` in its `Drop` impl. Register a `panic::set_hook` that flushes stdout before the default panic formatter runs. This benefits all TUI modes, not only paste.
  Affected files: `src/tui/mod.rs` (event loop entry point), new `src/tui/terminal_guard.rs`

### Low

- **Image-path paste — directory allow-list**
  `paste_text` now strips C0 control bytes (ESC, NUL, BEL, DEL, …) from image-list pastes via `sanitize_paste`, which removes the prompt-injection vector raised in the review. What remains is the broader question of validating that a pasted path resolves inside an allow-listed directory before the session launches — that's an orthogonal policy question and should be filed separately.
  Affected file: `src/tui/screens/prompt_input.rs` (`paste_text` method, image-list branch)

---

*Last updated: 2026-04-22. File as GitHub issues tagged `hardening` before v0.14.2 planning.*

### Resolved in #441

- ~~Strip control bytes before `TextArea::insert_str`~~ — implemented as `sanitize_paste` in `src/tui/screens/prompt_input.rs` (filters any `char::is_control()` except `\n` and `\t`).
