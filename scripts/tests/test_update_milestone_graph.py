from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "update-milestone-graph.py"
FIXTURES = Path(__file__).resolve().parent / "fixtures"

spec = importlib.util.spec_from_file_location("update_milestone_graph", SCRIPT)
update_milestone_graph = importlib.util.module_from_spec(spec)
assert spec.loader is not None
sys.modules[spec.name] = update_milestone_graph
spec.loader.exec_module(update_milestone_graph)


def fixture(name: str) -> str:
    return (FIXTURES / name).read_text()


def test_idempotent_rerun_exits_zero_without_patch(monkeypatch, capsys):
    description = fixture("milestone-fresh.md").replace("• #554", "• ✅ #554")
    patch_called = False

    monkeypatch.setattr(update_milestone_graph, "resolve_repo", lambda: "owner/repo")
    monkeypatch.setattr(
        update_milestone_graph, "fetch_description", lambda _repo, _milestone: description
    )

    def fail_patch(_repo, _milestone, _description):
        nonlocal patch_called
        patch_called = True

    monkeypatch.setattr(update_milestone_graph, "patch_description", fail_patch)

    assert update_milestone_graph.main(["--milestone", "1", "--issue", "554"]) == 0
    assert "already marked" in capsys.readouterr().out
    assert not patch_called


def test_anchored_bullet_replace_does_not_rewrite_prose():
    before = (
        "Depends on #554 in prose.\n\n"
        "Level 0 — workflow mechanization:\n"
        "• #554 chore(workflow): scripted milestone dependency-graph update\n"
        "A note blocks #554 but is not a bullet.\n\n"
        "Sequence: (#554)\n"
    )

    result = update_milestone_graph.update_description(before, 554)

    assert "Depends on #554 in prose." in result.description
    assert "A note blocks #554 but is not a bullet." in result.description
    assert "• ✅ #554 chore(workflow)" in result.description


def test_level_rollup_at_boundary_and_idempotent_second_run():
    before = fixture("milestone-rolled-up.md")

    result = update_milestone_graph.update_description(before, 12)
    assert "Level 2 — final polish: (COMPLETED ✅)" in result.description
    assert "Sequence: ✅(L2: #10 ∥ #11 ∥ #12)" in result.description

    second = update_milestone_graph.update_description(result.description, 12)
    assert second.already_marked
    assert result.description.count("(COMPLETED ✅)") == 1


def test_sequence_token_boundary_does_not_match_52_inside_521():
    before = fixture("milestone-token-boundary.md")

    result = update_milestone_graph.update_description(before, 521)

    assert "• ✅ #52 smaller issue" in result.description
    assert "• ✅ #521 larger issue" in result.description
    assert "This prose mentions #521 and #52 but neither should be rewritten." in result.description
    assert "Sequence: ✅(L0: #52) → ✅(L1: #521)" in result.description


def test_dry_run_prints_patch_body(monkeypatch, capsys):
    monkeypatch.setattr(update_milestone_graph, "resolve_repo", lambda: "owner/repo")
    monkeypatch.setattr(
        update_milestone_graph,
        "fetch_description",
        lambda _repo, _milestone: fixture("milestone-fresh.md"),
    )

    def fail_patch(_repo, _milestone, _description):
        raise AssertionError("dry-run must not PATCH")

    monkeypatch.setattr(update_milestone_graph, "patch_description", fail_patch)

    assert (
        update_milestone_graph.main(["--milestone", "1", "--issue", "554", "--dry-run"])
        == 0
    )
    body = json.loads(capsys.readouterr().out)
    assert body["description"].count("• ✅ #554") == 1


def test_verification_failure_path_returns_nonzero(monkeypatch, capsys):
    before = fixture("milestone-fresh.md")
    fetches = iter([before, before])

    monkeypatch.setattr(update_milestone_graph, "resolve_repo", lambda: "owner/repo")
    monkeypatch.setattr(
        update_milestone_graph,
        "fetch_description",
        lambda _repo, _milestone: next(fetches),
    )
    monkeypatch.setattr(update_milestone_graph, "patch_description", lambda *_args: None)

    assert update_milestone_graph.main(["--milestone", "1", "--issue", "554"]) == 1
    assert "verification failed" in capsys.readouterr().err
