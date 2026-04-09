"""Benchmark comparing Python pipeline vs Rust pipeline performance.

Measures transform pipeline throughput on the guava fixture JAR.
"""

from __future__ import annotations

import time
import zipfile

from pytecode._rust import RustClassModel
from pytecode.classfile.constants import ClassAccessFlag, MethodAccessFlag
from pytecode.edit.model import ClassModel, MethodModel
from pytecode.transforms import (
    class_is_public,
    has_code,
    method_is_public,
    on_classes,
    on_methods,
    pipeline,
)
from pytecode.transforms.rust_matchers import class_is_public as rust_class_is_public
from pytecode.transforms.rust_matchers import has_code as rust_has_code
from pytecode.transforms.rust_matchers import method_is_public as rust_method_is_public
from pytecode.transforms.rust_pipeline import RustPipelineBuilder
from pytecode.transforms.rust_transforms import add_access_flags

JAR_PATH = "crates/pytecode-engine/fixtures/jars/byte-buddy-1.17.5.jar"
RUNS = 5


def _median(vals: list[float]) -> float:
    s = sorted(vals)
    return s[len(s) // 2]


def _read_class_bytes(jar_path: str) -> list[bytes]:
    """Read all .class file bytes from a JAR."""
    result: list[bytes] = []
    with zipfile.ZipFile(jar_path) as zf:
        for name in zf.namelist():
            if name.endswith(".class"):
                result.append(zf.read(name))
    return result


def bench_rust_pipeline() -> float:
    """Benchmark Rust pipeline: read in Rust, match+transform in Rust."""
    p = (
        RustPipelineBuilder()
        .on_classes(rust_class_is_public(), add_access_flags(0x0010))
        .on_methods(
            rust_method_is_public() & rust_has_code(),
            add_access_flags(0x0800),
        )
        .build()
    )
    compiled = p.compile()

    class_bytes = _read_class_bytes(JAR_PATH)
    class_count = len(class_bytes)

    read_times: list[float] = []
    apply_times: list[float] = []
    for _ in range(RUNS):
        t0 = time.perf_counter()
        models = [RustClassModel.from_bytes(b) for b in class_bytes]
        read_times.append(time.perf_counter() - t0)

        t0 = time.perf_counter()
        compiled.apply_all(models)
        apply_times.append(time.perf_counter() - t0)

    print(f"=== Rust Pipeline ({class_count} classes, {RUNS} runs) ===")
    print(f"  Read:  median {_median(read_times) * 1000:.1f}ms")
    print(f"  Apply: median {_median(apply_times) * 1000:.1f}ms")
    total = _median([r + a for r, a in zip(read_times, apply_times)])
    print(f"  Total: median {total * 1000:.1f}ms")
    print(f"  Apply runs: {[f'{t * 1000:.1f}ms' for t in apply_times]}")
    return _median(apply_times)


def bench_python_pipeline() -> float:
    """Benchmark Python pipeline: read via Python, match+transform in Python."""

    def add_final(model: ClassModel) -> None:
        model.access_flags = ClassAccessFlag(int(model.access_flags) | 0x0010)

    def add_strict(method: MethodModel, owner: ClassModel) -> None:
        del owner
        method.access_flags = MethodAccessFlag(int(method.access_flags) | 0x0800)

    p = pipeline(
        on_classes(add_final, where=class_is_public()),
        on_methods(add_strict, where=method_is_public() & has_code()),
    )

    class_bytes = _read_class_bytes(JAR_PATH)
    class_count = len(class_bytes)

    read_times: list[float] = []
    apply_times: list[float] = []
    for _ in range(RUNS):
        t0 = time.perf_counter()
        models = [ClassModel.from_bytes(b) for b in class_bytes]
        read_times.append(time.perf_counter() - t0)

        t0 = time.perf_counter()
        for model in models:
            p(model)
        apply_times.append(time.perf_counter() - t0)

    print(f"\n=== Python Pipeline ({class_count} classes, {RUNS} runs) ===")
    print(f"  Read:  median {_median(read_times) * 1000:.1f}ms")
    print(f"  Apply: median {_median(apply_times) * 1000:.1f}ms")
    total = _median([r + a for r, a in zip(read_times, apply_times)])
    print(f"  Total: median {total * 1000:.1f}ms")
    print(f"  Apply runs: {[f'{t * 1000:.1f}ms' for t in apply_times]}")
    return _median(apply_times)


def bench_mixed_pipeline() -> float:
    """Benchmark mixed pipeline: Rust matching + one Python callback + one Rust transform."""

    def add_final_cb(model: RustClassModel) -> None:
        model.access_flags = model.access_flags | 0x0010

    p = (
        RustPipelineBuilder()
        .on_classes_custom(rust_class_is_public(), add_final_cb)
        .on_methods(
            rust_method_is_public() & rust_has_code(),
            add_access_flags(0x0800),
        )
        .build()
    )
    compiled = p.compile()

    class_bytes = _read_class_bytes(JAR_PATH)
    class_count = len(class_bytes)

    read_times: list[float] = []
    apply_times: list[float] = []
    for _ in range(RUNS):
        t0 = time.perf_counter()
        models = [RustClassModel.from_bytes(b) for b in class_bytes]
        read_times.append(time.perf_counter() - t0)

        t0 = time.perf_counter()
        compiled.apply_all(models)
        apply_times.append(time.perf_counter() - t0)

    print(f"\n=== Mixed Pipeline ({class_count} classes, {RUNS} runs) ===")
    print(f"  Read:  median {_median(read_times) * 1000:.1f}ms")
    print(f"  Apply: median {_median(apply_times) * 1000:.1f}ms")
    total = _median([r + a for r, a in zip(read_times, apply_times)])
    print(f"  Total: median {total * 1000:.1f}ms")
    print(f"  Apply runs: {[f'{t * 1000:.1f}ms' for t in apply_times]}")
    return _median(apply_times)


def bench_live_view_callback() -> float:
    """Benchmark callback path that actively reads Rust bridge live views."""

    def inspect_views(model: RustClassModel) -> None:
        methods = model.methods
        fields = model.fields
        interfaces = model.interfaces
        attributes = model.attributes

        _ = len(methods)
        _ = len(fields)
        _ = len(interfaces)
        _ = len(attributes)
        _ = model.constant_pool.count()

        if len(methods):
            first = methods[0]
            _ = first.name
            _ = first.descriptor
            code = first.code
            if code is not None:
                _ = len(code.instructions)
                _ = len(code.attributes)

        if len(fields):
            first_field = fields[0]
            _ = first_field.name
            _ = first_field.descriptor
            _ = len(first_field.attributes)

        _ = list(interfaces)

    p = RustPipelineBuilder().on_classes_custom(rust_class_is_public(), inspect_views).build()
    compiled = p.compile()

    class_bytes = _read_class_bytes(JAR_PATH)
    class_count = len(class_bytes)

    read_times: list[float] = []
    apply_times: list[float] = []
    for _ in range(RUNS):
        t0 = time.perf_counter()
        models = [RustClassModel.from_bytes(b) for b in class_bytes]
        read_times.append(time.perf_counter() - t0)

        t0 = time.perf_counter()
        compiled.apply_all(models)
        apply_times.append(time.perf_counter() - t0)

    print(f"\n=== Live-View Callback ({class_count} classes, {RUNS} runs) ===")
    print(f"  Read:  median {_median(read_times) * 1000:.1f}ms")
    print(f"  Apply: median {_median(apply_times) * 1000:.1f}ms")
    total = _median([r + a for r, a in zip(read_times, apply_times)])
    print(f"  Total: median {total * 1000:.1f}ms")
    print(f"  Apply runs: {[f'{t * 1000:.1f}ms' for t in apply_times]}")
    return _median(apply_times)


if __name__ == "__main__":
    rust_apply = bench_rust_pipeline()
    mixed_apply = bench_mixed_pipeline()
    live_view_apply = bench_live_view_callback()
    python_apply = bench_python_pipeline()

    print("\n=== Speedup (apply only) ===")
    print(f"  Pure Rust vs Python:  {python_apply / rust_apply:.1f}x")
    print(f"  Mixed vs Python:      {python_apply / mixed_apply:.1f}x")
    print(f"  Live-view vs Python:  {python_apply / live_view_apply:.1f}x")
    print(f"  Pure Rust vs Mixed:   {mixed_apply / rust_apply:.1f}x (callback overhead)")
