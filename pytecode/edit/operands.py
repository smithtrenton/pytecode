"""Symbolic instruction operand wrappers for the editing model.

This module re-exports from either the Cython-accelerated implementation
(``_operands_cy``) or the pure-Python fallback (``_operands_py``)
depending on availability and the ``PYTECODE_BLOCK_CYTHON`` environment
variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.edit._operands_py import _BASE_TO_WIDE as _BASE_TO_WIDE
    from pytecode.edit._operands_py import _IMPLICIT_VAR_SLOTS as _IMPLICIT_VAR_SLOTS
    from pytecode.edit._operands_py import _VAR_SHORTCUTS as _VAR_SHORTCUTS
    from pytecode.edit._operands_py import _WIDE_TO_BASE as _WIDE_TO_BASE
    from pytecode.edit._operands_py import FieldInsn as FieldInsn
    from pytecode.edit._operands_py import IIncInsn as IIncInsn
    from pytecode.edit._operands_py import InterfaceMethodInsn as InterfaceMethodInsn
    from pytecode.edit._operands_py import InvokeDynamicInsn as InvokeDynamicInsn
    from pytecode.edit._operands_py import LdcClass as LdcClass
    from pytecode.edit._operands_py import LdcDouble as LdcDouble
    from pytecode.edit._operands_py import LdcDynamic as LdcDynamic
    from pytecode.edit._operands_py import LdcFloat as LdcFloat
    from pytecode.edit._operands_py import LdcInsn as LdcInsn
    from pytecode.edit._operands_py import LdcInt as LdcInt
    from pytecode.edit._operands_py import LdcLong as LdcLong
    from pytecode.edit._operands_py import LdcMethodHandle as LdcMethodHandle
    from pytecode.edit._operands_py import LdcMethodType as LdcMethodType
    from pytecode.edit._operands_py import LdcString as LdcString
    from pytecode.edit._operands_py import LdcValue as LdcValue
    from pytecode.edit._operands_py import MethodInsn as MethodInsn
    from pytecode.edit._operands_py import MultiANewArrayInsn as MultiANewArrayInsn
    from pytecode.edit._operands_py import TypeInsn as TypeInsn
    from pytecode.edit._operands_py import VarInsn as VarInsn
    from pytecode.edit._operands_py import _require_i2 as _require_i2
    from pytecode.edit._operands_py import _require_u1 as _require_u1
    from pytecode.edit._operands_py import _require_u2 as _require_u2
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.edit._operands_cy",
        "pytecode.edit._operands_py",
    )

    FieldInsn = _impl.FieldInsn
    IIncInsn = _impl.IIncInsn
    InterfaceMethodInsn = _impl.InterfaceMethodInsn
    InvokeDynamicInsn = _impl.InvokeDynamicInsn
    LdcClass = _impl.LdcClass
    LdcDouble = _impl.LdcDouble
    LdcDynamic = _impl.LdcDynamic
    LdcFloat = _impl.LdcFloat
    LdcInsn = _impl.LdcInsn
    LdcInt = _impl.LdcInt
    LdcLong = _impl.LdcLong
    LdcMethodHandle = _impl.LdcMethodHandle
    LdcMethodType = _impl.LdcMethodType
    LdcString = _impl.LdcString
    MethodInsn = _impl.MethodInsn
    MultiANewArrayInsn = _impl.MultiANewArrayInsn
    TypeInsn = _impl.TypeInsn
    VarInsn = _impl.VarInsn
    _BASE_TO_WIDE = _impl._BASE_TO_WIDE
    _IMPLICIT_VAR_SLOTS = _impl._IMPLICIT_VAR_SLOTS
    _VAR_SHORTCUTS = _impl._VAR_SHORTCUTS
    _WIDE_TO_BASE = _impl._WIDE_TO_BASE
    _require_i2 = _impl._require_i2
    _require_u1 = _impl._require_u1
    _require_u2 = _impl._require_u2

    from pytecode.edit._operands_py import LdcValue as LdcValue  # noqa: E402

__all__ = [
    "FieldInsn",
    "IIncInsn",
    "InterfaceMethodInsn",
    "InvokeDynamicInsn",
    "LdcClass",
    "LdcDouble",
    "LdcDynamic",
    "LdcFloat",
    "LdcInsn",
    "LdcInt",
    "LdcLong",
    "LdcMethodHandle",
    "LdcMethodType",
    "LdcString",
    "LdcValue",
    "MethodInsn",
    "MultiANewArrayInsn",
    "TypeInsn",
    "VarInsn",
]
