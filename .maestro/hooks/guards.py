#!/usr/bin/env python3
"""PreToolUse guard hooks for projects that run setwave waves.

Claude Code pipes one JSON event to each hook on stdin; a guard answers through
its exit code alone: 2 refuses the tool call (stderr goes back to the agent, so
the refusal names the safe alternative), 0 passes. Everything a guard cannot
parse, or does not survive, is a pass too: the guards are fail-open by
construction — a broken guard must never wedge the loop it guards. What is
refused here is what the wave prompt already forbids in prose (templates/agent.md);
the guard is the environment-level backstop for the agent that ignores the prose.

    python3 guards.py <attribution|forbidden-git|secret-read|dispatch-contract|background-gate>   # judge one stdin event
    python3 guards.py <name> --ping                                                               # liveness probe for `wave hooks status`

Stdlib only. The command being judged is never executed — a guard reads strings
and, for attribution, the body/message file the command itself names.
"""
import json
import re
import sys
from pathlib import Path

# The same three patterns `wave verify` flags in PRs (ATTRIBUTION in scripts/wave.py): the
# trailer must never be written, and verify only sees it after the fact.
ATTRIBUTION = re.compile(r"co-authored-by:\s*claude|generated with \[?claude code|noreply@anthropic\.com", re.I)
# message/PR-body files a command names inline: their content is part of what is being committed
BODY_FILE = re.compile(r"(?:--body-file|--notes-file)[= ](\S+)")
# git's shorthand for a message file; grep's `-F` is a pattern flag, so it counts only in a git/gh command
MESSAGE_FILE = re.compile(r"(?:^|\s)-F[= ](\S+)")
# the command text itself is a message only when the command writes one; a grep or a log search
# naming the pattern is reading, not writing, and stays open
WRITES_MESSAGE = re.compile(r"\bgit\s+(?:commit|tag)\b|\bgh\s+(?:pr|issue|release)\s+(?:create|edit|comment)\b")

# each denial names the safe alternative, so the refusal is also the documentation
FORBIDDEN = [
    (re.compile(r"\bgit\s+gc\b[^|;&]*--prune\b"), "let unreachable objects expire on their own instead"),
    (re.compile(r"\bgit\s+reflog\s+expire\b"), "the reflog is the undo log: let its entries expire on their own instead"),
    (re.compile(r"\bgit\s+stash\b"), "commit the work to a branch instead"),
    (re.compile(r"\bgit\s+reset\s+--hard\b"), "discard file edits with `git checkout -- <paths>`, undo commits by reverting them instead"),
    (re.compile(r"\bgit\s+clean\s+-\w*f"), "list first with `git clean -n`, then delete the files you actually named instead"),
    (re.compile(r"\bgit\s+push\b[^|;&]*(?:--force(?:-with-lease)?\b|\s-f\b)"), "a force push rewrites shared history: push a new branch instead"),
    (re.compile(r"\bgit\s+branch\s+-\w*D\b"), "`git branch -d` (lowercase) refuses unmerged work — use it instead"),
]

# secret paths: setwave's own registry carries account tokens; the rest is the common denylist
SECRET_PATHS = re.compile(r"\.env\b|auth\.json\b|\bcredentials\b|setwave[/\\]repos\.json\b", re.I)
SHOW_VERB = re.compile(r"\b(cat|head|tail|less|more|vim|nvim|vi|nano|emacs|sed|awk|xxd|od|base64|strings|diff|cmp|cp|scp|rsync)\b")
GREP_LIKE = re.compile(r"\b(grep|rg|ag|ack)\b")
COUNT_FLAG = re.compile(r"-[a-zA-Z]*[clq]\b")

# the dispatch contract: what every prompt must carry because the agent sees the prompt
# and nothing else (the section headings of templates/agent.md, matched loosely)
CONTRACT = [
    (re.compile(r"^#{1,6}.*\b(?:onde|where)\b", re.I), "where the work happens (the worktree)"),
    (re.compile(r"^#{1,6}.*\b(?:tarefa|task)\b", re.I), "the task itself"),
    (re.compile(r"^#{1,6}.*\bgate\b", re.I), "the gate to run before claiming done"),
    (re.compile(r"^#{1,6}.*\b(?:entrega|delivery|report)\b", re.I), "what to deliver and how"),
]
LEDGER = "the Done-when ledger the PR must carry"
EXEMPT_SUBAGENTS = {"fork"}  # a fork inherits the parent's whole conversation: the premise does not hold
# the dispatch stamp dispatch writes into each wave worktree: its presence in the event's cwd is what
# makes this seat a wave agent's, and the background-gate judge applies only there — the orchestrator's
# own main checkout has no stamp and may background `wave agents --watch` freely
DISPATCH_STAMP = ".setwave.json"


