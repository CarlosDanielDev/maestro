"""Schema validation for scripts/coverage-tiers.yml and (later)
scripts/architecture-layers.yml.

Uses `yq` via subprocess since the project avoids pyyaml (stdlib-only
Python policy).
"""
import json
import subprocess
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def yq_json(yaml_path: Path, expr: str = ".") -> object:
    """Return the parsed YAML as a Python object via yq + json round-trip."""
    result = subprocess.run(
        ["yq", "eval", "-o", "json", expr, str(yaml_path)],
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(result.stdout)


class CoverageTiersTest(unittest.TestCase):
    manifest_path = REPO_ROOT / "scripts" / "coverage-tiers.yml"

    @classmethod
    def setUpClass(cls):
        cls.data = yq_json(cls.manifest_path)

    def test_top_level_has_tiers(self):
        self.assertIn("tiers", self.data)

    def test_required_tiers_present(self):
        tiers = self.data["tiers"]
        self.assertIn("core", tiers)
        self.assertIn("tui", tiers)
        self.assertIn("excluded", tiers)

    def test_core_has_floor_and_aspiration(self):
        core = self.data["tiers"]["core"]
        self.assertIn("floor", core)
        self.assertIn("aspiration", core)
        self.assertIsInstance(core["floor"], (int, float))
        self.assertIsInstance(core["aspiration"], (int, float))
        self.assertGreaterEqual(core["aspiration"], core["floor"])

    def test_tui_has_floor(self):
        tui = self.data["tiers"]["tui"]
        self.assertIn("floor", tui)
        self.assertIsInstance(tui["floor"], (int, float))

    def test_excluded_has_paths(self):
        excluded = self.data["tiers"]["excluded"]
        self.assertIn("paths", excluded)
        self.assertIsInstance(excluded["paths"], list)
        self.assertGreater(len(excluded["paths"]), 0)

    def test_all_paths_are_strings(self):
        for tier_name, tier in self.data["tiers"].items():
            for path in tier.get("paths", []):
                self.assertIsInstance(path, str, f"tier={tier_name}")


if __name__ == "__main__":
    unittest.main()
