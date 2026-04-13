"""Unified Rust vs Python-Rust benchmark comparison with per-stage profiling.

Runs both native Rust (via bench-smoke CLI) and Python-via-Rust benchmarks for
all five pipeline stages, then prints a side-by-side comparison table with
overhead ratios. Optionally rebuilds the Python extension in a chosen profile
before benchmarking so the Python-via-Rust path is compared against the same
optimization level as the native Rust CLI. Optionally generates cProfile
``.prof`` files for the Python side and/or a JSON report for CI tracking.

Usage:
    uv run python tools/bench_full_comparison.py
    uv run python tools/bench_full_comparison.py --iterations 10
    uv run python tools/bench_full_comparison.py --extension-build installed
    uv run python tools/bench_full_comparison.py --jar path/to/custom.jar --profile
    uv run python tools/bench_full_comparison.py --output report.json --profile --profile-dir output/profiles
"""

from __future__ import annotations

import argparse
import cProfile
import json
import subprocess
import sys
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

DEFAULT_JAR = REPO_ROOT / "crates" / "pytecode-engine" / "fixtures" / "jars" / "byte-buddy-1.17.5.jar"

STAGE_ORDER = ("jar-read", "class-parse", "model-lift", "model-lower", "class-write")


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class StageResult:
    """Timing result for one stage from one implementation."""

    name: str
    median_ms: float
    spread_ms: float
    min_ms: float
    max_ms: float
    samples: list[float]
    units: int = 0
    bytes_total: int = 0


@dataclass(frozen=True)
class ComparisonRow:
    """One row of the comparison table."""

    stage: str
    rust: StageResult
    python: StageResult
    overhead: float | None  # python_median / rust_median


@dataclass(frozen=True)
class ComparisonReport:
    """Full comparison report."""

    jar: str
    iterations: int
    python_extension_build: str
    rows: list[ComparisonRow]
    rust_total_ms: float
    python_total_ms: float
    overall_overhead: float | None


# ---------------------------------------------------------------------------
# Rust benchmarks (via pytecode-cli bench-smoke)
# ---------------------------------------------------------------------------


def run_rust_benchmarks(jar_path: Path, iterations: int) -> list[StageResult]:
    """Run the native Rust CLI benchmark and return per-stage results."""
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
        raise RuntimeError(f"Rust benchmark failed:\n{completed.stderr.strip()}")

    payload = json.loads(completed.stdout)
    results: list[StageResult] = []
    for stage in payload["stage_reports"]:
        results.append(
            StageResult(
                name=stage["stage"],
                median_ms=stage["median_milliseconds"],
                spread_ms=stage["spread_milliseconds"],
                min_ms=stage["min_milliseconds"],
                max_ms=stage["max_milliseconds"],
                samples=[float(s) for s in stage["samples_milliseconds"]],
                units=stage.get("units", 0),
                bytes_total=stage.get("bytes", 0),
            )
        )
    return results


def ensure_python_extension(build_profile: str) -> None:
    """Build and install the Python extension in the requested profile."""
    if build_profile == "installed":
        return

    command = [
        "uv",
        "run",
        "maturin",
        "develop",
        "-m",
        str(REPO_ROOT / "crates" / "pytecode-python" / "Cargo.toml"),
    ]
    if build_profile == "release":
        command.insert(4, "--release")

    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"Python extension rebuild failed with profile {build_profile!r}")


# ---------------------------------------------------------------------------
# Python-via-Rust benchmarks
# ---------------------------------------------------------------------------


@dataclass
class _PythonInputs:
    """Lazy cache of shared inputs for isolated Python-via-Rust stage benchmarks."""

    jar_path: Path
    _archive: Any = field(default=None, repr=False)
    _classified: Any = field(default=None, repr=False)
    _parsed: Any = field(default=None, repr=False)
    _models: Any = field(default=None, repr=False)

    def archive(self) -> Any:
        from pytecode import JarFile

        if self._archive is None:
            self._archive = JarFile(self.jar_path)
        return self._archive

    def classified(self) -> tuple[list[Any], list[Any]]:
        if self._classified is None:
            jar = self.archive()
            import os

            class_entries: list[Any] = []
            other_entries: list[Any] = []
            for jar_info in jar.files.values():
                fn = jar_info.filename
                if not fn.endswith(os.sep) and fn.endswith(".class"):
                    class_entries.append(jar_info)
                else:
                    other_entries.append(jar_info)
            self._classified = (class_entries, other_entries)
        return self._classified

    def parsed(self) -> list[tuple[Any, Any]]:
        from pytecode import ClassReader

        if self._parsed is None:
            class_entries, _ = self.classified()
            self._parsed = [(ji, ClassReader.from_bytes(ji.bytes)) for ji in class_entries]
        return self._parsed

    def models(self) -> list[tuple[Any, Any]]:
        from pytecode import ClassModel

        if self._models is None:
            parsed = self.parsed()
            self._models = [(ji, ClassModel.from_bytes(ji.bytes)) for ji, _r in parsed]
        return self._models


