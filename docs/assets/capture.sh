#!/usr/bin/env bash
# Scripted demo capture of the real maestro TUI for the gh-pages hero (#1061).
#
# Tooling: vhs (charmbracelet) — https://github.com/charmbracelet/vhs
# Install: brew install vhs   (vhs brings ttyd and ffmpeg along)
# Font:    a Nerd Font is required for the TUI's icon glyphs, e.g.
#          brew install --cask font-hack-nerd-font
# Headless: vhs drives a headless ttyd terminal from the tape below — no
# human keystrokes; the script seeds everything the demo shows.
#
# Re-run whenever the TUI changes:
#   ./docs/assets/capture.sh
# Output: docs/assets/demo.webm (kept under the 2 MB gh-pages budget).
# To ship it: commit the file to the gh-pages branch, where index.html
# embeds it as <video autoplay muted loop playsinline>.
#
# Deliberately narrow key arc — screens that render local machine state
# (Fleet Board reads the user's real ~/.claude session history) must never
# appear in a public capture.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

# 1. The capture drives the real binary, built from this tree (release:
#    the debug TUI loop is too slow to redraw under ttyd).
cargo build --release --quiet
bin="$repo_root/target/release/maestro"

# 2. Stage an isolated demo project so the capture is deterministic and
#    touches nothing outside its own temp dir.
demo="$(mktemp -d "${TMPDIR:-/tmp}/maestro-demo-capture.XXXXXX")"
trap 'rm -rf "$demo"' EXIT

cp "$bin" "$demo/maestro"

# A git repo with the project remote so doctor checks stay green and the
# stats bar resolves real context.
git -C "$demo" init -q
git -C "$demo" remote add origin https://github.com/CarlosDanielDev/maestro

# A valid config via the binary's own initializer, with the permission mode
# softened off bypass (its red banner would dominate the hero) and the
# [agents] block doctor's schema check expects.
(cd "$demo" && ./maestro init --non-interactive >/dev/null 2>&1) || {
  echo "maestro init failed in demo dir" >&2
  exit 1
}
sed -i '' 's/permission_mode = "bypassPermissions"/permission_mode = "acceptEdits"/' "$demo/maestro.toml"
cat >> "$demo/maestro.toml" <<'EOF'

[agents]
default = "claude"

[agents.claude]
kind = "claude"
enabled = true
command = "claude"
model = "opus"
permission_mode = "acceptEdits"
allowed_tools = []
EOF

# Seeded state: three sessions so the dashboard opens with recent activity.
cat > "$demo/maestro-state.json" <<'EOF'
{
  "version": 1,
  "sessions": [
    {
      "id": "6f1de7a0-2c34-4e11-9a5e-3b1c8d90a101",
      "status": "running",
      "prompt": "Wire quota routing behind the [quota] config flag",
      "issue_number": 1082,
      "issue_title": "Wire quota routing, dashboard badges and 97% alert behind [quota] enabled",
      "model": "claude-opus-5-5",
      "mode": "orchestrator",
      "started_at": "2026-11-10T12:04:00Z",
      "finished_at": null,
      "cost_usd": 1.24,
      "context_pct": 42.0,
      "current_activity": "Editing src/tui/app/budget.rs",
      "last_message": "Wiring quota routing into the dashboard",
      "activity_log": [
        { "timestamp": "2026-11-10T12:04:10Z", "message": "Session started" },
        { "timestamp": "2026-11-10T12:06:30Z", "message": "Reading quota config schema" }
      ],
      "files_touched": ["src/tui/app/budget.rs", "src/config/mod.rs"],
      "pid": null
    },
    {
      "id": "9a42c5e1-7d08-4b62-a3f1-5c2e90bd7f22",
      "status": "completed",
      "prompt": "Vendor the setwave guard hooks in-repo",
      "issue_number": 1086,
      "issue_title": "Vendor setwave guard hooks in-repo + session memory log",
      "model": "claude-sonnet-5-5",
      "mode": "orchestrator",
      "started_at": "2026-11-10T11:40:00Z",
      "finished_at": "2026-11-10T11:58:00Z",
      "cost_usd": 0.87,
      "context_pct": 88.0,
      "current_activity": "Finished — gates green",
      "last_message": "Guard hooks vendored, all five guards pass",
      "activity_log": [
        { "timestamp": "2026-11-10T11:40:10Z", "message": "Session started" },
        { "timestamp": "2026-11-10T11:57:40Z", "message": "Completed" }
      ],
      "files_touched": [".maestro/hooks/guards.py"],
      "pid": null
    },
    {
      "id": "2b8d94f6-1e57-4c93-8a20-d47c6f08ba33",
      "status": "queued",
      "prompt": "Wire the sanitize module into the pipeline",
      "issue_number": 1088,
      "issue_title": "Sanitize module wiring",
      "model": "claude-sonnet-5-5",
      "mode": "vibe",
      "started_at": null,
      "finished_at": null,
      "cost_usd": 0.0,
      "context_pct": 0.0,
      "current_activity": "Waiting for a free slot",
      "last_message": "Queued — sanitize module wiring",
      "activity_log": [],
      "files_touched": [],
      "pid": null
    }
  ],
  "total_cost_usd": 2.11,
  "file_claims": {},
  "last_updated": "2026-11-10T12:30:00Z"
}
EOF

# 3. Sanity gate: a broken seed would silently render an empty dashboard.
status_out="$(cd "$demo" && ./maestro status)"
printf '%s\n' "$status_out" | grep -q "Sessions: 3 total" || {
  echo "seeded state did not load — aborting before capture" >&2
  exit 1
}

# 4. Record the demo: landing menu -> dashboard with the seeded sessions
#    -> quit. Plain keys only. Project Stats and Fleet Board are left out
#    on purpose: one needs a >10s network fetch, the other renders the
#    user's real local session history (never ship that).
cat > "$demo/demo.tape" <<'EOF'
Output demo.webm
Set Shell bash
Set FontFamily "Hack Nerd Font"
Set FontSize 16
Set Width 1280
Set Height 800
Set TypingSpeed 100ms
Set Padding 16
Set WindowBar Colorful
Set Theme {"background": "#0b0e0c", "foreground": "#d8ded9", "cursor": "#45e08a", "selectionBackground": "#1d2b22"}
Type "./maestro"
Enter
Sleep 4500ms
Type "d"
Sleep 8500ms
Ctrl+X
Sleep 500ms
EOF
(cd "$demo" && vhs demo.tape)

# 5. Land the capture in the repo and enforce the gh-pages size budget.
mv "$demo/demo.webm" "$repo_root/docs/assets/demo.webm"
bytes="$(wc -c < "$repo_root/docs/assets/demo.webm" | tr -d ' ')"
max_bytes=$((2 * 1024 * 1024))
if [ "$bytes" -ge "$max_bytes" ]; then
  echo "docs/assets/demo.webm is $bytes bytes — over the 2 MB budget; shorten the tape" >&2
  exit 1
fi
echo "docs/assets/demo.webm written ($bytes bytes, under budget)"
