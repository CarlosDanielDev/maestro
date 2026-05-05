#!/usr/bin/env bash
# Regenerate the README hero from the canonical insta landing snapshot plus
# the app chrome visible on the real welcome page.
#
# Required tool:
#   freeze v0.2.2 - https://github.com/charmbracelet/freeze
#
# Install:
#   go install github.com/charmbracelet/freeze@v0.2.2

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SNAPSHOT="$ROOT_DIR/src/tui/snapshot_tests/snapshots/maestro__tui__snapshot_tests__landing__landing_welcome_180x48_nerd_font.snap"
OUTPUT="$ROOT_DIR/docs/assets/readme-hero.svg"
FONT_FILE="/System/Library/Fonts/SFNSMono.ttf"
FONT_ARGS=()
if [[ -f "$FONT_FILE" ]]; then
  FONT_ARGS=(--font.file "$FONT_FILE")
fi

if ! command -v freeze >/dev/null 2>&1; then
  echo "error: freeze is required; install with: go install github.com/charmbracelet/freeze@v0.2.2" >&2
  exit 1
fi

TMP_INPUT="$(mktemp)"
cleanup() {
  rm -f "$TMP_INPUT"
}
trap cleanup EXIT

python3 - "$SNAPSHOT" > "$TMP_INPUT" <<'PY'
import json
import re
import sys
from pathlib import Path

snapshot = Path(sys.argv[1])
dash_count = 0
WIDTH = 180

RESET = "\033[0m"
GREEN = "\033[38;2;73;255;32m"
DIM_GREEN = "\033[38;2;0;130;42m"
AMBER = "\033[38;2;255;176;0m"
ORANGE = "\033[38;2;255;140;0m"
SELECT = "\033[38;2;32;36;49;48;2;255;140;0m"
TQ = "\033[38;2;32;36;49;48;2;73;255;32m"
link_pending_top_up = False


def pad(line: str, width: int = WIDTH) -> str:
    return line[:width].ljust(width)


def span(text: str, color: str) -> str:
    return f"{color}{text}{RESET}"


def write_plain(line: str, color: str = GREEN) -> None:
    print(span(pad(line), color))


def write_segments(segments: list[tuple[str, str]]) -> None:
    visible = "".join(text for text, _ in segments)
    if len(visible) < WIDTH:
        segments.append((" " * (WIDTH - len(visible)), GREEN))
    out = []
    remaining = WIDTH
    for text, color in segments:
        if remaining <= 0:
            break
        clipped = text[:remaining]
        out.append(span(clipped, color))
        remaining -= len(clipped)
    print("".join(out))


def replace_link_panel(line: str, text: str) -> str:
    match = re.search(r"(.*││)(.*?)(│║.*)", line)
    if not match:
        return line
    width = len(match.group(2))
    return f"{match.group(1)}{text[:width].ljust(width)}{match.group(3)}"


def replace_split_panel_row(line: str, left_text: str, right_text: str) -> str:
    match = re.match(r"^(.*?│)(.*)(││)(.*?)(│║.*)$", line)
    if not match:
        return line
    left_edge, left, divider, right, right_edge = match.groups()
    return (
        f"{left_edge}{left_text[:len(left)].ljust(len(left))}"
        f"{divider}{right_text[:len(right)].ljust(len(right))}{right_edge}"
    )


def color_status_bar() -> None:
    top = "┌" + "─" * (WIDTH - 2) + "┐"
    bottom = "└" + "─" * (WIDTH - 2) + "┘"
    write_plain(top, AMBER)
    parts = [
        ("│", AMBER),
        (" MAESTRO v0.24.0 ", TQ),
        (" -- Welcome -- ", DIM_GREEN),
        ("0 agents (0 active)", GREEN),
        (" -- ", DIM_GREEN),
        ("$0.00/50.00", ORANGE),
        (" -- ", DIM_GREEN),
        ("00:33:59", GREEN),
        (" -- RAM: 32MB -- ", GREEN),
        ("TQ:ON", TQ),
        (" -- ", DIM_GREEN),
        ("[Enter] Activate  [j/k] Navigate", GREEN),
    ]
    visible = sum(len(text) for text, _ in parts)
    parts.append((" " * max(0, WIDTH - visible - 1), GREEN))
    parts.append(("│", AMBER))
    write_segments(parts)
    write_plain(bottom, AMBER)


