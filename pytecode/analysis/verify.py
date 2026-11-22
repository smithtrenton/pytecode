"""Verification helpers for raw classfiles and mutable class models."""

from __future__ import annotations

from .. import _rust
from ..classfile import ClassFile, ClassReader
from ..model import ClassModel
from .hierarchy import MappingClassResolver

Diagnostic = _rust.Diagnostic


def _unwrap_classfile_input(value: object) -> ClassFile | ClassReader | bytes | bytearray:
    seen: set[int] = set()
    current: object = value
    while True:
        if isinstance(current, (ClassFile, ClassReader, bytes, bytearray)):
            return current
        obj_id = id(current)
        if obj_id in seen:
            break
        seen.add(obj_id)
        class_info = getattr(current, "class_info", None)
        if class_info is None:
            break
        current = class_info

    raise TypeError(
        "verify_classfile expects bytes, bytearray, ClassFile, ClassReader, or an object exposing class_info"
    )


def verify_classfile(value: object, *, fail_fast: bool = False) -> list[Diagnostic]:
    """Run the Rust verifier on classfile bytes or a Rust-backed classfile.

    ``value`` may be raw bytes, a ``bytearray``, a
    :class:`pytecode.classfile.ClassFile`, a :class:`pytecode.classfile.ClassReader`,
    or any object exposing a ``class_info`` attribute.
    """

    return _rust.rust_verify_classfile(_unwrap_classfile_input(value), fail_fast=fail_fast)


def verify_classmodel(
    value: object,
    resolver: MappingClassResolver | None = None,
    *,
    fail_fast: bool = False,
) -> list[Diagnostic]:
    """Run the Rust verifier on a mutable :class:`pytecode.model.ClassModel`.

    Pass ``resolver`` when verification needs hierarchy information, such as
    frame or override-sensitive checks.
    """

    if not isinstance(value, ClassModel):
        raise TypeError("verify_classmodel expects a ClassModel")
    return _rust.rust_verify_classmodel(value, resolver=resolver, fail_fast=fail_fast)


__all__ = ["Diagnostic", "MappingClassResolver", "verify_classfile", "verify_classmodel"]
