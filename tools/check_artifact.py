"""Install a wheel in fresh environments and test it outside the checkout."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    """Test every requested CPython version against one native wheel."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("wheel", type=Path)
    parser.add_argument("--python", nargs="+", default=["3.12", "3.13", "3.14"])
    args = parser.parse_args()
    wheel = Path(args.wheel).resolve()
    uv = shutil.which("uv")
    if uv is None:
        raise RuntimeError("uv must be on PATH")
    smoke = Path(__file__).with_name("artifact_smoke.py")
    with tempfile.TemporaryDirectory(prefix="pytecode-installed-") as directory:
        root = Path(directory)
        shutil.copyfile(smoke, root / smoke.name)
        for version in args.python:
            environment = root / f"python-{version}"
            subprocess.run([uv, "venv", "--python", version, str(environment)], cwd=root, check=True)
            python = environment / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
            subprocess.run([uv, "pip", "install", "--python", str(python), str(wheel)], cwd=root, check=True)
            subprocess.run([str(python), "-I", smoke.name], cwd=root, check=True)


if __name__ == "__main__":
    main()