def color_main(line: str) -> None:
    global link_pending_top_up
    if "state standby" in line:
        line = replace_link_panel(line, "state last sample")
    elif "press [w] measure" in line:
        line = replace_link_panel(line, "v down 6.20 KiB/s")
    elif "mode  manual" in line:
        line = replace_link_panel(line, "  top  6.77 KiB/s")
    elif "sample paused" in line:
        line = replace_split_panel_row(line, "." * 119, "^ up   577 Byte/s")
        link_pending_top_up = True
    elif link_pending_top_up and re.search(r"││\s+│║", line):
        line = replace_link_panel(line, "  top 662 Byte/s")
        link_pending_top_up = False
    line = pad(line)
    stripped = line.strip()

    if not stripped:
        write_plain(line, GREEN)
        return
    if "██" in line or stripped == "v0.24.0":
        write_plain(line, GREEN)
        return
    if "[d]  Dashboard" in line:
        before, rest = line.split("[d]  Dashboard", 1)
        write_segments([(before, GREEN), ("[d]  Dashboard", SELECT), (rest, GREEN)])
        return
    if re.search(r"\[[iprhmqs]\]", line):
        write_plain(line, GREEN)
        return
    if "Release Console" in line or "Press [n]" in line:
        write_plain(line, AMBER)
        return
    if any(title in line for title in ("Internet", "Link", "Mix", "Ref Trend", "Highlights")):
        write_plain(line, AMBER)
        return
    if (
        "state " in line
        or "down " in line
        or "up " in line
        or "top  " in line
        or "sample " in line
    ):
        write_plain(line, GREEN)
        return
    if "[Added]" in line:
        write_plain(line, GREEN)
        return
    if any(ch in line for ch in "▁▂▃▄▅▆▇█"):
        write_plain(line, ORANGE if "Ref Trend" not in line else GREEN)
        return
    if "Add Fix Chg Per Doc" in line or "changes 10" in line:
        write_plain(line, GREEN)
        return
    if "<q> quit" in line:
        write_plain(line, GREEN)
        return
    if set(stripped) <= set("║╔╗╚╝═─┌┐└┘│ "):
        write_plain(line, DIM_GREEN)
        return
    write_plain(line, GREEN)


def color_activity_log() -> None:
    write_plain("", GREEN)
    write_plain("<q> quit  <n> notes  <w> net", GREEN)
    title = "[ Activity Log ]"
    left = (WIDTH - len(title) - 2) // 2
    right = WIDTH - len(title) - 2 - left
    write_plain("┌" + "─" * left + title + "─" * right + "┐", AMBER)
    logs = [
        ("21:22:46 ", GREEN, "[#orphan-prs] ", AMBER, "4 pending PR(s) restored from previous run: #545, #543, #542, #539 -- focus the matching session and press Shift+P to retry", ORANGE),
        ("21:22:46 ", GREEN, "[BYPASS] ", AMBER, "Bypass mode enabled (CLI) -- auto-accepting review corrections", ORANGE),
        ("21:24:47 ", GREEN, "[UPDATE] ", AMBER, "Already on the latest version.", GREEN),
        ("21:53:40 ", GREEN, "[PUSHUP] ", AMBER, "Detected /pushup PR #655; dispatching auto-review", GREEN),
        ("21:54:41 ", GREEN, "[Review] ", AMBER, "Review for PR #655: 0 concern(s) -- press [C] to view", ORANGE),
        ("21:54:41 ", GREEN, "[BYPASS] ", AMBER, "Bypass mode disabled (review-cycle-complete)", GREEN),
    ]
    for time, time_color, label, label_color, msg, msg_color in logs:
        prefix_width = 1 + len(time) + len(label)
        msg_width = max(0, WIDTH - prefix_width - 1)
        msg = msg[:msg_width]
        segments = [
            ("│", DIM_GREEN),
            (time, time_color),
            (label, label_color),
            (msg, msg_color),
        ]
        visible = sum(len(text) for text, _ in segments)
        segments.append((" " * max(0, WIDTH - visible - 1), GREEN))
        segments.append(("│", DIM_GREEN))
        write_segments(segments)
    for _ in range(4):
        write_plain("│" + " " * (WIDTH - 2) + "│", DIM_GREEN)
    write_plain("└" + "─" * (WIDTH - 2) + "┘", DIM_GREEN)
    write_segments([("F1", TQ), (" Help  ", GREEN), ("F10", TQ), (" Exit", GREEN)])


color_status_bar()

for raw_line in snapshot.read_text(encoding="utf-8").splitlines():
    if raw_line == "---":
        dash_count += 1
        continue
    if dash_count < 2 or not raw_line.startswith('"'):
        continue
    line = json.loads(raw_line)
    if line.lstrip().startswith("<q> quit"):
        continue
    color_main(line)

color_activity_log()
PY

freeze "$TMP_INPUT" \
  --language ansi \
  --output "$OUTPUT" \
  --background "#202431" \
  --padding "10,6" \
  --margin 0 \
  --border.radius 0 \
  --border.width 0 \
  --font.family "JetBrains Mono, SFMono-Regular, Menlo, Consolas, monospace" \
  "${FONT_ARGS[@]}" \
  --font.size 12 \
  --line-height 1.15
