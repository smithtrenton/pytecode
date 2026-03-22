from __future__ import annotations

import hashlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
RUN_SCRIPT = REPO_ROOT / "run.py"
SAMPLE_JAR = REPO_ROOT / "225.jar"
EXPECTED_OUTPUT = REPO_ROOT / "output" / "225"


def snapshot_tree(root: Path) -> dict[str, str]:
    return {
        path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


class RunScriptSmokeTests(unittest.TestCase):
    def test_run_script_matches_checked_in_output(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            jar_path = temp_path / SAMPLE_JAR.name
            shutil.copy2(SAMPLE_JAR, jar_path)

            completed = subprocess.run(
                [sys.executable, str(RUN_SCRIPT), str(jar_path)],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
                check=True,
            )

            self.assertIn("classes: 1228", completed.stdout)
            self.assertIn("other_files: 3", completed.stdout)

            actual_output = temp_path / "output" / jar_path.stem
            self.assertTrue(actual_output.is_dir())
            self.assertEqual(snapshot_tree(EXPECTED_OUTPUT), snapshot_tree(actual_output))


if __name__ == "__main__":
    unittest.main()