@dataclass(frozen=True)
class _PreparedStage:
    name: str
    workload: Callable[[], str]


def _prepare_jar_read(inputs: _PythonInputs) -> _PreparedStage:
    import os

    from pytecode import JarFile

    def workload() -> str:
        jar = JarFile(inputs.jar_path)
        cls = [j for j in jar.files.values() if not j.filename.endswith(os.sep) and j.filename.endswith(".class")]
        return f"entries={len(jar.files)} classes={len(cls)}"

    return _PreparedStage(name="jar-read", workload=workload)


def _prepare_class_parse(inputs: _PythonInputs) -> _PreparedStage:
    from pytecode import ClassReader

    class_entries, _ = inputs.classified()

    def workload() -> str:
        parsed = [ClassReader.from_bytes(ji.bytes) for ji in class_entries]
        return f"parsed={len(parsed)}"

    return _PreparedStage(name="class-parse", workload=workload)


def _prepare_model_lift(inputs: _PythonInputs) -> _PreparedStage:
    from pytecode import ClassModel

    parsed = inputs.parsed()

    def workload() -> str:
        models = [ClassModel.from_bytes(ji.bytes) for ji, _r in parsed]
        return f"models={len(models)}"

    return _PreparedStage(name="model-lift", workload=workload)


def _prepare_model_lower(inputs: _PythonInputs) -> _PreparedStage:
    models = inputs.models()
    model_values = [model for _ji, model in models]

    def workload() -> str:
        lowered = [model.to_classfile() for model in model_values]
        return f"lowered={len(lowered)}"

    return _PreparedStage(name="model-lower", workload=workload)


def _prepare_class_write(inputs: _PythonInputs) -> _PreparedStage:
    from pytecode import ClassWriter

    parsed = inputs.parsed()

    def workload() -> str:
        written = [bytes(ClassWriter.write(r.class_info)) for _ji, r in parsed]
        return f"written={len(written)}"

    return _PreparedStage(name="class-write", workload=workload)


_PYTHON_STAGE_BUILDERS: dict[str, Callable[[_PythonInputs], _PreparedStage]] = {
    "jar-read": _prepare_jar_read,
    "class-parse": _prepare_class_parse,
    "model-lift": _prepare_model_lift,
    "model-lower": _prepare_model_lower,
    "class-write": _prepare_class_write,
}


def _summarize(samples: list[float]) -> tuple[float, float, float, float]:
    """Return (median, spread, min, max) in ms."""
    ordered = sorted(samples)
    mn, mx = ordered[0], ordered[-1]
    mid = len(ordered) // 2
    median = ordered[mid] if len(ordered) % 2 == 1 else (ordered[mid - 1] + ordered[mid]) / 2
    return median, mx - mn, mn, mx


def run_python_benchmarks(
    jar_path: Path,
    iterations: int,
    *,
    profile: bool = False,
    profile_dir: Path | None = None,
) -> list[StageResult]:
    """Run Python-via-Rust benchmarks for all stages, optionally generating cProfile output."""
    inputs = _PythonInputs(jar_path)
    results: list[StageResult] = []

    for stage_name in STAGE_ORDER:
        builder = _PYTHON_STAGE_BUILDERS[stage_name]
        prepared = builder(inputs)
        samples_ms: list[float] = []

        profiler = cProfile.Profile() if profile else None

        for _ in range(iterations):
            if profiler is not None:
                profiler.enable()
            t0 = time.perf_counter()
            prepared.workload()
            elapsed_ms = (time.perf_counter() - t0) * 1000
            if profiler is not None:
                profiler.disable()
            samples_ms.append(elapsed_ms)

        if profiler is not None and profile_dir is not None:
            profile_dir.mkdir(parents=True, exist_ok=True)
            profiler.dump_stats(str(profile_dir / f"{stage_name}.prof"))

        median, spread, mn, mx = _summarize(samples_ms)
        results.append(
            StageResult(
                name=stage_name,
                median_ms=median,
                spread_ms=spread,
                min_ms=mn,
                max_ms=mx,
                samples=samples_ms,
            )
        )

    return results


# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------


def build_comparison(
    jar_path: Path,
    iterations: int,
    python_extension_build: str,
    rust_results: list[StageResult],
    python_results: list[StageResult],
) -> ComparisonReport:
    """Build a ComparisonReport from Rust and Python results."""
    rust_by_name = {r.name: r for r in rust_results}
    python_by_name = {r.name: r for r in python_results}

    rows: list[ComparisonRow] = []
    for stage_name in STAGE_ORDER:
        rust = rust_by_name[stage_name]
        python = python_by_name[stage_name]
        overhead = round(python.median_ms / rust.median_ms, 2) if rust.median_ms > 0 else None
        rows.append(ComparisonRow(stage=stage_name, rust=rust, python=python, overhead=overhead))

    rust_total = sum(r.rust.median_ms for r in rows)
    python_total = sum(r.python.median_ms for r in rows)
    overall = round(python_total / rust_total, 2) if rust_total > 0 else None

    return ComparisonReport(
        jar=str(jar_path),
        iterations=iterations,
        python_extension_build=python_extension_build,
        rows=rows,
        rust_total_ms=rust_total,
        python_total_ms=python_total,
        overall_overhead=overall,
    )


# ---------------------------------------------------------------------------
# Output formatting
# ---------------------------------------------------------------------------


def _fmt_ms(val: float) -> str:
    if val >= 1000:
        return f"{val / 1000:.2f}s"
    if val >= 10:
        return f"{val:.0f}ms"
    if val >= 1:
        return f"{val:.1f}ms"
    return f"{val:.2f}ms"


def print_comparison_table(report: ComparisonReport) -> None:
    """Print a formatted comparison table to stdout."""
    jar_name = Path(report.jar).name
    print(f"\n{'=' * 78}")
    print("  Rust vs Python-Rust Benchmark Comparison")
    print(f"  JAR: {jar_name}  |  Iterations: {report.iterations}")
    print(f"  Python extension build: {report.python_extension_build}")
    print(f"{'=' * 78}")
    print()

    # Header
    header = f"  {'Stage':<15} {'Rust':>10} {'Python':>10} {'Overhead':>10} {'Status':>8}"
    print(header)
    print(f"  {'-' * 15} {'-' * 10} {'-' * 10} {'-' * 10} {'-' * 8}")

    for row in report.rows:
        rust_str = _fmt_ms(row.rust.median_ms)
        python_str = _fmt_ms(row.python.median_ms)
        if row.overhead is not None:
            overhead_str = f"{row.overhead:.2f}x"
            status = "OK" if row.overhead < 2.0 else "SLOW" if row.overhead < 5.0 else "!!"
        else:
            overhead_str = "N/A"
            status = "?"
        print(f"  {row.stage:<15} {rust_str:>10} {python_str:>10} {overhead_str:>10} {status:>8}")

    print(f"  {'-' * 15} {'-' * 10} {'-' * 10} {'-' * 10} {'-' * 8}")
    print(
        f"  {'TOTAL':<15} "
        f"{_fmt_ms(report.rust_total_ms):>10} "
        f"{_fmt_ms(report.python_total_ms):>10} "
        f"{f'{report.overall_overhead:.2f}x' if report.overall_overhead else 'N/A':>10}"
    )
    print()

    # Per-stage sample details
    print(f"  {'Stage':<15} {'Rust samples (ms)':>40} {'Python samples (ms)':>40}")
    print(f"  {'-' * 15} {'-' * 40} {'-' * 40}")
    for row in report.rows:
        rust_samples = ", ".join(f"{s:.0f}" for s in row.rust.samples[:8])
        python_samples = ", ".join(f"{s:.0f}" for s in row.python.samples[:8])
        print(f"  {row.stage:<15} {rust_samples:>40} {python_samples:>40}")
    print()


