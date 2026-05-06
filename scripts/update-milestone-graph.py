#!/usr/bin/env python3
"""Mark an issue complete in a GitHub milestone dependency graph."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass


COMPLETED = "(COMPLETED ✅)"


class MilestoneGraphError(RuntimeError):
    pass


@dataclass(frozen=True)
class UpdateResult:
    description: str
    changed: bool
    already_marked: bool
    issue_marked: bool
    level_completed: bool


def completed_bullet_re(issue: int) -> re.Pattern[str]:
    return re.compile(rf"^(\s*[•-]) ✅ #{issue}\b", re.MULTILINE)


def pending_bullet_re(issue: int) -> re.Pattern[str]:
    return re.compile(rf"^(\s*[•-]) #{issue}\b", re.MULTILINE)


def is_token_char_allowed_before(ch: str | None) -> bool:
    return ch is None or ch.isspace() or ch in "(→∥"


def is_token_char_allowed_after(ch: str | None) -> bool:
    return ch is None or ch.isspace() or ch in ")→∥"


def contains_issue_token(text: str, issue: int) -> bool:
    needle = f"#{issue}"
    start = 0
    while True:
        idx = text.find(needle, start)
        if idx == -1:
            return False
        before = text[idx - 1] if idx > 0 else None
        after_idx = idx + len(needle)
        after = text[after_idx] if after_idx < len(text) else None
        if is_token_char_allowed_before(before) and is_token_char_allowed_after(after):
            return True
        start = idx + len(needle)


def bullet_info(line: str) -> tuple[str, bool] | None:
    match = re.match(r"^(\s*)[•-]\s+(✅\s+)?#\d+\b", line)
    if not match:
        return None
    return match.group(1), match.group(2) is not None


def find_issue_line(lines: list[str], issue: int) -> int:
    pattern = re.compile(rf"^(\s*)[•-]\s+✅\s+#{issue}\b")
    for idx, line in enumerate(lines):
        if pattern.search(line):
            return idx
    raise MilestoneGraphError(f"failed to find marked bullet for #{issue}")


def find_level_header(lines: list[str], issue_line: int) -> tuple[int, int]:
    pattern = re.compile(r"^\s*Level\s+(\d+)\s+—.*:")
    for idx in range(issue_line - 1, -1, -1):
        match = pattern.match(lines[idx])
        if match:
            return idx, int(match.group(1))
    raise MilestoneGraphError("marked issue is not inside a Level block")


def level_block_end(lines: list[str], header_idx: int) -> int:
    pattern = re.compile(r"^\s*Level\s+\d+\s+—.*:")
    for idx in range(header_idx + 1, len(lines)):
        if pattern.match(lines[idx]):
            return idx
    return len(lines)


def roll_up_level_if_complete(
    lines: list[str], issue_line: int
) -> tuple[list[str], int, bool]:
    header_idx, level = find_level_header(lines, issue_line)
    info = bullet_info(lines[issue_line])
    if info is None:
        raise MilestoneGraphError("marked issue line is not a bullet")
    issue_indent, _ = info
    end_idx = level_block_end(lines, header_idx)

    same_indent_bullets: list[bool] = []
    for line in lines[header_idx + 1 : end_idx]:
        current = bullet_info(line)
        if current and current[0] == issue_indent:
            same_indent_bullets.append(current[1])

    if not same_indent_bullets or not all(same_indent_bullets):
        return lines, level, False

    if COMPLETED not in lines[header_idx]:
        lines = lines.copy()
        lines[header_idx] = f"{lines[header_idx]} {COMPLETED}"
    return lines, level, True


def sequence_group_spans(sequence: str) -> list[tuple[int, int, bool, str]]:
    spans: list[tuple[int, int, bool, str]] = []
    idx = 0
    while idx < len(sequence):
        start = sequence.find("(", idx)
        if start == -1:
            break
        end = sequence.find(")", start + 1)
        if end == -1:
            break
        prefix = sequence[max(0, start - 1) : start]
        spans.append((start, end + 1, prefix == "✅", sequence[start + 1 : end]))
        idx = end + 1
    return spans


def update_sequence_line(sequence: str, issue: int, level: int) -> str:
    if not sequence.startswith("Sequence:"):
        return sequence

    for start, end, already_done, content in sequence_group_spans(sequence):
        if not contains_issue_token(content, issue):
            continue
        updated_content = content
        if not re.match(r"\s*L\d+\s*:", updated_content):
            updated_content = f"L{level}: {updated_content}"
        updated_group = f"({updated_content})"
        prefix = "" if already_done else "✅"
        return f"{sequence[:start]}{prefix}{updated_group}{sequence[end:]}"

    needle = f"#{issue}"
    start = 0
    while True:
        idx = sequence.find(needle, start)
        if idx == -1:
            break
        before = sequence[idx - 1] if idx > 0 else None
        after_idx = idx + len(needle)
        after = sequence[after_idx] if after_idx < len(sequence) else None
        if is_token_char_allowed_before(before) and is_token_char_allowed_after(after):
            return f"{sequence[:idx]}✅(L{level}: {needle}){sequence[after_idx:]}"
        start = idx + len(needle)

    raise MilestoneGraphError(f"failed to find #{issue} token in Sequence line")


def update_sequence_if_level_complete(
    lines: list[str], issue: int, level: int, level_completed: bool
) -> list[str]:
    if not level_completed:
        return lines

    for idx, line in enumerate(lines):
        if line.startswith("Sequence:"):
            updated = update_sequence_line(line, issue, level)
            if updated == line:
                return lines
            lines = lines.copy()
            lines[idx] = updated
            return lines

    raise MilestoneGraphError("missing Sequence line")


def update_description(description: str, issue: int) -> UpdateResult:
    if completed_bullet_re(issue).search(description):
        return UpdateResult(description, False, True, True, False)

    updated, count = pending_bullet_re(issue).subn(rf"\1 ✅ #{issue}", description, count=1)
    if count == 0:
        raise MilestoneGraphError(f"no pending bullet found for #{issue}")

    lines = updated.splitlines()
    trailing_newline = updated.endswith("\n")
    issue_line = find_issue_line(lines, issue)
    lines, level, level_completed = roll_up_level_if_complete(lines, issue_line)
    lines = update_sequence_if_level_complete(lines, issue, level, level_completed)
    final = "\n".join(lines)
    if trailing_newline:
        final += "\n"

    return UpdateResult(final, final != description, False, True, level_completed)


def run_gh(args: list[str]) -> str:
    proc = subprocess.run(
        ["gh", *args],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if proc.returncode != 0:
        raise MilestoneGraphError(proc.stderr.strip() or f"gh {' '.join(args)} failed")
    return proc.stdout


def resolve_repo() -> str:
    payload = run_gh(["api", "repos/:owner/:repo"])
    try:
        data = json.loads(payload)
    except json.JSONDecodeError as exc:
        raise MilestoneGraphError("failed to parse repository metadata from gh") from exc
    full_name = data.get("full_name")
    if not isinstance(full_name, str) or "/" not in full_name:
        raise MilestoneGraphError("gh repository metadata did not include full_name")
    return full_name


def fetch_description(repo: str, milestone: int) -> str:
    payload = run_gh(["api", f"repos/{repo}/milestones/{milestone}"])
    try:
        data = json.loads(payload)
    except json.JSONDecodeError as exc:
        raise MilestoneGraphError("failed to parse milestone payload from gh") from exc
    description = data.get("description")
    if not isinstance(description, str):
        raise MilestoneGraphError("milestone payload did not include a string description")
    return description


def patch_description(repo: str, milestone: int, description: str) -> None:
    run_gh(
        [
            "api",
            f"repos/{repo}/milestones/{milestone}",
            "-X",
            "PATCH",
            "-f",
            f"description={description}",
        ]
    )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Mark an issue complete in a milestone dependency graph."
    )
    parser.add_argument("positional", nargs="*", metavar="N")
    parser.add_argument("--milestone", type=int, help="GitHub milestone number")
    parser.add_argument("--issue", type=int, help="Issue number to mark complete")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the would-be PATCH body instead of calling the GitHub API",
    )
    args = parser.parse_args(argv)

    if args.positional:
        if len(args.positional) != 2:
            parser.error("positional form requires exactly: <milestone> <issue>")
        if args.milestone is not None or args.issue is not None:
            parser.error("use either positional arguments or --milestone/--issue, not both")
        args.milestone = int(args.positional[0])
        args.issue = int(args.positional[1])

    if args.milestone is None or args.issue is None:
        parser.error("--milestone and --issue are required")
    if args.milestone <= 0 or args.issue <= 0:
        parser.error("--milestone and --issue must be positive integers")
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        repo = resolve_repo()
        original = fetch_description(repo, args.milestone)
        result = update_description(original, args.issue)
        if result.already_marked:
            print(f"already marked: issue #{args.issue} is complete in milestone graph")
            return 0
        if args.dry_run:
            print(json.dumps({"description": result.description}, ensure_ascii=False))
            return 0

        patch_description(repo, args.milestone, result.description)
        verified = fetch_description(repo, args.milestone)
        if not completed_bullet_re(args.issue).search(verified):
            raise MilestoneGraphError("verification failed: updated bullet was not present")
        if result.level_completed and result.description != verified:
            raise MilestoneGraphError("verification failed: milestone description differs")
        print(f"marked issue #{args.issue} in milestone #{args.milestone}")
        return 0
    except MilestoneGraphError as exc:
        print(f"update-milestone-graph: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
