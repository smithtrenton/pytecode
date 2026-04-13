from __future__ import annotations

import argparse
import json
import unicodedata
from collections.abc import Iterable, Sequence
from pathlib import Path

from pytecode import JarFile
from pytecode.archive import FrameComputationMode
from pytecode.model import BranchInsn, ClassModel, Label, LdcInsn, LookupSwitchInsn, RawInsn, TableSwitchInsn
from pytecode.transforms import (
    CodeTransform,
    InsnMatcher,
    MethodMatcher,
    Pipeline,
    PipelineBuilder,
    class_named,
)


def analyze_jar(jar_path: str | Path) -> dict[str, object]:
    jar = JarFile(jar_path)
    class_metrics: list[dict[str, object]] = []

    for jar_info in jar.files.values():
        if not jar_info.filename.endswith(".class"):
            continue
        model = ClassModel.from_bytes(jar_info.bytes)
        class_metrics.append(_class_metrics(model, len(jar_info.bytes)))

    package_counts: dict[str, list[int]] = {}
    suspicious_classes: list[str] = []
    rl_classes: list[str] = []
    for metrics in class_metrics:
        package = str(metrics["package"])
        counts = package_counts.setdefault(package, [0, 0])
        counts[0] += 1
        if bool(metrics["suspicious_name"]):
            counts[1] += 1
            suspicious_classes.append(str(metrics["class_name"]))
        if bool(metrics["rl_name"]):
            rl_classes.append(str(metrics["class_name"]))

    top_packages = [
        {
            "package": package,
            "class_count": class_count,
            "suspicious_class_count": suspicious_count,
        }
        for package, (class_count, suspicious_count) in package_counts.items()
    ]
    top_packages.sort(
        key=lambda item: (
            -int(item["class_count"]),
            -int(item["suspicious_class_count"]),
            str(item["package"]),
        )
    )

    hotspot_classes = [
        {
            "class_name": str(metrics["class_name"]),
            "byte_len": int(metrics["byte_len"]),
            "method_count": int(metrics["method_count"]),
            "field_count": int(metrics["field_count"]),
            "readable_string_count": len(metrics["readable_strings"]),
            "branch_count": int(metrics["branch_count"]),
            "nop_count": int(metrics["nop_count"]),
        }
        for metrics in class_metrics
    ]
    hotspot_classes.sort(
        key=lambda item: (
            -int(item["method_count"]),
            -int(item["field_count"]),
            -int(item["byte_len"]),
            str(item["class_name"]),
        )
    )

    string_hint_classes = [
        {
            "class_name": str(metrics["class_name"]),
            "string_count": len(metrics["readable_strings"]),
            "samples": list(metrics["readable_strings"][:3]),
        }
        for metrics in class_metrics
        if metrics["readable_strings"]
    ]
    string_hint_classes.sort(key=lambda item: (-int(item["string_count"]), str(item["class_name"])))

    return {
        "input_jar": str(Path(jar.filename)),
        "class_entries": len(class_metrics),
        "resource_entries": sum(1 for jar_info in jar.files.values() if not jar_info.filename.endswith(".class")),
        "suspicious_class_count": len(suspicious_classes),
        "rl_class_count": len(rl_classes),
        "top_packages": top_packages[:10],
        "sample_suspicious_classes": suspicious_classes[:50],
        "sample_rl_classes": rl_classes[:25],
        "compiler_control_excludes": _compiler_control_excludes(jar),
        "hotspot_classes": hotspot_classes[:10],
        "string_hint_classes": string_hint_classes[:10],
    }


def rewrite_jar(jar_path: str | Path, output_path: str | Path) -> dict[str, object]:
    jar = JarFile(jar_path)
    class_entries = sum(1 for jar_info in jar.files.values() if jar_info.filename.endswith(".class"))
    resource_entries = sum(1 for jar_info in jar.files.values() if not jar_info.filename.endswith(".class"))
    stats = {
        "classes_changed": 0,
        "nops_removed": 0,
        "noop_gotos_removed": 0,
        "goto_redirects": 0,
    }

    for jar_info in list(jar.files.values()):
        if not jar_info.filename.endswith(".class"):
            continue
        model = ClassModel.from_bytes(jar_info.bytes)
        pipeline, class_stats = _build_class_rewrite_pipeline(model)
        if pipeline is None:
            continue
        pipeline.apply(model)
        updated_bytes = model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
        if updated_bytes == jar_info.bytes:
            continue
        jar.add_file(jar_info.filename, updated_bytes, zipinfo=jar_info.zipinfo)
        stats["classes_changed"] += 1
        stats["nops_removed"] += class_stats["nops_removed"]
        stats["noop_gotos_removed"] += class_stats["noop_gotos_removed"]
        stats["goto_redirects"] += class_stats["goto_redirects"]

    jar.rewrite(output_path)
    return {
        "input_jar": str(Path(jar_path)),
        "output_jar": str(Path(output_path)),
        "class_entries": class_entries,
        "resource_entries": resource_entries,
        **stats,
    }


