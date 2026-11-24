"""Independent JVM acceptance and rejection of emitted classfiles."""

import json
import os
import struct
import subprocess
from pathlib import Path

import pytest
from helpers import TEST_RESOURCES, _jdk_tool, compile_java_sources

from pytecode import _rust
from pytecode.analysis import verify_classfile, verify_classmodel
from pytecode.archive import FrameComputationMode
from pytecode.model import (
    BranchInsn,
    ClassModel,
    CodeModel,
    ExceptionHandler,
    FieldModel,
    Label,
    MethodInsn,
    MethodModel,
    RawInsn,
    TypeInsn,
    VarInsn,
)

BASE_CLASS = Path(__file__).resolve().parents[1] / "crates/pytecode-engine/fixtures/classes/HelloWorld/HelloWorld.class"


@pytest.fixture(scope="module")
def harness(tmp_path_factory: pytest.TempPathFactory) -> Path:
    return compile_java_sources(
        tmp_path_factory.mktemp("verifier"),
        [TEST_RESOURCES / "VerifierHarness.java"],
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


@pytest.mark.parametrize("shape", ["tail", "head", "middle", "all_dead", "nested_overlapping", "constructor"])
def test_dead_code_inside_exception_range_lowers_and_verifies_on_jvm(tmp_path: Path, harness: Path, shape: str):
    """Dead code inside a protected try range lowers to JVM-verifiable bytecode for all shapes."""
    model = ClassModel("DeadRangeCase", super_name="java/lang/Object", access_flags=0x0021, version=(52, 0))
    init = MethodModel("<init>", "()V", 0x0001)
    init.code = CodeModel(
        1,
        1,
        [
            VarInsn(0x19, 0),
            MethodInsn(0xB7, "java/lang/Object", "<init>", "()V", False),
            RawInsn(0xB1),
        ],
    )

    throw_npe = [
        TypeInsn(0xBB, "java/lang/NullPointerException"),
        RawInsn(0x59),
        MethodInsn(0xB7, "java/lang/NullPointerException", "<init>", "()V", False),
        RawInsn(0xBF),
    ]
    throw_iae = [
        TypeInsn(0xBB, "java/lang/IllegalArgumentException"),
        RawInsn(0x59),
        MethodInsn(0xB7, "java/lang/IllegalArgumentException", "<init>", "()V", False),
        RawInsn(0xBF),
    ]
    dead_block = [
        VarInsn(0x19, 0),
        RawInsn(0x57),
        RawInsn(0x03),
        RawInsn(0x57),
    ]

    try_start = Label.named("try_start")
    try_end = Label.named("try_end")
    handler = Label.named("handler")
    live_mid = Label.named("live_mid")
    exit_lbl = Label.named("exit")

    if shape == "tail":
        method = MethodModel("run", "(I)I", 0x0001)
        method.code = CodeModel(
            2,
            2,
            [
                try_start,
                VarInsn(0x15, 1),
                BranchInsn(0x99, exit_lbl),
                *throw_npe,
                *dead_block,
                try_end,
                exit_lbl,
                RawInsn(0x04),
                RawInsn(0xAC),
                handler,
                RawInsn(0x57),
                VarInsn(0x19, 0),  # handler requires `this` in local 0
                RawInsn(0x57),
                VarInsn(0x15, 1),  # and int parameter in local 1
                RawInsn(0xAC),
            ],
            [ExceptionHandler(try_start, try_end, handler, "java/lang/RuntimeException")],
        )
        model.methods = [init, method]
    elif shape == "head":
        method = MethodModel("run", "(I)I", 0x0001)
        method.code = CodeModel(
            2,
            2,
            [
                BranchInsn(0xA7, live_mid),
                try_start,
                *dead_block,
                live_mid,
                VarInsn(0x15, 1),
                BranchInsn(0x99, exit_lbl),
                *throw_npe,
                try_end,
                exit_lbl,
                RawInsn(0x04),
                RawInsn(0xAC),
                handler,
                RawInsn(0x57),
                VarInsn(0x19, 0),
                RawInsn(0x57),
                RawInsn(0x05),
                RawInsn(0xAC),
            ],
            [ExceptionHandler(try_start, try_end, handler, "java/lang/RuntimeException")],
        )
        model.methods = [init, method]
    elif shape == "middle":
        method = MethodModel("run", "(I)I", 0x0001)
        method.code = CodeModel(
            2,
            2,
            [
                try_start,
                VarInsn(0x15, 1),
                BranchInsn(0x99, live_mid),
                *throw_npe,
                *dead_block,
                live_mid,
                *throw_iae,
                try_end,
                handler,
                RawInsn(0x57),
                VarInsn(0x19, 0),
                RawInsn(0x57),
                VarInsn(0x15, 1),
                RawInsn(0xAC),
            ],
            [ExceptionHandler(try_start, try_end, handler, "java/lang/RuntimeException")],
        )
        model.methods = [init, method]
    elif shape == "all_dead":
        method = MethodModel("run", "(I)I", 0x0001)
        method.code = CodeModel(
            2,
            2,
            [
                VarInsn(0x15, 1),
                RawInsn(0xAC),
                try_start,
                *dead_block,
                try_end,
                handler,
                RawInsn(0x57),
                VarInsn(0x19, 0),
                RawInsn(0x57),
                RawInsn(0x03),
                RawInsn(0xAC),
            ],
            [ExceptionHandler(try_start, try_end, handler, None)],
        )
        model.methods = [init, method]
    elif shape == "nested_overlapping":
        outer_start = Label.named("outer_start")
        outer_end = Label.named("outer_end")
        inner_handler = Label.named("inner_handler")
        outer_handler = Label.named("outer_handler")
        method = MethodModel("run", "(I)I", 0x0001)
        method.code = CodeModel(
            2,
            2,
            [
                outer_start,
                RawInsn(0x00),
                try_start,
                VarInsn(0x15, 1),
                BranchInsn(0x99, live_mid),
                *throw_npe,
                *dead_block,
                live_mid,
                *throw_iae,
                try_end,
                outer_end,
                inner_handler,
                RawInsn(0x57),
                VarInsn(0x19, 0),
                RawInsn(0x57),
                RawInsn(0x04),
                RawInsn(0xAC),
                outer_handler,
                RawInsn(0x57),
                VarInsn(0x19, 0),
                RawInsn(0x57),
                RawInsn(0x05),
                RawInsn(0xAC),
            ],
            [
                ExceptionHandler(try_start, try_end, inner_handler, "java/lang/NullPointerException"),
                ExceptionHandler(outer_start, outer_end, outer_handler, "java/lang/RuntimeException"),
            ],
        )
        model.methods = [init, method]
    else:
        # Constructor with dead code inside a try block after super().
        ctor = MethodModel("<init>", "(I)V", 0x0001)
        ctor.code = CodeModel(
            2,
            2,
            [
                VarInsn(0x19, 0),
                MethodInsn(0xB7, "java/lang/Object", "<init>", "()V", False),
                try_start,
                VarInsn(0x15, 1),
                BranchInsn(0x99, exit_lbl),
                *throw_npe,
                *dead_block,
                try_end,
                exit_lbl,
                RawInsn(0xB1),
                handler,
                RawInsn(0x57),
                VarInsn(0x19, 0),
                RawInsn(0x57),
                RawInsn(0xB1),
            ],
            [ExceptionHandler(try_start, try_end, handler, "java/lang/RuntimeException")],
        )
        model.methods = [init, ctor]

    emitted = model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    assert [d.message for d in verify_classfile(emitted) if d.severity == "error"] == []
    assert [d.message for d in verify_classmodel(ClassModel.from_bytes(emitted)) if d.severity == "error"] == []

    path = tmp_path / "DeadRangeCase.class"
    path.write_bytes(emitted)
    result = verify(harness, path)
    assert result["status"] == "VERIFY_OK", result

    # For split-handler shapes, execute both live segments under -Xverify:all to confirm
    # exception dispatch semantics are preserved across the split.
    if shape in ("middle", "nested_overlapping"):
        driver_src = tmp_path / "Driver.java"
        expected = "0 1" if shape == "middle" else "2 1"
        driver_src.write_text(
            """
            public class Driver {
                public static void main(String[] args) {
                    DeadRangeCase c = new DeadRangeCase();
                    System.out.println(c.run(0) + " " + c.run(1));
                }
            }
            """,
            encoding="utf-8",
        )
        subprocess.run(
            [_jdk_tool("javac"), "--release", "17", "-cp", str(tmp_path), "-d", str(tmp_path), str(driver_src)],
            check=True,
            capture_output=True,
            text=True,
        )
        run = subprocess.run(
            [_jdk_tool("java"), "-Xverify:all", "-cp", str(tmp_path), "Driver"],
            check=False,
            capture_output=True,
            text=True,
        )
        assert run.returncode == 0, run.stderr
        assert run.stdout.strip() == expected


def test_verifier_matches_hotspot_on_incompatible_dead_frame_in_exception_range(tmp_path: Path, harness: Path):
    """A classfile with a dead `{}`-locals StackMapTable frame inside a try range fails both HotSpot and pytecode."""
    model = ClassModel("BrokenDeadTry", super_name="java/lang/Object", access_flags=0x0021, version=(52, 0))
    init = MethodModel("<init>", "()V", 0x0001)
    init.code = CodeModel(
        1,
        1,
        [
            VarInsn(0x19, 0),
            MethodInsn(0xB7, "java/lang/Object", "<init>", "()V", False),
            RawInsn(0xB1),
        ],
    )
    pool = model.constant_pool
    this_cls = pool.add_class("BrokenDeadTry")
    throwable_cls = pool.add_class("java/lang/Throwable")
    smt_name = pool.add_utf8("StackMapTable")

    # Bytecode:
    #   0: aconst_null
    #   1: athrow
    #   2: athrow        <-- dead, full_frame(offset_delta=2, locals=[], stack=[Throwable])
    #   3: pop           <-- handler, full_frame(offset_delta=0, locals=[BrokenDeadTry], stack=[Throwable])
    #   4: aload_0
    #   5: pop
    #   6: return
    # Exception table: [0, 3) -> 3 (catch-all) -- STILL covers dead pc 2!
    raw_code = bytes([0x01, 0xBF, 0xBF, 0x57, 0x2A, 0x57, 0xB1])
    frame_dead = struct.pack(">BHHHBH", 255, 2, 0, 1, 7, throwable_cls)
    frame_handler = struct.pack(">BHHBHHBH", 255, 0, 1, 7, this_cls, 1, 7, throwable_cls)
    smt_payload = struct.pack(">H", 2) + frame_dead + frame_handler
    smt_attr = struct.pack(">HI", smt_name, len(smt_payload)) + smt_payload
    code_body = (
        struct.pack(">HHI", 1, 1, len(raw_code)) + raw_code + struct.pack(">HHHHHH", 1, 0, 3, 3, 0, 1) + smt_attr
    )
    broken_method = MethodModel("run", "()V", 0x0001)
    broken_method.set_prebuilt_code(code_body)
    model.methods = [init, broken_method]

    broken_bytes = model.to_bytes()
    path = tmp_path / "BrokenDeadTry.class"
    path.write_bytes(broken_bytes)

    # HotSpot rejects the class because frame at pc 2 has `{}` locals while handler at pc 3 expects `this`.
    hotspot_before = verify(harness, path)
    assert hotspot_before["status"] == "VERIFY_FAIL", hotspot_before

    # Both pytecode verifiers agree with HotSpot and report an error.
    cf_errors = [d.message for d in verify_classfile(broken_bytes) if d.severity == "error"]
    assert any("does not match exception handler" in msg for msg in cf_errors), cf_errors

    lifted = ClassModel.from_bytes(broken_bytes)
    cm_errors = [d.message for d in verify_classmodel(lifted) if d.severity == "error"]
    assert any("does not match exception handler" in msg for msg in cm_errors), cm_errors

    # Re-lowering with RECOMPUTE repairs the class so HotSpot and both pytecode verifiers accept it.
    repaired_bytes = lifted.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    assert [d.message for d in verify_classfile(repaired_bytes) if d.severity == "error"] == []
    assert [d.message for d in verify_classmodel(ClassModel.from_bytes(repaired_bytes)) if d.severity == "error"] == []
    path.write_bytes(repaired_bytes)
    hotspot_after = verify(harness, path)
    assert hotspot_after["status"] == "VERIFY_OK", hotspot_after
