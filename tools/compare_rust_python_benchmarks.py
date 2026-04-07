from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections.abc import Sequence
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments for Rust-vs-Python benchmark comparison."""

    from tools.profile_jar_pipeline import positive_int

    parser = argparse.ArgumentParser(
        description="Compare isolated Rust and Python pytecode stage benchmarks for one jar.",
    )
    parser.add_argument("--jar", type=Path, required=True, help="Path to the benchmark jar.")
    parser.add_argument(
        "--iterations",
        type=positive_int,
        default=5,
        help="Number of timing samples to collect per stage for each implementation.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Optional JSON file to write with the comparison payload.",
    )
    return parser.parse_args(argv)


def run_rust_benchmark(jar_path: Path, iterations: int) -> dict[str, Any]:
    """Run the Rust benchmark CLI in release mode and return the parsed JSON payload."""

    command = [
        "cargo",
        "run",
        "--release",
        "-q",
        "-p",
        "pytecode-cli",
        "--",
        "bench-smoke",
        "--jar",
        str(jar_path),
        "--iterations",
        str(iterations),
    ]
    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"Rust benchmark failed ({' '.join(command)}):\n{completed.stderr.strip()}")
    return json.loads(completed.stdout)


def stage_speedup(rust_median: int, python_median: int) -> float | None:
    """Return Python/Rust median ratio so values above 1 mean Rust is faster."""

    if rust_median == 0:
        return None if python_median > 0 else 1.0
    return round(python_median / rust_median, 3)


def compare_reports(rust_payload: dict[str, Any], python_jar_report: dict[str, Any]) -> dict[str, Any]:
    """Build a stage-by-stage comparison payload from Rust and Python benchmark reports."""

    python_by_stage = {stage_report["name"]: stage_report for stage_report in python_jar_report["stage_reports"]}
    stages: list[dict[str, Any]] = []
    for rust_stage in rust_payload["stage_reports"]:
        stage_name = rust_stage["stage"]
        python_stage = python_by_stage[stage_name]
        rust_median = int(rust_stage["median_milliseconds"])
        python_median = int(python_stage["median_milliseconds"])
        stages.append(
            {
                "stage": stage_name,
                "rust": rust_stage,
                "python": python_stage,
                "rust_speedup_vs_python": stage_speedup(rust_median, python_median),
            }
        )

    return {
        "jar": rust_payload["jar"],
        "iterations": rust_payload["iterations"],
        "stages": stages,
    }


def write_json(path: Path, payload: dict[str, Any]) -> None:
    """Write JSON output to *path* with a stable formatting style."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def main(argv: Sequence[str] | None = None) -> int:
    """Run isolated Rust and Python benchmarks for one jar and emit comparison JSON."""

    from tools.benchmark_jar_pipeline import benchmark_jar
    from tools.profile_jar_pipeline import STAGE_BUILDERS

    args = parse_args(argv)
    jar_path = args.jar.expanduser().resolve()
    rust_payload = run_rust_benchmark(jar_path, args.iterations)
    python_report = benchmark_jar(jar_path, list(STAGE_BUILDERS), args.iterations)
    payload = compare_reports(
        rust_payload,
        {
            "jar_path": str(python_report.jar_path),
            "stage_reports": [
                {
                    "name": stage_report.name,
                    "description": stage_report.description,
                    "summary": stage_report.summary,
                    "iterations": stage_report.iterations,
                    "samples_milliseconds": stage_report.samples_milliseconds,
                    "median_milliseconds": stage_report.median_milliseconds,
                    "spread_milliseconds": stage_report.spread_milliseconds,
                    "min_milliseconds": stage_report.min_milliseconds,
                    "max_milliseconds": stage_report.max_milliseconds,
                }
                for stage_report in python_report.stage_reports
            ],
        },
    )
    if args.output is not None:
        write_json(args.output, payload)
    print(json.dumps(payload, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
