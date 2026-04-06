"""Compare Cython vs pure-Python performance on a JAR file.

Runs each pipeline stage under both backends (Cython extensions and
pure-Python fallback) in isolated subprocesses, then prints a comparison
table and optionally writes a JSON report.

Usage:
    uv run python tools/benchmark_cython.py 225.jar
    uv run python tools/benchmark_cython.py 225.jar --stages class-parse class-write
    uv run python tools/benchmark_cython.py 225.jar --iterations 5 --regression-threshold 0.95
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import sys
import textwrap
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

ALL_STAGES = (
    "jar-read",
    "jar-classify",
    "class-parse",
    "model-lift",
    "model-lower",
    "class-write",
)

# Inline Python snippet executed in each subprocess.  It imports the
# pipeline module, builds stage inputs once, then runs the requested
# stage ``iterations`` times and prints a JSON dict mapping each stage
# name to a list of elapsed seconds.
_SUBPROCESS_SCRIPT = textwrap.dedent("""\
    import json, os, sys, time
    from pathlib import Path

    # Ensure the repo root is on sys.path so ``tools.profile_jar_pipeline``
    # can resolve its helper imports even when invoked as a bare snippet.
    repo_root = Path({repo_root!r})
    if str(repo_root) not in sys.path:
        sys.path.insert(0, str(repo_root))

    from tools.profile_jar_pipeline import ProfileInputs, STAGE_BUILDERS

    jar_path = Path({jar_path!r})
    stage_names = {stage_names!r}
    iterations = {iterations!r}

    inputs = ProfileInputs(jar_path=jar_path)
    results: dict[str, list[float]] = {{}}

    for stage_name in stage_names:
        timings: list[float] = []
        for _ in range(iterations):
            prepared = STAGE_BUILDERS[stage_name](inputs)
            t0 = time.perf_counter()
            prepared.workload()
            t1 = time.perf_counter()
            timings.append(t1 - t0)
        results[stage_name] = timings

    json.dump(results, sys.stdout)
""")


def _run_backend(
    jar_path: Path,
    stages: list[str],
    iterations: int,
    *,
    block_cython: bool,
) -> dict[str, list[float]]:
    """Run stages in a subprocess and return per-stage timing lists."""
    env = os.environ.copy()
    if block_cython:
        env["PYTECODE_BLOCK_CYTHON"] = "1"
    else:
        env.pop("PYTECODE_BLOCK_CYTHON", None)

    script = _SUBPROCESS_SCRIPT.format(
        repo_root=str(REPO_ROOT),
        jar_path=str(jar_path),
        stage_names=stages,
        iterations=iterations,
    )

    result = subprocess.run(
        [sys.executable, "-c", script],
        capture_output=True,
        text=True,
        env=env,
    )

    if result.returncode != 0:
        raise RuntimeError(f"Subprocess failed (block_cython={block_cython}):\n{result.stderr}")

    return json.loads(result.stdout)


def _check_cython_available() -> bool:
    """Return True if Cython extensions can be imported."""
    env = os.environ.copy()
    env.pop("PYTECODE_BLOCK_CYTHON", None)

    result = subprocess.run(
        [
            sys.executable,
            "-c",
            (
                "import pytecode._internal.cython_import as ci; "
                "m = ci.import_cython_module("
                "'pytecode._internal._bytes_utils_cy', "
                "'pytecode._internal._bytes_utils_py'); "
                "import sys; "
                "sys.exit(0 if '_cy' in m.__name__ else 1)"
            ),
        ],
        capture_output=True,
        env=env,
    )
    return result.returncode == 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="Compare Cython vs pure-Python performance on a JAR file.",
    )
    parser.add_argument(
        "jar",
        nargs="?",
        type=Path,
        default=REPO_ROOT / "225.jar",
        help="Path to the JAR file to benchmark (default: 225.jar in repo root).",
    )
    parser.add_argument(
        "--stages",
        nargs="+",
        choices=ALL_STAGES,
        default=None,
        help="Stages to benchmark.  Defaults to all pipeline stages.",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=3,
        help="Number of timed runs per stage per backend (takes median).  Default: 3.",
    )
    parser.add_argument(
        "--regression-threshold",
        type=float,
        default=1.0,
        help=("Fail (exit 1) if any Cython stage ratio (cython/python) exceeds this value.  Default: 1.0."),
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=REPO_ROOT / "output" / "profiles" / "cython-comparison.json",
        help="Path for the JSON comparison report.",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    """Entry point for the Cython benchmark comparison."""
    args = parse_args(argv)
    jar_path: Path = args.jar.expanduser().resolve()
    stages: list[str] = list(args.stages or ALL_STAGES)
    iterations: int = args.iterations
    threshold: float = args.regression_threshold
    output_path: Path = args.output.expanduser()

    if not jar_path.is_file():
        print(f"error: JAR file not found: {jar_path}", file=sys.stderr)
        return 1

    # -- check Cython availability -----------------------------------------
    cython_available = _check_cython_available()
    if not cython_available:
        print(
            "warning: Cython extensions are not built; "
            "skipping Cython runs (only pure-Python timings will be collected).",
            file=sys.stderr,
        )

    # -- run backends ------------------------------------------------------
    print(f"jar: {jar_path}")
    print(f"stages: {', '.join(stages)}")
    print(f"iterations: {iterations}")
    print()

    print("Running pure-Python backend …")
    python_timings = _run_backend(
        jar_path,
        stages,
        iterations,
        block_cython=True,
    )

    cython_timings: dict[str, list[float]] | None = None
    if cython_available:
        print("Running Cython backend …")
        cython_timings = _run_backend(
            jar_path,
            stages,
            iterations,
            block_cython=False,
        )

    # -- build comparison --------------------------------------------------
    stage_results: dict[str, dict[str, object]] = {}
    any_regression = False

    header = f"{'Stage':<16} {'Python (s)':>12} {'Cython (s)':>12} {'Ratio (Cy/Py)':>14} {'Status':>8}"
    print()
    print(header)
    print("-" * len(header))

    for stage in stages:
        py_median = statistics.median(python_timings[stage])

        if cython_timings is not None:
            cy_median = statistics.median(cython_timings[stage])
            ratio = cy_median / py_median if py_median > 0 else float("inf")
            status = "ok" if ratio <= threshold else "REGRESS"
            if ratio > threshold:
                any_regression = True
        else:
            cy_median = None
            ratio = None
            status = "skip"

        cy_display = f"{cy_median:.6f}" if cy_median is not None else "n/a"
        ratio_display = f"{ratio:.3f}" if ratio is not None else "n/a"

        print(f"{stage:<16} {py_median:>12.6f} {cy_display:>12} {ratio_display:>14} {status:>8}")

        entry: dict[str, object] = {
            "python_seconds": round(py_median, 6),
        }
        if cy_median is not None:
            entry["cython_seconds"] = round(cy_median, 6)
            entry["ratio"] = round(ratio, 6)  # type: ignore[arg-type]
            entry["status"] = status
        else:
            entry["cython_seconds"] = None
            entry["ratio"] = None
            entry["status"] = status

        stage_results[stage] = entry

    print()

    # -- write JSON --------------------------------------------------------
    report = {
        "jar": str(jar_path),
        "iterations": iterations,
        "stages": stage_results,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"report: {output_path}")

    if any_regression:
        print(
            f"FAIL: one or more stages exceeded regression threshold ({threshold})",
            file=sys.stderr,
        )
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
