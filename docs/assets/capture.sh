#!/usr/bin/env bash
# Scripted demo capture of the real maestro TUI (#1061): three agents fix
# three real, already-shipped maestro bugs side by side.
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
# Output:
#   docs/assets/demo.webm — gh-pages hero (2 MB budget). To ship it, commit
#     it to the gh-pages branch, where index.html embeds it as
#     <video autoplay muted loop playsinline>.
#   docs/assets/demo.gif  — README hero and social posts.
#
# Privacy: the whole run uses a throwaway HOME. Fleet Board and Adopt
# Session read $HOME/.claude/projects, so they only ever see the fake
# conversations seeded below — never the real ones on this machine. The
# demo lives under /tmp/maestro-demo.* so no user name or local path shows
# up on screen. Watch the output end to end before publishing anyway.
#
# Safety: the agent is docs/assets/demo-agent.sh (scripted stream-json, edits
# nothing). auto_pr is off and origin's push URL is invalid, so nothing can
# reach GitHub except read-only gh calls for the stats bar.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

# 1. The capture drives the real binary, built from this tree (release:
#    the debug TUI loop is too slow to redraw under ttyd).
cargo build --release --quiet
bin="$repo_root/target/release/maestro"

# gh auth comes from the real environment before HOME is swapped. It lives
# only in the process env: never in the tape, never on screen.
GH_TOKEN="$(gh auth token 2>/dev/null || true)"
export GH_TOKEN

# 2. Isolated demo project + fake HOME.
demo="$(mktemp -d /tmp/maestro-demo.XXXXXX)"
trap 'rm -rf "$demo"' EXIT
fake_home="$demo/home"
mkdir -p "$fake_home"

cp "$bin" "$demo/maestro"
cp "$repo_root/docs/assets/demo-agent.sh" "$demo/claude"

git -C "$demo" init -q
git -C "$demo" config user.name demo
git -C "$demo" config user.email demo@example.invalid
git -C "$demo" commit -q --allow-empty -m init
git -C "$demo" remote add origin https://github.com/CarlosDanielDev/maestro
git -C "$demo" remote set-url --push origin no-push://disabled

(cd "$demo" && HOME="$fake_home" ./maestro init --non-interactive >/dev/null 2>&1) || {
  echo "maestro init failed in demo dir" >&2
  exit 1
}
sed -i '' \
  -e 's/^auto_pr = true/auto_pr = false/' \
  -e 's/^desktop = true/desktop = false/' \
  -e "s#^command = \"claude\"#command = \"$demo/claude\"\\
transport = \"headless\"#" \
  "$demo/maestro.toml"
grep -q '^auto_pr = true' "$demo/maestro.toml" && {
  echo "auto_pr still on — refusing to record" >&2
  exit 1
}
(cd "$demo" && HOME="$fake_home" ./maestro trust --yes >/dev/null)

# Fake Claude Code conversations for Fleet Board and Adopt Session.
seed_transcript() { # <n> <cwd> <title> <age, as for date -v->
  local dir
  dir="$fake_home/.claude/projects/$(printf %s "$2" | sed 's#[/.]#-#g')"
  local id="0000000$1-demo-4000-8000-00000000000$1"
  mkdir -p "$dir"
  printf '{"type":"user","sessionId":"%s","cwd":"%s","message":{"role":"user","content":"%s"}}\n{"type":"ai-title","sessionId":"%s","aiTitle":"%s"}\n' \
    "$id" "$2" "$3" "$id" "$3" >"$dir/$id.jsonl"
  touch -t "$(date -v-"$4" +%Y%m%d%H%M.%S)" "$dir/$id.jsonl"
}
seed_transcript 1 /home/dev/acme-api "Fix flaky checkout retry test" 3M
seed_transcript 2 /home/dev/acme-web "Dark mode tokens for the settings page" 25M
seed_transcript 3 /home/dev/maestro "Triage v0.32 bug reports" 2H
seed_transcript 4 /home/dev/infra "Bump Postgres to 17 in staging" 6H
seed_transcript 5 /home/dev/acme-api "Profile slow /orders endpoint" 1d

# 3. Record. Landing -> Fleet Board -> Adopt Session -> dashboard -> launch
#    three bug-fix prompts -> watch them run to green -> token report -> back to the all-green overview.
cat >"$demo/demo.tape" <<EOF
Output demo.webm
Output demo.gif
Set Shell bash
Set FontFamily "Hack Nerd Font"
Set FontSize 16
Set Width 1280
Set Height 800
Set Framerate 20
Set TypingSpeed 35ms
Set Padding 16
Set WindowBar Colorful
Set Theme {"background": "#0b0e0c", "foreground": "#d8ded9", "cursor": "#45e08a", "selectionBackground": "#1d2b22"}
Hide
Type "export HOME=$fake_home PS1='\$ '; clear"
Enter
Show
Type "./maestro"
Enter
Sleep 5500ms
Type "f"
Sleep 3500ms
Escape
Sleep 600ms
Type "a"
Sleep 2500ms
Escape
Sleep 600ms
Type "d"
Sleep 2500ms
Type "r"
Sleep 600ms
Type "fix #1155: a model maestro cannot price shows \$0.00"
Enter
Sleep 1500ms
Escape
Sleep 600ms
Type "r"
Sleep 600ms
Type "fix #1154: one network error aborts maestro run"
Enter
Sleep 1500ms
Escape
Sleep 600ms
Type "r"
Sleep 600ms
Type "fix #1153: the TUI freezes while a session finishes"
Enter
Sleep 23000ms
Escape
Sleep 1200ms
Type "t"
Sleep 3500ms
Escape
Sleep 5000ms
EOF
(cd "$demo" && vhs demo.tape)

# 4. Land the captures and enforce the gh-pages size budget.
mv "$demo/demo.webm" "$demo/demo.gif" "$repo_root/docs/assets/"
bytes="$(wc -c <"$repo_root/docs/assets/demo.webm" | tr -d ' ')"
max_bytes=$((2 * 1024 * 1024))
if [ "$bytes" -ge "$max_bytes" ]; then
  echo "docs/assets/demo.webm is $bytes bytes — over the 2 MB budget; shorten the tape" >&2
  exit 1
fi
echo "docs/assets/demo.webm written ($bytes bytes, under budget)"
echo "docs/assets/demo.gif written ($(wc -c <"$repo_root/docs/assets/demo.gif" | tr -d ' ') bytes)"
