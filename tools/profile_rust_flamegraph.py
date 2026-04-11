"""Generate Rust flamegraphs for individual pipeline stages.

Wraps ``cargo flamegraph`` to produce per-stage SVG flamegraphs by running a
small Rust binary that exercises only the requested stage.  Requires the
``flamegraph`` cargo sub-command (``cargo install flamegraph``).

On Windows this relies on ``dtrace`` or ETW; on Linux it uses ``perf``.

Usage:
    uv run python tools/profile_rust_flamegraph.py
    uv run python tools/profile_rust_flamegraph.py --stages class-parse model-lift
    uv run python tools/profile_rust_flamegraph.py --iterations 20 --output-dir output/flamegraphs
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from collections.abc import Sequence
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_JAR = REPO_ROOT / "crates" / "pytecode-engine" / "fixtures" / "jars" / "byte-buddy-1.17.5.jar"
STAGE_ORDER = ("jar-read", "class-parse", "model-lift", "model-lower", "class-write")


def check_cargo_flamegraph() -> bool:
    """Return True if ``cargo flamegraph`` is available."""
    result = subprocess.run(
        ["cargo", "flamegraph", "--help"],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode == 0


def build_profiling_binary(jar_path: Path, stage: str, iterations: int) -> str:
    """Return the Rust source for a minimal binary that runs one stage."""
    return f'''\
//! Auto-generated stage profiler. Do not edit.
use pytecode_archive::JarFile;
use pytecode_engine::{{parse_class, write_class}};
use pytecode_engine::model::ClassModel;
use pytecode_engine::raw::ClassFile;
use std::path::Path;

fn main() {{
    let jar_path = Path::new(r"{jar_path}");
    let iterations = {iterations}_usize;

    match "{stage}" {{
        "jar-read" => {{
            for _ in 0..iterations {{
                let jar = JarFile::open(jar_path).unwrap();
                let _ = jar.parse_classes();
            }}
        }}
        "class-parse" => {{
            let jar = JarFile::open(jar_path).unwrap();
            let (classes, _) = jar.parse_classes();
            for _ in 0..iterations {{
                for (_, stub) in &classes {{
                    let _ = parse_class(&stub.bytes).unwrap();
                }}
            }}
        }}
        "model-lift" => {{
            let jar = JarFile::open(jar_path).unwrap();
            let (classes, _) = jar.parse_classes();
            let classfiles: Vec<ClassFile> = classes
                .iter()
                .map(|(_, stub)| parse_class(&stub.bytes).unwrap())
                .collect();
            for _ in 0..iterations {{
                for cf in &classfiles {{
                    let _ = ClassModel::from_classfile(cf).unwrap();
                }}
            }}
        }}
        "model-lower" => {{
            let jar = JarFile::open(jar_path).unwrap();
            let (classes, _) = jar.parse_classes();
            let classfiles: Vec<ClassFile> = classes
                .iter()
                .map(|(_, stub)| parse_class(&stub.bytes).unwrap())
                .collect();
            let models: Vec<ClassModel> = classfiles
                .iter()
                .map(|cf| ClassModel::from_classfile(cf).unwrap())
                .collect();
            for _ in 0..iterations {{
                for model in &models {{
                    let _ = model.to_classfile().unwrap();
                }}
            }}
        }}
        "class-write" => {{
            let jar = JarFile::open(jar_path).unwrap();
            let (classes, _) = jar.parse_classes();
            let classfiles: Vec<ClassFile> = classes
                .iter()
                .map(|(_, stub)| parse_class(&stub.bytes).unwrap())
                .collect();
            let models: Vec<ClassModel> = classfiles
                .iter()
                .map(|cf| ClassModel::from_classfile(cf).unwrap())
                .collect();
            let lowered: Vec<ClassFile> = models
                .iter()
                .map(|m| m.to_classfile().unwrap())
                .collect();
            for _ in 0..iterations {{
                for cf in &lowered {{
                    let _ = write_class(cf).unwrap();
                }}
            }}
        }}
        other => panic!("unknown stage: {{other}}"),
    }}
}}
'''


def run_flamegraph_for_stage(
    jar_path: Path,
    stage: str,
    iterations: int,
    output_dir: Path,
) -> Path | None:
    """Generate a flamegraph SVG for one stage.  Returns the SVG path or None on failure."""
    # Write the profiling binary source
    profiler_dir = REPO_ROOT / "target" / "flamegraph-profiler"
    src_dir = profiler_dir / "src"
    src_dir.mkdir(parents=True, exist_ok=True)

    cargo_toml = profiler_dir / "Cargo.toml"
    cargo_toml.write_text(
        f"""\
