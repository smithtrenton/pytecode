"""Rust-first verification entry points for classfiles and class models."""

from __future__ import annotations

from .. import _rust
from .hierarchy import MappingClassResolver

Diagnostic = _rust.Diagnostic


def _classfile_bytes(value: object) -> bytes:
    if isinstance(value, (bytes, bytearray)):
        return bytes(value)
    if isinstance(value, _rust.ClassFile):
        return bytes(value.to_bytes())

    class_info = getattr(value, "class_info", None)
    if class_info is not None:
        return _classfile_bytes(class_info)

    raise TypeError("verify_classfile expects bytes, a Rust ClassFile, or an object exposing class_info")


def verify_classfile(value: object, *, fail_fast: bool = False) -> list[Diagnostic]:
    """Verify classfile bytes or a Rust-backed classfile through the Rust verifier."""

    return _rust.rust_verify_classfile(_classfile_bytes(value), fail_fast=fail_fast)


def verify_classmodel(
    value: object,
    resolver: MappingClassResolver | None = None,
    *,
    fail_fast: bool = False,
) -> list[Diagnostic]:
    """Verify a Rust-backed class model through the Rust verifier."""

    if not isinstance(value, _rust.ClassModel):
        raise TypeError("verify_classmodel expects a ClassModel")
    return _rust.rust_verify_classmodel(value, resolver=resolver, fail_fast=fail_fast)


__all__ = ["Diagnostic", "MappingClassResolver", "verify_classfile", "verify_classmodel"]
