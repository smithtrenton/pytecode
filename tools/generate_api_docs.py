"""Generate API reference documentation and validate docstring coverage.

Usage:
    uv run python tools/generate_api_docs.py          # generate + validate
    uv run python tools/generate_api_docs.py --check   # validate only (no HTML)
"""

from __future__ import annotations

import argparse
import importlib
import inspect
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
OUTPUT_DIR = REPO_ROOT / "docs" / "api"

PUBLIC_MODULES: list[str] = [
    "pytecode",
    "pytecode.analysis",
    "pytecode.classfile.constants",
    "pytecode.analysis.hierarchy",
    "pytecode.archive",
    "pytecode.classfile.modified_utf8",
    "pytecode.transforms",
    "pytecode.transforms.rust",
    "pytecode.analysis.verify",
]


def get_public_symbols(module_name: str) -> list[str]:
    """Return the list of public symbol names for a module via ``__all__``."""
    mod = importlib.import_module(module_name)
    return list(getattr(mod, "__all__", []))


def validate_docstrings() -> tuple[int, int, list[str]]:
    """Check that every public symbol has a non-empty docstring.

    Returns:
        A 3-tuple of (total symbols, documented count, list of
        ``module.name`` strings for any undocumented symbols).
    """
    total = 0
    documented = 0
    missing: list[str] = []
    for module_name in PUBLIC_MODULES:
        mod = importlib.import_module(module_name)
        if not inspect.getdoc(mod):
            missing.append(f"{module_name} (module docstring)")
        for name in get_public_symbols(module_name):
            total += 1
            obj = getattr(mod, name)
            owner = getattr(obj, "__module__", None)
            if inspect.getdoc(obj) or (owner is not None and owner != module_name):
                documented += 1
            else:
                missing.append(f"{module_name}.{name}")
    return total, documented, missing


def generate_html() -> None:
    """Run pdoc to produce HTML reference docs in ``docs/api/``."""
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    cmd = [
        sys.executable,
        "-m",
        "pdoc",
        "--output-directory",
        str(OUTPUT_DIR),
        *PUBLIC_MODULES,
    ]
    subprocess.run(cmd, check=True, cwd=str(REPO_ROOT))


def main(argv: list[str] | None = None) -> int:
    """Entry point for the script.

    Args:
        argv: Command-line arguments (defaults to ``sys.argv[1:]``).

    Returns:
        Exit code: 0 on success, 1 if validation fails.
    """
    parser = argparse.ArgumentParser(
        description="Generate API docs and validate docstring coverage.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Validate docstring coverage without generating HTML.",
    )
    args = parser.parse_args(argv)

    total, documented, missing = validate_docstrings()
    pct = (documented / total * 100) if total else 0

    print(f"Docstring coverage: {documented}/{total} ({pct:.1f}%)")

    if missing:
        print(f"\n{len(missing)} undocumented symbol(s):")
        for sym in sorted(missing):
            print(f"  ✗ {sym}")
        return 1

    print("✓ All public symbols are documented.")

    if not args.check:
        print(f"\nGenerating HTML docs in {OUTPUT_DIR} ...")
        generate_html()
        html_count = len(list(OUTPUT_DIR.rglob("*.html")))
        print(f"✓ Generated {html_count} HTML file(s).")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