def judge_attribution(event):
    """No AI attribution line in a commit message or PR body command."""
    if event.get("tool_name") != "Bash":
        return None
    command = (event.get("tool_input") or {}).get("command") or ""
    texts = [command] if WRITES_MESSAGE.search(command) else []
    paths = BODY_FILE.findall(command)
    if re.search(r"\b(?:git|gh)\b", command):
        paths += MESSAGE_FILE.findall(command)
    for path in paths:  # what the file holds is what the command commits
        try:
            texts.append(Path(path).expanduser().read_text(errors="ignore")[:1_000_000])
        except OSError:
            pass  # unreadable here is not evidence of a trailer: fail open, `verify` re-checks later
    for text in texts:
        m = ATTRIBUTION.search(text)
        if m:
            return (f"this command carries AI attribution ({m.group(0)}) — commits and PR bodies in this project "
                    "carry none: drop the trailer and run the command again, `wave verify` refuses the PR otherwise")
    return None


def judge_forbidden_git(event):
    """None of the destructive commands survives Bash; the refusal carries the way back. Global options
    (`git -c x=y reset --hard`), pager flags and command shims (`sudo|command|env git`, `VAR=v git`) are
    peeled before matching: they change nothing about what the command does."""
    if event.get("tool_name") != "Bash":
        return None
    statement: str
    for statement in re.split(r"&&|\|\||[;\n]", (event.get("tool_input") or {}).get("command") or ""):
        for pattern, instead in FORBIDDEN:
            m = pattern.search(git_normalized(statement))
            if m:
                return f"`{m.group(0).strip()}` is refused: {instead}"
        if rm_recursive_force(statement):
            return "`rm -rf` is refused: delete the specific paths you named, without -r and -f together, instead"
    return None


GIT_SHIMS = re.compile(r"^\s*(?:\w+=\S+\s+)*(?:sudo|command|env|nohup)?\s*git\b")
GIT_GLOBAL_OPTION = re.compile(r"\s+(?:-[cC]\s+\S+|--[\w-]+=(?:\"[^\"]*\"|'[^']*'|\S+)|--[\w-]+)(?=\s|$)")


def git_normalized(statement: str) -> str:
    """`command git -c x=y --no-pager reset --hard` reads as `git reset --hard`: leading VAR=... and shim
    words go, and every option token between `git` and the first subcommand token goes with them — global
    options only, the subcommand and everything after it are left verbatim for the patterns."""
    m = GIT_SHIMS.match(statement)
    if not m:
        return statement
    rest = statement[m.end():]
    while True:
        n = GIT_GLOBAL_OPTION.match(rest)
        if not n:
            break
        rest = rest[n.end():]
    return statement[:m.start()] + "git" + rest


def rm_recursive_force(statement: str) -> bool:
    m = re.search(r"\brm\b", statement)
    if not m:
        return False
    flags = re.findall(r"(?:^|\s)-{1,2}([A-Za-z][\w-]*)", statement[m.end():])
    has_r = any("r" in f.lower() or f == "recursive" for f in flags)
    has_f = any("f" in f.lower() or f == "force" for f in flags)
    return has_r and has_f


# the allowlist a secret path must survive: metadata only — names, sizes, counts, presence (#142 review:
# a denylist of reader verbs loses to `python3 -c`, `nl`, `tr < file`, and every reader not yet invented)
METADATA_VERB = re.compile(
    r"\b(?:ls|stat|file|wc|test|check-ignore)\b|\[\s"
    r"|\bgrep\b[^|;&]*\s-\w*[cq]\w*\b|\bgrep\b[^|;&]*(?:--count|--quiet)")


def judge_secret_read(event):
    """When a statement names a secret path, only its metadata may cross: the allowlist above decides, and
    everything else — any reader verb, known or not — is refused."""
    if event.get("tool_name") != "Bash":
        return None
    command = (event.get("tool_input") or {}).get("command") or ""
    statement: str
    for statement in re.split(r"&&|\|\||[;\n]", command):
        m = SECRET_PATHS.search(statement)
        if not m:
            continue
        last = statement.split("|")[-1]  # a pipeline answers with its last stage: `cat .env | wc -l` is a count
        if METADATA_VERB.search(last):
            continue
        return (f"this command reads a secret ({m.group(0)}), and only its metadata is safe here — use "
                "`ls -la`, `stat`, `file`, `wc -l`, `test -f` or `grep -c`/`grep -q` instead")
    return None


