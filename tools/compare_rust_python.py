from __future__ import annotations

import argparse
import json
import subprocess
from collections.abc import Sequence
from pathlib import Path
from typing import Any

from pytecode.classfile.attributes import CodeAttr
from pytecode.classfile.constant_pool import Utf8Info
from pytecode.classfile.modified_utf8 import decode_modified_utf8
from pytecode.classfile.reader import ClassReader

REPO_ROOT = Path(__file__).resolve().parent.parent
RUST_FIXTURES_DIR = REPO_ROOT / "crates" / "pytecode-engine" / "fixtures"
JAVA_FIXTURES_DIR = RUST_FIXTURES_DIR / "java"
CLASS_FIXTURES_DIR = RUST_FIXTURES_DIR / "classes"
BENCHMARK_JARS_DIR = RUST_FIXTURES_DIR / "jars"
INFRASTRUCTURE_FIXTURES = {"VerifierHarness.java"}
FIXTURE_MIN_RELEASES = {
    "StaticInterfaceMethods.java": 9,
    "StringConcat.java": 9,
    "NestAccess.java": 11,
    "SwitchExpressions.java": 14,
    "RecordClass.java": 16,
    "SealedHierarchy.java": 17,
    "PatternMatching.java": 21,
    "Java25Features.java": 25,
}
BENCHMARK_STAGES = [
    "jar-read",
    "class-parse",
    "model-lift",
    "model-lower",
    "class-write",
]


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Compare Rust CLI JSON exports against Python pytecode outputs without "
            "embedding cross-language assertions into Rust crate tests."
        )
    )
    parser.add_argument(
        "mode",
        choices=("all", "manifest", "class-summaries"),
        nargs="?",
        default="all",
        help="comparison set to run",
    )
    parser.add_argument(
        "--max-release",
        type=int,
        default=25,
        help="maximum Java fixture release to include in manifest comparison",
    )
    parser.add_argument(
        "--resource",
        action="append",
        default=[],
        help=("limit class-summary comparison to one Rust-owned Java fixture name; repeat for multiple fixtures"),
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        help="optional directory for writing exported JSON from both implementations",
    )
    return parser.parse_args(argv)


def list_java_resources(max_release: int) -> list[str]:
    resources: list[str] = []
    for path in sorted(JAVA_FIXTURES_DIR.glob("*.java")):
        if path.name in INFRASTRUCTURE_FIXTURES:
            continue
        if FIXTURE_MIN_RELEASES.get(path.name, 8) > max_release:
            continue
        resources.append(path.relative_to(JAVA_FIXTURES_DIR).as_posix())
    return resources


def list_benchmark_jars() -> list[str]:
    return sorted(path.name for path in BENCHMARK_JARS_DIR.glob("*.jar") if path.is_file())


def rust_manifest(max_release: int) -> dict[str, Any]:
    return run_rust_json(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            "pytecode-cli",
            "--",
            "compat-manifest",
            "--max-release",
            str(max_release),
        ]
    )


def python_manifest(max_release: int) -> dict[str, Any]:
    return {
        "max_release": max_release,
        "java_resources": list_java_resources(max_release),
        "benchmark_jars": list_benchmark_jars(),
        "benchmark_stages": BENCHMARK_STAGES,
    }


def selected_class_paths(resources: Sequence[str]) -> list[Path]:
    resource_names = list(resources) if resources else list_java_resources(25)
    class_paths: list[Path] = []
    for resource in resource_names:
        stem = Path(resource).stem
        resource_dir = CLASS_FIXTURES_DIR / stem
        if not resource_dir.is_dir():
            raise FileNotFoundError(f"compiled fixture directory not found: {resource_dir}")
        class_paths.extend(sorted(resource_dir.rglob("*.class")))
    return class_paths


def rust_class_summaries(resources: Sequence[str]) -> dict[str, Any]:
    summaries: dict[str, Any] = {}
    for class_path in selected_class_paths(resources):
        summaries[class_path.relative_to(CLASS_FIXTURES_DIR).as_posix()] = run_rust_json(
            [
                "cargo",
                "run",
                "-q",
                "-p",
                "pytecode-cli",
                "--",
                "class-summary",
                "--path",
                str(class_path),
            ]
        )
    return summaries


def python_class_summaries(resources: Sequence[str]) -> dict[str, Any]:
    summaries: dict[str, Any] = {}
    for class_path in selected_class_paths(resources):
        summaries[class_path.relative_to(CLASS_FIXTURES_DIR).as_posix()] = python_class_summary(class_path)
    return summaries


def python_class_summary(path: Path) -> dict[str, Any]:
    cf = ClassReader.from_file(path).class_info

    def cp_utf8(index: int) -> str:
        entry = cf.constant_pool[index]
        if entry is None or not isinstance(entry, Utf8Info):
            raise ValueError(f"constant-pool entry {index} is not Utf8")
        return decode_modified_utf8(entry.str_bytes)

    methods: list[dict[str, Any]] = []
    for method in cf.methods:
        opcodes: list[int] | None = None
        for attr in method.attributes:
            if isinstance(attr, CodeAttr):
                opcodes = [insn.type.value for insn in attr.code]
                break
        methods.append(
            {
                "name": cp_utf8(method.name_index),
                "descriptor": cp_utf8(method.descriptor_index),
                "opcodes": opcodes,
            }
        )

    return {
        "major": cf.major_version,
        "minor": cf.minor_version,
        "constant_pool_count": len(cf.constant_pool),
        "class_attr_names": [cp_utf8(attr.attribute_name_index) for attr in cf.attributes],
        "methods": methods,
    }


def run_rust_json(command: Sequence[str]) -> dict[str, Any]:
    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"Rust command failed ({' '.join(command)}):\n{completed.stderr.strip()}")
    return json.loads(completed.stdout)


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def compare_payloads(name: str, rust_payload: dict[str, Any], python_payload: dict[str, Any]) -> None:
    if rust_payload != python_payload:
        raise RuntimeError(
            f"{name} mismatch\n"
            f"Rust: {json.dumps(rust_payload, indent=2, sort_keys=True)}\n"
            f"Python: {json.dumps(python_payload, indent=2, sort_keys=True)}"
        )


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)

    if args.mode in {"all", "manifest"}:
        rust_payload = rust_manifest(args.max_release)
        python_payload = python_manifest(args.max_release)
        if args.output_dir is not None:
            write_json(args.output_dir / "manifest-rust.json", rust_payload)
            write_json(args.output_dir / "manifest-python.json", python_payload)
        compare_payloads("manifest", rust_payload, python_payload)
        print("manifest: match")

    if args.mode in {"all", "class-summaries"}:
        rust_payload = rust_class_summaries(args.resource)
        python_payload = python_class_summaries(args.resource)
        if args.output_dir is not None:
            write_json(args.output_dir / "class-summaries-rust.json", rust_payload)
            write_json(args.output_dir / "class-summaries-python.json", python_payload)
        compare_payloads("class summaries", rust_payload, python_payload)
        print("class summaries: match")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