def report_to_json(report: ComparisonReport) -> dict[str, Any]:
    """Convert a ComparisonReport to a JSON-serializable dict."""
    return {
        "jar": report.jar,
        "iterations": report.iterations,
        "python_extension_build": report.python_extension_build,
        "rust_total_ms": report.rust_total_ms,
        "python_total_ms": report.python_total_ms,
        "overall_overhead": report.overall_overhead,
        "stages": [
            {
                "stage": row.stage,
                "rust_median_ms": row.rust.median_ms,
                "rust_spread_ms": row.rust.spread_ms,
                "rust_min_ms": row.rust.min_ms,
                "rust_max_ms": row.rust.max_ms,
                "rust_samples_ms": row.rust.samples,
                "python_median_ms": row.python.median_ms,
                "python_spread_ms": row.python.spread_ms,
                "python_min_ms": row.python.min_ms,
                "python_max_ms": row.python.max_ms,
                "python_samples_ms": row.python.samples,
                "overhead": row.overhead,
            }
            for row in report.rows
        ],
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="Run Rust vs Python-Rust benchmarks for all pipeline stages.",
    )
    parser.add_argument(
        "--jar",
        type=Path,
        default=DEFAULT_JAR,
        help=f"JAR file to benchmark (default: {DEFAULT_JAR.name}).",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=5,
        help="Number of timing iterations per stage (default: 5).",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Write JSON comparison report to this path.",
    )
    parser.add_argument(
        "--profile",
        action="store_true",
        help="Enable cProfile for Python stages; writes .prof files.",
    )
    parser.add_argument(
        "--profile-dir",
        type=Path,
        default=None,
        help="Directory for .prof files (default: output/profiles/<jar-stem>).",
    )
    parser.add_argument(
        "--python-only",
        action="store_true",
        help="Skip Rust benchmarks; only run and report Python-via-Rust timings.",
    )
    parser.add_argument(
        "--extension-build",
        choices=("release", "dev", "installed"),
        default="release",
        help=(
            "Python extension build to benchmark: rebuild with the chosen maturin profile "
            "or use the already-installed extension (default: release)."
        ),
    )
    parser.add_argument(
        "--rust-only",
        action="store_true",
        help="Skip Python benchmarks; only run and report native Rust timings.",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    """Run the full comparison harness."""
    args = parse_args(argv)
    jar_path = args.jar.expanduser().resolve()

    if not jar_path.exists():
        print(f"Error: JAR not found: {jar_path}", file=sys.stderr)
        return 1

    profile_dir = args.profile_dir
    if args.profile and profile_dir is None:
        profile_dir = REPO_ROOT / "output" / "profiles" / jar_path.stem

    # Run benchmarks
    rust_results: list[StageResult] | None = None
    python_results: list[StageResult] | None = None

    if not args.python_only:
        print(f"Running Rust benchmarks ({args.iterations} iterations)...")
        rust_results = run_rust_benchmarks(jar_path, args.iterations)
        if args.rust_only:
            print(f"\n  {'Stage':<15} {'Median':>10} {'Spread':>10}")
            print(f"  {'-' * 15} {'-' * 10} {'-' * 10}")
            for r in rust_results:
                print(f"  {r.name:<15} {_fmt_ms(r.median_ms):>10} {_fmt_ms(r.spread_ms):>10}")
            total = sum(r.median_ms for r in rust_results)
            print(f"  {'-' * 15} {'-' * 10}")
            print(f"  {'TOTAL':<15} {_fmt_ms(total):>10}")
            return 0

    if not args.rust_only:
        print(f"Preparing Python extension ({args.extension_build})...")
        ensure_python_extension(args.extension_build)
        print(f"Running Python-via-Rust benchmarks ({args.iterations} iterations)...")
        python_results = run_python_benchmarks(
            jar_path,
            args.iterations,
            profile=args.profile,
            profile_dir=profile_dir,
        )
        if args.python_only:
            print(f"\n  {'Stage':<15} {'Median':>10} {'Spread':>10}")
            print(f"  {'-' * 15} {'-' * 10} {'-' * 10}")
            for r in python_results:
                print(f"  {r.name:<15} {_fmt_ms(r.median_ms):>10} {_fmt_ms(r.spread_ms):>10}")
            total = sum(r.median_ms for r in python_results)
            print(f"  {'-' * 15} {'-' * 10}")
            print(f"  {'TOTAL':<15} {_fmt_ms(total):>10}")
            if args.profile and profile_dir:
                print(f"\n  cProfile output: {profile_dir}")
            return 0

    assert rust_results is not None and python_results is not None
    report = build_comparison(
        jar_path,
        args.iterations,
        args.extension_build,
        rust_results,
        python_results,
    )
    print_comparison_table(report)

    if args.profile and profile_dir:
        print(f"  cProfile output: {profile_dir}")
        print(f"  Inspect with: python -m pstats {profile_dir / '<stage>.prof'}")
        print()

    if args.output is not None:
        payload = report_to_json(report)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        print(f"  JSON report: {args.output}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
