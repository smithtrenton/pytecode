"""Benchmark comparing Python pipeline vs Rust pipeline performance.

Measures transform pipeline throughput on the guava fixture JAR.
"""

from __future__ import annotations

import time
import zipfile

from pytecode._rust import RustClassModel
from pytecode.classfile.constants import ClassAccessFlag, MethodAccessFlag
from pytecode.edit.model import ClassModel
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
    result = []
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

    read_times: list[float] = []
    apply_times: list[float] = []
    for _ in range(RUNS):
        t0 = time.perf_counter()
        models = [RustClassModel.from_bytes(b) for b in class_bytes]
        read_times.append(time.perf_counter() - t0)

        t0 = time.perf_counter()
        compiled.apply_all(models)
        apply_times.append(time.perf_counter() - t0)

    print(f"=== Rust Pipeline ({len(models)} classes, {RUNS} runs) ===")
    print(f"  Read:  median {_median(read_times)*1000:.1f}ms")
    print(f"  Apply: median {_median(apply_times)*1000:.1f}ms")
    total = _median([r + a for r, a in zip(read_times, apply_times)])
    print(f"  Total: median {total*1000:.1f}ms")
    print(f"  Apply runs: {[f'{t*1000:.1f}ms' for t in apply_times]}")
    return _median(apply_times)


def bench_python_pipeline() -> float:
    """Benchmark Python pipeline: read via Python, match+transform in Python."""

    def add_final(model: ClassModel) -> None:
        model.access_flags = ClassAccessFlag(int(model.access_flags) | 0x0010)

    def add_strict(method, owner) -> None:  # noqa: ANN001
        method.access_flags = MethodAccessFlag(int(method.access_flags) | 0x0800)

    p = pipeline(
        on_classes(add_final, where=class_is_public()),
        on_methods(add_strict, where=method_is_public() & has_code()),
    )

    class_bytes = _read_class_bytes(JAR_PATH)

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

    print(f"\n=== Python Pipeline ({len(models)} classes, {RUNS} runs) ===")
    print(f"  Read:  median {_median(read_times)*1000:.1f}ms")
    print(f"  Apply: median {_median(apply_times)*1000:.1f}ms")
    total = _median([r + a for r, a in zip(read_times, apply_times)])
    print(f"  Total: median {total*1000:.1f}ms")
    print(f"  Apply runs: {[f'{t*1000:.1f}ms' for t in apply_times]}")
    return _median(apply_times)


if __name__ == "__main__":
    rust_apply = bench_rust_pipeline()
    python_apply = bench_python_pipeline()

    print("\n=== Speedup ===")
    print(f"  Apply: {python_apply / rust_apply:.1f}x faster with Rust pipeline")
