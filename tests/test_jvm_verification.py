"""Independent JVM acceptance and rejection of emitted classfiles."""

import json
import os
import struct
import subprocess
from pathlib import Path

import pytest
from helpers import _jdk_tool, compile_java_sources

from pytecode import _rust
from pytecode.archive import FrameComputationMode
from pytecode.model import BranchInsn, ClassModel, FieldModel, Label, MethodModel, RawInsn, VarInsn

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


@pytest.mark.parametrize("case", ["overwrite_this", "wrong_constructor", "foreign_field", "missing_field"])
def test_invalid_constructor_is_rejected_by_recomputation_and_jvm(tmp_path: Path, harness: Path, case: str):
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    model.methods = [MethodModel("<init>", "()V", 0x0001)]
    pool = model.constant_pool
    super_init = pool.add_methodref("java/lang/Object", "<init>", "()V").to_bytes(2, "big")
    if case == "overwrite_this":
        raw = bytes.fromhex("01 4b b1")
        error = "before initializing this"
    elif case == "wrong_constructor":
        wrong_init = pool.add_methodref("java/lang/String", "<init>", "()V").to_bytes(2, "big")
        raw = b"\x2a\xb7" + wrong_init + b"\xb1"
        error = "direct superclass"
    else:
        owner = "java/lang/Object" if case == "foreign_field" else "HelloWorld"
        field = pool.add_fieldref(owner, "missing", "I").to_bytes(2, "big")
        raw = b"\x2a\x03\xb5" + field + b"\x2a\xb7" + super_init + b"\xb1"
        error = "field declared by the current class"
    model.methods[0].set_raw_code(2, 1, raw)
    path = tmp_path / "HelloWorld.class"
    path.write_bytes(model.to_bytes())
    result = verify(harness, path)
    assert result["status"] == "VERIFY_FAIL", result
    with pytest.raises(_rust.MalformedClassException, match=error):
        model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)


@pytest.mark.parametrize("use_alias", [False, True])
def test_constructor_can_assign_own_field_and_initialize_saved_alias(tmp_path: Path, harness: Path, use_alias: bool):
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    model.fields = [FieldModel("value", "I", 0x0001)]
    model.methods = [MethodModel("<init>", "()V", 0x0001)]
    field = model.constant_pool.add_fieldref("HelloWorld", "value", "I").to_bytes(2, "big")
    super_init = model.constant_pool.add_methodref("java/lang/Object", "<init>", "()V").to_bytes(2, "big")
    raw = b"\x2a\x03\xb5" + field
    # Preserve this in local 1, overwrite local 0, and initialize through the alias.
    raw += bytes.fromhex("2a 4c 01 4b 2b") if use_alias else b"\x2a"
    raw += b"\xb7" + super_init + b"\xb1"
    model.methods[0].set_raw_code(2, 2, raw)
    path = tmp_path / "HelloWorld.class"
    path.write_bytes(model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE))
    result = verify(harness, path)
    assert result["status"] == "VERIFY_OK", result


@pytest.mark.parametrize("retry", [False, True])
def test_failed_allocation_initialization_handler(tmp_path: Path, harness: Path, retry: bool):
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    model.methods = [MethodModel("run", "()V", 0x0009)]
    pool = model.constant_pool
    object_class = pool.add_class("java/lang/Object").to_bytes(2, "big")
    init = pool.add_methodref("java/lang/Object", "<init>", "()V").to_bytes(2, "big")
    raw = b"\xbb" + object_class + b"\x59\x4b\xb7" + init + b"\xb1"
    raw += b"\x57\x2a\xb7" + init + b"\xb1" if retry else b"\xbf"
    # Catch only invokespecial. Preserve a local alias to the allocated object.
    body = struct.pack(">HHI", 2, 1, len(raw)) + raw + struct.pack(">HHHHHH", 1, 5, 8, 9, 0, 0)
    model.methods[0].set_prebuilt_code(body)
    model.version = (49, 0)  # The independent rejection must not be a missing-stack-map error.
    path = tmp_path / "HelloWorld.class"
    path.write_bytes(model.to_bytes())
    result = verify(harness, path)
    assert result["status"] == ("VERIFY_FAIL" if retry else "VERIFY_OK"), result
    model = ClassModel.from_bytes(path.read_bytes())
    model.version = (52, 0)
    if retry:
        with pytest.raises(_rust.MalformedClassException, match="slot is not initialized"):
            model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    else:
        path.write_bytes(model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE))
        result = verify(harness, path)
        assert result["status"] == "VERIFY_OK", result


def test_constructor_handler_with_unencodable_initialization_flag_is_rejected():
    model = ClassModel.from_bytes(BASE_CLASS.read_bytes())
    model.methods = [MethodModel("<init>", "()V", 0x0001)]
    init = model.constant_pool.add_methodref("java/lang/Object", "<init>", "()V").to_bytes(2, "big")
    raw = b"\x2a\xb7" + init + b"\xb1\xbf"
    body = struct.pack(">HHI", 1, 1, len(raw)) + raw + struct.pack(">HHHHHH", 1, 1, 4, 5, 0, 0)
    model.methods[0].set_prebuilt_code(body)
    model = ClassModel.from_bytes(model.to_bytes())
    with pytest.raises(_rust.MalformedClassException, match="cannot encode uninitialized constructor state"):
        model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
