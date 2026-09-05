"""Independent JVM acceptance and rejection of emitted classfiles."""

import json
import os
import subprocess
from pathlib import Path

import pytest
from helpers import _jdk_tool, compile_java_sources

from pytecode import _rust
from pytecode.archive import FrameComputationMode
from pytecode.model import BranchInsn, ClassModel, Label, MethodModel, RawInsn, VarInsn

BASE_CLASS = Path("crates/pytecode-engine/fixtures/classes/HelloWorld/HelloWorld.class")


@pytest.fixture(scope="module")
def harness(tmp_path_factory: pytest.TempPathFactory) -> Path:
    return compile_java_sources(
        tmp_path_factory.mktemp("verifier"),
        [Path("crates/pytecode-engine/fixtures/java/VerifierHarness.java")],
        release=17,
    )


def verify(harness: Path, path: Path, *args: str) -> dict[str, str]:
    result = subprocess.run(
        [
            _jdk_tool("java"),
            "-Xverify:all",
            "-cp",
            os.pathsep.join([str(harness), str(path.parent)]),
            "VerifierHarness",
            str(path),
            *args,
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


@pytest.mark.parametrize(
    "raw,descriptor",
    [
        ("09 59 57 58 b1", "()V"),  # dup cannot split a long
        ("15 04 ac", "(I)I"),  # uninitialized local
        ("09 ac", "()I"),  # wrong operand type for ireturn
    ],
)
def test_harness_rejects_invalid_methods_without_executing_them(
    tmp_path: Path,
    harness: Path,
    raw: str,
    descriptor: str,
):
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    method = MethodModel("run", descriptor, 0x0009)
    method.set_raw_code(4, 8, bytes.fromhex(raw))
    model.methods = [method]
    path = tmp_path / "HelloWorld.class"
    path.write_bytes(model.to_bytes())
    assert verify(harness, path)["status"] == "VERIFY_FAIL"


@pytest.mark.parametrize(
    "raw,descriptor,max_locals",
    [
        ("06 ac", "()I", 0),
        ("04 c4 36 01 00 c4 15 01 00 ac", "()I", 257),
    ],
)
def test_harness_accepts_recomputed_methods(tmp_path: Path, harness: Path, raw: str, descriptor: str, max_locals: int):
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    method = MethodModel("run", descriptor, 0x0009)
    method.set_raw_code(4, max_locals, bytes.fromhex(raw))
    model.methods = [method]
    path = tmp_path / "HelloWorld.class"
    path.write_bytes(model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE))
    assert verify(harness, path)["status"] == "VERIFY_OK"
    assert verify(harness, path, "execute", "HelloWorld")["status"] == "EXEC_FAIL"


def test_legacy_subroutine_verifies_on_jvm(tmp_path: Path, harness: Path):
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    model.version = (49, 0)
    method = MethodModel("run", "()V", 0x0009)
    method.set_raw_code(1, 1, bytes.fromhex("a8 00 04 b1 4b a9 00"))
    model.methods = [method]
    path = tmp_path / "HelloWorld.class"
    path.write_bytes(model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE))
    assert verify(harness, path)["status"] == "VERIFY_OK"


def test_conditional_branch_widening_verifies_on_jvm(tmp_path: Path, harness: Path):
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    method = MethodModel("run", "(I)I", 0x0009)
    method.set_raw_code(1, 1, bytes.fromhex("03 ac"))
    model.methods = [method]
    target = Label()
    transform = _rust.CodeTransform.replace_insn(
        _rust.InsnMatcher.opcode(0x03),
        [
            VarInsn(0x15, 0),
            BranchInsn(0x99, target),
            *([RawInsn(0)] * 35_000),
            RawInsn(0x04),
            RawInsn(0xAC),
            target,
            RawInsn(0x03),
        ],
    )
    pipeline = _rust.Pipeline()
    pipeline.on_code(_rust.MethodMatcher.named("run"), transform)
    pipeline.apply(model)
    path = tmp_path / "HelloWorld.class"
    path.write_bytes(model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE))
    assert verify(harness, path)["status"] == "VERIFY_OK"
