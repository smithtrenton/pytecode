from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = REPO_ROOT / "tools" / "parse_wiki_instructions.py"
FIXTURE_PATH = REPO_ROOT / "tests" / "fixtures" / "wiki_instructions_sample.html"
EXPECTED_PATH = REPO_ROOT / "tests" / "fixtures" / "wiki_instructions_expected.txt"


class ParseWikiInstructionsTests(unittest.TestCase):
    def test_fixture_output_matches_expected_rendering(self) -> None:
        completed = subprocess.run(
            [sys.executable, str(SCRIPT_PATH), "--input-html", str(FIXTURE_PATH)],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=True,
        )

        self.assertEqual(EXPECTED_PATH.read_text(encoding="utf-8"), completed.stdout)


if __name__ == "__main__":
    unittest.main()
