"""Declarative class transform factories.

Factory functions that return ``ClassTransform`` instances applied
natively by the Rust engine::

    from pytecode.transforms.class_transforms import rename_class, remove_method

    t = rename_class("com/example/NewName")
    t2 = remove_method("oldMethod")
"""

from __future__ import annotations

from pytecode._rust import RustClassTransform


def rename_class(name: str) -> RustClassTransform:
    """Rename the class to *name* (internal JVM format)."""
    return RustClassTransform.rename_class(name)


def set_access_flags(flags: int) -> RustClassTransform:
    """Set class access flags to *flags*."""
    return RustClassTransform.set_access_flags(flags)


def add_access_flags(flags: int) -> RustClassTransform:
    """Add *flags* to class access flags (bitwise OR)."""
    return RustClassTransform.add_access_flags(flags)


def remove_access_flags(flags: int) -> RustClassTransform:
    """Remove *flags* from class access flags (bitwise AND NOT)."""
    return RustClassTransform.remove_access_flags(flags)


def set_super_class(name: str) -> RustClassTransform:
    """Set super class to *name* (internal JVM format)."""
    return RustClassTransform.set_super_class(name)


def add_interface(name: str) -> RustClassTransform:
    """Add *name* to the class's interface list."""
    return RustClassTransform.add_interface(name)


def remove_interface(name: str) -> RustClassTransform:
    """Remove *name* from the class's interface list."""
    return RustClassTransform.remove_interface(name)


def remove_method(name: str, descriptor: str | None = None) -> RustClassTransform:
    """Remove method by *name*, optionally filtered by *descriptor*."""
    return RustClassTransform.remove_method(name, descriptor)


def remove_field(name: str, descriptor: str | None = None) -> RustClassTransform:
    """Remove field by *name*, optionally filtered by *descriptor*."""
    return RustClassTransform.remove_field(name, descriptor)


def rename_method(from_name: str, to_name: str) -> RustClassTransform:
    """Rename all methods named *from_name* to *to_name*."""
    return RustClassTransform.rename_method(from_name, to_name)


def rename_field(from_name: str, to_name: str) -> RustClassTransform:
    """Rename all fields named *from_name* to *to_name*."""
    return RustClassTransform.rename_field(from_name, to_name)


def set_method_access_flags(name: str, flags: int) -> RustClassTransform:
    """Set access flags of method *name* to *flags*."""
    return RustClassTransform.set_method_access_flags(name, flags)


def set_field_access_flags(name: str, flags: int) -> RustClassTransform:
    """Set access flags of field *name* to *flags*."""
    return RustClassTransform.set_field_access_flags(name, flags)


def sequence(*transforms: RustClassTransform) -> RustClassTransform:
    """Apply multiple transforms in order."""
    return RustClassTransform.sequence(list(transforms))
