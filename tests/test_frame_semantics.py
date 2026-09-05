"""Regressions for frame computation and JVM acceptance of emitted methods."""

import subprocess
from pathlib import Path

import pytest
from helpers import _jdk_tool, compile_java_sources

from pytecode.analysis import MappingClassResolver, verify_classfile
from pytecode.archive import FrameComputationMode
from pytecode.model import ClassModel, MethodModel

BASE_CLASS = Path("crates/pytecode-engine/fixtures/classes/HelloWorld/HelloWorld.class")


@pytest.mark.parametrize(
    ("descriptor", "raw"),
    [
        ("()I", "04 74 ac"),
        ("()J", "09 75 ad"),
        ("()F", "0b 76 ae"),
        ("()D", "0e 77 af"),
        ("()J", "09 04 79 ad"),
        ("()J", "09 04 7b ad"),
        ("()J", "09 04 7d ad"),
        ("(I)I", "15 00 ac"),
    ],
)
def test_recompute_minimal_methods(descriptor: str, raw: str) -> None:
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    method = MethodModel("run", descriptor, 0x0009)
    method.set_raw_code(4, 1, bytes.fromhex(raw))
    model.methods = [method]
    emitted = model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    assert ClassModel.from_bytes(emitted).methods[0].descriptor == descriptor


def test_recompute_rejects_dup_on_long() -> None:
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    method = MethodModel("run", "()V", 0x0009)
    method.set_raw_code(4, 0, bytes.fromhex("09 59 57 58 b1"))
    model.methods = [method]
    with pytest.raises(Exception, match="category|type|stack"):
        model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)


@pytest.mark.parametrize("with_resolver", [False, True])
def test_recomputed_javac_methods_execute_with_jvm_verification(tmp_path: Path, with_resolver: bool) -> None:
    classes = compile_java_sources(
        tmp_path,
        [
            Path("crates/pytecode-engine/fixtures/java") / name
            for name in ("FrameRegression.java", "RuntimeClasses.java")
        ],
    )
    originals = {path: path.read_bytes() for path in classes.glob("Frame*.class")}
    resolver = None
    if with_resolver:
        # The strict resolver must include the ancestors needed at array joins.
        runtime_dir = tmp_path / "runtime"
        names = [f"java/lang/{name}" for name in ("String", "Integer", "Number")]
        subprocess.run(
            [_jdk_tool("java"), "-cp", str(classes), "RuntimeClasses", str(runtime_dir), *names],
            capture_output=True,
            text=True,
            check=True,
        )
        ancestors = [(runtime_dir / f"{name}.class").read_bytes() for name in names]
        resolver = MappingClassResolver.from_bytes([*originals.values(), *ancestors])
    for path, original in originals.items():
        model = ClassModel.from_bytes(original)
        path.write_bytes(
            model.to_bytes_with_options(
                frame_mode=FrameComputationMode.RECOMPUTE,
                resolver=resolver,
            )
        )
    result = subprocess.run(
        [_jdk_tool("java"), "-Xverify:all", "-cp", str(classes), "FrameRegression"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "frames-ok"


def test_raw_verifier_fail_fast_returns_only_first_error() -> None:
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    model.access_flags = 0x0411
    model.super_name = None
    raw = model.to_bytes()
    all_diagnostics = verify_classfile(raw)
    first = verify_classfile(raw, fail_fast=True)
    assert len(all_diagnostics) >= 2
    assert len(first) == 1
    assert first[0].message == all_diagnostics[0].message