def _build_class_rewrite_pipeline(model: ClassModel) -> tuple[Pipeline | None, dict[str, int]]:
    builder = PipelineBuilder()
    stats = {
        "nops_removed": 0,
        "noop_gotos_removed": 0,
        "goto_redirects": 0,
    }
    planned = False

    for method in model.methods:
        code = method.code
        if code is None:
            continue
        instructions = code.instructions.to_list()
        if _has_control_flow_items(instructions):
            continue

        nop_count = sum(1 for item in instructions if _is_nop(item))
        if nop_count == 0:
            continue

        builder.on_code(
            MethodMatcher.named(method.name) & MethodMatcher.descriptor(method.descriptor),
            CodeTransform.remove_insn(_nop_matcher()),
            owner_matcher=class_named(model.name),
        )
        planned = True
        stats["nops_removed"] += nop_count

    return (builder.build() if planned else None, stats)


def _class_metrics(model: ClassModel, byte_len: int) -> dict[str, object]:
    readable_strings: list[str] = []
    seen_strings: set[str] = set()
    branch_count = 0
    nop_count = 0

    for method in model.methods:
        code = method.code
        if code is None:
            continue
        instructions = code.instructions.to_list()
        branch_count += sum(1 for item in instructions if isinstance(item, BranchInsn))
        nop_count += sum(1 for item in instructions if _is_nop(item))

        for value in _iter_readable_strings(instructions):
            if value in seen_strings:
                continue
            seen_strings.add(value)
            readable_strings.append(value)

    package, _, simple_name = model.name.rpartition("/")
    return {
        "class_name": model.name,
        "package": package or "<root>",
        "byte_len": byte_len,
        "method_count": len(model.methods),
        "field_count": len(model.fields),
        "readable_strings": readable_strings,
        "branch_count": branch_count,
        "nop_count": nop_count,
        "suspicious_name": _is_suspicious_simple_name(simple_name),
        "rl_name": _is_rl_simple_name(simple_name),
    }


def _iter_readable_strings(instructions: Iterable[object]) -> Iterable[str]:
    for item in instructions:
        if not isinstance(item, LdcInsn) or item.value_type != "string":
            continue
        value = item.value
        if isinstance(value, str) and _is_readable_string(value):
            yield value


def _has_control_flow_items(instructions: Sequence[object]) -> bool:
    return any(isinstance(item, (Label, BranchInsn, LookupSwitchInsn, TableSwitchInsn)) for item in instructions)


def _is_nop(item: object) -> bool:
    return isinstance(item, RawInsn) and item.opcode == 0


def _nop_matcher():
    return InsnMatcher.opcode(0)


def _compiler_control_excludes(jar: JarFile) -> list[str]:
    entry = jar.files.get("compilercontrol.json")
    if entry is None:
        return []
    parsed = json.loads(entry.bytes.decode("utf-8"))
    matches: list[str] = []
    for item in parsed:
        c2 = item.get("c2") or {}
        if c2.get("exclude"):
            matches.extend(str(match) for match in item.get("match", []))
    return matches


def _is_suspicious_simple_name(name: str) -> bool:
    return 1 <= len(name) <= 2 and name.isascii() and name.islower() and name.isalpha()


def _is_rl_simple_name(name: str) -> bool:
    return name.startswith("rl") and name[2:].isdigit()


def _is_readable_string(value: str) -> bool:
    return (
        len(value) >= 4
        and sum(character.isalpha() for character in value) >= 3
        and all(unicodedata.category(character)[0] != "C" for character in value)
    )


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Analyze and rewrite obfuscated jars with pytecode's Python API.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    analyze_parser = subparsers.add_parser(
        "analyze",
        help="Report suspicious naming, compiler-control hints, hotspots, and string anchors.",
    )
    analyze_parser.add_argument("--jar", type=Path, required=True, help="Input JAR to inspect.")

    rewrite_parser = subparsers.add_parser(
        "rewrite",
        help="Apply safe bytecode cleanup passes and write a rewritten JAR.",
    )
    rewrite_parser.add_argument("--jar", type=Path, required=True, help="Input JAR to rewrite.")
    rewrite_parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Output path for the rewritten JAR.",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    if args.command == "analyze":
        report = analyze_jar(args.jar)
    else:
        report = rewrite_jar(args.jar, args.output)
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