[package]
name = "flamegraph-profiler"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
pytecode-engine = {{ path = "{(REPO_ROOT / "crates" / "pytecode-engine").as_posix()}" }}
pytecode-archive = {{ path = "{(REPO_ROOT / "crates" / "pytecode-archive").as_posix()}" }}
""",
        encoding="utf-8",
    )

    main_rs = src_dir / "main.rs"
    main_rs.write_text(
        build_profiling_binary(jar_path, stage, iterations),
        encoding="utf-8",
    )

    # Run cargo flamegraph
    svg_path = output_dir / f"{stage}.svg"
    output_dir.mkdir(parents=True, exist_ok=True)

    command = [
        "cargo",
        "flamegraph",
        "--manifest-path",
        str(cargo_toml),
        "--root",
        "-o",
        str(svg_path),
        "--release",
    ]

    print(f"  Generating flamegraph for {stage}...")
    completed = subprocess.run(
        command,
        cwd=profiler_dir,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        print(f"    FAILED: {completed.stderr.strip()[:200]}", file=sys.stderr)
        return None

    print(f"    -> {svg_path}")
    return svg_path


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="Generate Rust flamegraphs for individual pipeline stages.",
    )
    parser.add_argument(
        "--jar",
        type=Path,
        default=DEFAULT_JAR,
        help=f"JAR file to profile (default: {DEFAULT_JAR.name}).",
    )
    parser.add_argument(
        "--stages",
        nargs="+",
        choices=STAGE_ORDER,
        default=list(STAGE_ORDER),
        help="Stages to profile (default: all).",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=5,
        help="Iterations per stage (more = smoother flamegraph, default: 5).",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=REPO_ROOT / "output" / "flamegraphs",
        help="Directory for SVG output.",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    """Generate flamegraphs for the requested stages."""
    args = parse_args(argv)
    jar_path = args.jar.expanduser().resolve()

    if not jar_path.exists():
        print(f"Error: JAR not found: {jar_path}", file=sys.stderr)
        return 1

    if not check_cargo_flamegraph():
        print("Error: cargo-flamegraph not installed.")
        print("Install with: cargo install flamegraph")
        print()
        print("Platform requirements:")
        print("  Linux:   install 'perf' (linux-tools-generic or perf)")
        print("  macOS:   dtrace (built in)")
        print("  Windows: install DTrace or use WSL")
        return 1

    print(f"JAR: {jar_path.name}")
    print(f"Stages: {', '.join(args.stages)}")
    print(f"Iterations: {args.iterations}")
    print(f"Output: {args.output_dir}")
    print()

    results: list[tuple[str, Path | None]] = []
    for stage in args.stages:
        svg = run_flamegraph_for_stage(jar_path, stage, args.iterations, args.output_dir)
        results.append((stage, svg))

    print()
    print("Results:")
    for stage, svg in results:
        status = str(svg) if svg else "FAILED"
        print(f"  {stage}: {status}")

    # Clean up generated profiler crate
    profiler_dir = REPO_ROOT / "target" / "flamegraph-profiler"
    if profiler_dir.exists():
        shutil.rmtree(profiler_dir, ignore_errors=True)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
