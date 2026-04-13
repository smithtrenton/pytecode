from __future__ import annotations

import argparse
import json
import sys
import time
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))


@dataclass(frozen=True)
class StageBenchmarkReport:
    """Elapsed-time summary for one isolated benchmark stage."""

    name: str
    description: str
    summary: str
    iterations: int
    samples_milliseconds: list[int]
    median_milliseconds: int
    spread_milliseconds: int
    min_milliseconds: int
    max_milliseconds: int


@dataclass(frozen=True)
class JarBenchmarkReport:
    """Benchmark report for one JAR path."""

    jar_path: Path
    stage_reports: list[StageBenchmarkReport]


@dataclass(frozen=True)
class CorpusBenchmarkReport:
    """Benchmark report for one or more JAR paths."""

    iterations: int
    jars: list[JarBenchmarkReport]


def summarize_samples(samples: Sequence[int]) -> tuple[int, int, int, int]:
    """Return median, spread, min, and max milliseconds for *samples*."""

    if not samples:
        raise ValueError("benchmark samples must not be empty")
    ordered = sorted(samples)
    min_milliseconds = ordered[0]
    max_milliseconds = ordered[-1]
    midpoint = len(ordered) // 2
    if len(ordered) % 2 == 1:
        median_milliseconds = ordered[midpoint]
    else:
        median_milliseconds = (ordered[midpoint - 1] + ordered[midpoint]) // 2
    spread_milliseconds = max_milliseconds - min_milliseconds
    return median_milliseconds, spread_milliseconds, min_milliseconds, max_milliseconds


def benchmark_stage(jar_path: Path, stage_name: str, iterations: int) -> StageBenchmarkReport:
    """Run one isolated stage repeatedly and return median+spread timing."""

    from tools.profile_jar_pipeline import STAGE_BUILDERS, ProfileInputs

    prepared_stage = STAGE_BUILDERS[stage_name](ProfileInputs(jar_path))
    samples_milliseconds: list[int] = []
    summary = ""
    for _ in range(iterations):
        started = time.perf_counter()
        summary = prepared_stage.workload()
        elapsed_milliseconds = int(round((time.perf_counter() - started) * 1000))
        samples_milliseconds.append(elapsed_milliseconds)

    median_milliseconds, spread_milliseconds, min_milliseconds, max_milliseconds = summarize_samples(
        samples_milliseconds
    )
    return StageBenchmarkReport(
        name=prepared_stage.name,
        description=prepared_stage.description,
        summary=summary,
        iterations=iterations,
        samples_milliseconds=samples_milliseconds,
        median_milliseconds=median_milliseconds,
        spread_milliseconds=spread_milliseconds,
        min_milliseconds=min_milliseconds,
        max_milliseconds=max_milliseconds,
    )


def benchmark_jar(jar_path: Path, stage_names: Sequence[str], iterations: int) -> JarBenchmarkReport:
    """Benchmark the requested isolated stages for one JAR."""

    return JarBenchmarkReport(
        jar_path=jar_path,
        stage_reports=[benchmark_stage(jar_path, stage_name, iterations) for stage_name in stage_names],
    )


def corpus_report_to_json(report: CorpusBenchmarkReport) -> dict[str, Any]:
    """Convert a corpus benchmark report into JSON-ready data."""

    return {
        "iterations": report.iterations,
        "jars": [
            {
                "jar_path": str(jar_report.jar_path),
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
                    for stage_report in jar_report.stage_reports
                ],
            }
            for jar_report in report.jars
        ],
    }


def write_json(path: Path, payload: dict[str, Any]) -> None:
    """Write JSON output to *path* with a stable formatting style."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments for isolated Python stage benchmarks."""

    from tools.profile_jar_pipeline import STAGE_BUILDERS, positive_int

    parser = argparse.ArgumentParser(
        description="Benchmark one jar or a directory of jars with isolated pytecode stages.",
    )
    parser.add_argument(
        "inputs",
        nargs="+",
        type=Path,
        help="A .jar file, multiple .jar files, or directories containing .jar files.",
    )
    parser.add_argument(
        "--recursive",
        action="store_true",
        help="Search provided directories recursively for .jar files.",
    )
    parser.add_argument(
        "--stages",
        nargs="+",
        choices=tuple(STAGE_BUILDERS),
        default=None,
        help=(
            "Stages to benchmark. Defaults to all stages for a single jar, or "
            "model-lift/model-lower for multiple jars or directories."
        ),
    )
    parser.add_argument(
        "--iterations",
        type=positive_int,
        default=5,
        help="Number of timing samples to collect for each requested stage.",
    )
    parser.add_argument(
        "--summary-json",
        type=Path,
        default=None,
        help="Optional JSON file to write with the benchmark report payload.",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    """Run isolated Python benchmarks and emit a JSON report."""

    from tools.profile_jar_pipeline import default_stage_names, expand_jar_inputs

    args = parse_args(argv)
    try:
        jar_paths = expand_jar_inputs(args.inputs, recursive=args.recursive)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc
    stage_names = default_stage_names(args.inputs, args.stages)
    report = CorpusBenchmarkReport(
        iterations=args.iterations,
        jars=[benchmark_jar(jar_path, stage_names, args.iterations) for jar_path in jar_paths],
    )
    payload = corpus_report_to_json(report)
    if args.summary_json is not None:
        write_json(args.summary_json, payload)
    print(json.dumps(payload, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
