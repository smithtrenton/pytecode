"""Verification helpers for raw classfiles and mutable class models."""

from __future__ import annotations

from .. import _rust
from ..classfile import ClassFile
from ..model import ClassModel
from .hierarchy import MappingClassResolver

Diagnostic = _rust.Diagnostic


def _classfile_bytes(value: object) -> bytes:
    if isinstance(value, (bytes, bytearray)):
        return bytes(value)
    if isinstance(value, ClassFile):
        return bytes(value.to_bytes())

    class_info = getattr(value, "class_info", None)
    if class_info is not None:
        return _classfile_bytes(class_info)

    raise TypeError("verify_classfile expects bytes, a Rust ClassFile, or an object exposing class_info")


def verify_classfile(value: object, *, fail_fast: bool = False) -> list[Diagnostic]:
    """Run the Rust verifier on classfile bytes or a Rust-backed classfile.

    ``value`` may be raw bytes, a :class:`pytecode.classfile.ClassFile`, or any
    object exposing a ``class_info`` attribute such as
    :class:`pytecode.classfile.ClassReader`.
    """

    return _rust.rust_verify_classfile(_classfile_bytes(value), fail_fast=fail_fast)


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
