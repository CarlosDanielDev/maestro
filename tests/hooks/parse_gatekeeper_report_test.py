"""Unit tests for .claude/hooks/parse_gatekeeper_report.py."""
import importlib.util
import unittest
from pathlib import Path

HOOK_PATH = Path(__file__).resolve().parents[2] / ".claude" / "hooks" / "parse_gatekeeper_report.py"

def _load_parser():
    spec = importlib.util.spec_from_file_location("parse_gatekeeper_report", HOOK_PATH)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(f"Parser not found at {HOOK_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

class ParserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.parser = _load_parser()

    def test_parses_valid_pass_report(self):
        text = """
Prose above the fence.

```json gatekeeper
{
  "report_version": 1,
  "status": "PASS",
  "task_type": "implementation",
  "dor": {"passed": true, "missing_sections": [], "weak_sections": []},
  "blockers": {"passed": true, "open": []},
  "contracts": {"passed": true, "missing": []},
  "remediation": {"comment_body": "", "labels_to_add": []},
  "reasons": []
}
```

Prose below the fence.
"""
        report = self.parser.extract_report(text)
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["task_type"], "implementation")
        self.assertTrue(report["dor"]["passed"])

if __name__ == "__main__":
    unittest.main()