def judge_dispatch_contract(event):
    """An Agent dispatch carries the sections of the contract: the prompt is all the agent sees."""
    if event.get("tool_name") != "Task":
        return None
    tool_input = event.get("tool_input") or {}
    if (tool_input.get("subagent_type") or "") in EXEMPT_SUBAGENTS:
        return None
    prompt = tool_input.get("prompt")
    if not isinstance(prompt, str) or not prompt.strip():
        return None  # nothing to judge; the dispatch fails on its own terms
    headings = [l for l in prompt.splitlines() if l.lstrip().startswith("#")]
    missing = [label for pattern, label in CONTRACT if not any(pattern.search(h) for h in headings)]
    if "done when" not in prompt.lower():
        missing.append(LEDGER)
    if missing:
        return ("the prompt is all the agent sees, and it lacks the dispatch contract: " + ", ".join(missing)
                + " — build the prompt from templates/agent.md (where, task, gate, delivery, Done when)")
    return None


def judge_background_gate(event):
    """A backgrounded Bash call inside a dispatch worktree ends the turn while the gate still runs, and an
    ended turn never wakes up on its own (#140): refuse it — the gate runs in foreground, one step per call.
    The tool's `run_in_background` flag is not the only door: a trailing `&` (not `&&`), `nohup`, `disown`
    or `setsid` in the command string backgrounds the same gate past the flag, and is refused too.
    Only a seat carries the stamp, so the orchestrator's own checkout stays open. (Known limit, by design:
    the stamp sits in the agent-writable worktree, so an agent that erases it escapes this judge — the
    guards are fail-open, and `wave verify` plus the judge still gate the PR.)"""
    if event.get("tool_name") != "Bash":
        return None
    tool_input = event.get("tool_input") or {}
    flagged = bool(tool_input.get("run_in_background"))
    cwd = event.get("cwd")
    if not cwd:
        return None  # no cwd in the event: nothing says this is a seat, and a guard never guesses
    try:
        stamped = (Path(cwd) / DISPATCH_STAMP).exists()
    except OSError:
        return None  # unreadable cwd is no evidence: fail open, the prose still forbids it
    if not stamped:
        return None
    command = tool_input.get("command") or ""
    if flagged or shelled_to_background(command):
        return ("a backgrounded command inside a wave worktree ends the turn while the gate still runs, and "
                "nothing wakes the turn: run the gate in foreground, one step per call, and never end the "
                "turn while it runs")
    return None


def shelled_to_background(command: str) -> bool:
    """True when the command string backgrounds, detaches or re-parses work at the shell level: an unquoted
    `&` that is not `&&` (nor a redirect like `2>&1` / `&>`), nohup / disown / setsid / coproc, a detached
    or multiplexed session (tmux, screen, at, batch, parallel), or a re-parsing launcher (eval, any *sh)
    with an `&` anywhere in the command — the launcher's payload is judged on a quote-NORMALIZED string
    (quote characters deleted after escape pairs), so `ba'sh' -c 'x &'` and `bash "-c"` spellings read as
    what they run. Known limit, documented: a determined daemonizer (ssh -f, systemd-run, `| at now`) with
    no flagged token still passes — the judge is a backstop, and verify plus the judge gate the PR."""
    stripped = command.replace("\\'", " ").replace('\\"', " ")  # an escaped quote is a literal char: the span it opens is not a quoted span
    spans_gone = re.sub(r"'[^']*'|\"[^\"]*\"", " ", stripped)  # a quoted `&` is data for the outer shell's operator scan
    ops = spans_gone.replace("&&", " ").replace("&>", " ").replace(">&", " ")
    if re.search(r"&", ops):
        return True
    if re.search(r"\b(?:nohup|disown|setsid|coproc|tmux|screen|at|batch|parallel)\b", ops):
        return True
    normalized = re.sub(r"[\"']", "", stripped)  # the launcher reads through the quotes: judge its spelling dequoted
    if re.search(r"\beval\b|\b(?:ba|z|da|k|mk|s)?sh\b", normalized) and "&" in command:
        return True
    return False


JUDGES = {"attribution": judge_attribution, "forbidden-git": judge_forbidden_git,
          "secret-read": judge_secret_read, "dispatch-contract": judge_dispatch_contract,
          "background-gate": judge_background_gate}


def run_guard(name: str, raw: str) -> int:
    try:
        verdict = JUDGES[name](json.loads(raw) if raw.strip() else {})
    except Exception:
        return 0  # fail open: a guard that cannot run, or cannot read its event, never blocks
    if verdict:
        print(f"{name}-guard: {verdict}", file=sys.stderr)
        return 2
    return 0


def main(argv=None, stdin=None) -> int:
    args = sys.argv[1:] if argv is None else argv
    if "--ping" in args:
        print("pong")
        return 0
    raw = sys.stdin.read() if stdin is None else stdin
    return run_guard(args[0] if args else "", raw)


if __name__ == "__main__":
    sys.exit(main())
