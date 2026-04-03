from __future__ import annotations

import pytest

import pytecode.analysis.verify as verify_module
from pytecode.analysis.verify import Category, verify_classfile
from pytecode.classfile.attributes import CodeAttr
from pytecode.classfile.constant_pool import ConstantPoolInfo
from pytecode.classfile.constants import MethodAccessFlag
from pytecode.classfile.info import MethodInfo
from pytecode.classfile.instructions import ConstPoolIndex, InsnInfo, InsnInfoType
from tests.test_verify import _base_cp, _make_classfile, _simple_code, _utf8


def test_verify_code_wrapper_uses_rust(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[tuple[int, int, int]] = []

    def fake_rust_verify(code: CodeAttr, cp: list[object | None], major: int) -> list[tuple[str, int | None]]:
        calls.append((major, len(code.code), len(cp)))
        return [("rust sentinel", 0)]

    monkeypatch.setattr(verify_module, "_rust_verify_code", fake_rust_verify)

    code = _simple_code(insns=[InsnInfo(InsnInfoType.RETURN, 0)])
    cf = _make_classfile(
        methods_count=1,
        methods=[
            MethodInfo(
                access_flags=MethodAccessFlag.PUBLIC,
                name_index=3,
                descriptor_index=4,
                attributes_count=1,
                attributes=[code],
            )
        ],
    )

    diags = verify_classfile(cf)

    assert calls == [(cf.major_version, 1, len(cf.constant_pool))]
    code_errs = [diag for diag in diags if diag.category is Category.CODE]
    assert [(diag.message, diag.location.bytecode_offset) for diag in code_errs] == [("rust sentinel", 0)]


def test_verify_code_wrapper_falls_back_to_python(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[str] = []
    original = verify_module._verify_code_cp_refs

    def wrapped_python_path(
        code: CodeAttr,
        cp: list[ConstantPoolInfo | None],
        major: int,
        dc: verify_module._Collector,
        class_name: str | None,
        method_name: str | None,
        method_desc: str | None,
    ) -> None:
        calls.append("python")
        original(code, cp, major, dc, class_name, method_name, method_desc)

    monkeypatch.setattr(verify_module, "_rust_verify_code", None)
    monkeypatch.setattr(verify_module, "_verify_code_cp_refs", wrapped_python_path)

    cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
    code = _simple_code(insns=[ConstPoolIndex(InsnInfoType.GETFIELD, 0, index=2)], code_length=3)
    cf = _make_classfile(
        constant_pool=cp,
        constant_pool_count=7,
        methods_count=1,
        methods=[
            MethodInfo(
                access_flags=MethodAccessFlag.PUBLIC,
                name_index=5,
                descriptor_index=6,
                attributes_count=1,
                attributes=[code],
            )
        ],
    )

    diags = verify_classfile(cf)

    assert calls == ["python"]
    assert any("GETFIELD" in diag.message and "FieldrefInfo" in diag.message for diag in diags)
