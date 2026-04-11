"""Declarative class transform factories.

Factory functions that return ``ClassTransform`` instances applied
natively by the Rust engine::

    from pytecode.transforms.class_transforms import rename_class, remove_method

    t = rename_class("com/example/NewName")
    t2 = remove_method("oldMethod")
"""

from __future__ import annotations

from pytecode._rust import ClassTransform


def rename_class(name: str) -> ClassTransform:
    """Rename the class to *name* (internal JVM format)."""
    return ClassTransform.rename_class(name)


def set_access_flags(flags: int) -> ClassTransform:
    """Set class access flags to *flags*."""
    return ClassTransform.set_access_flags(flags)


def add_access_flags(flags: int) -> ClassTransform:
    """Add *flags* to class access flags (bitwise OR)."""
    return ClassTransform.add_access_flags(flags)


def remove_access_flags(flags: int) -> ClassTransform:
    """Remove *flags* from class access flags (bitwise AND NOT)."""
    return ClassTransform.remove_access_flags(flags)


def set_super_class(name: str) -> ClassTransform:
    """Set super class to *name* (internal JVM format)."""
    return ClassTransform.set_super_class(name)


def add_interface(name: str) -> ClassTransform:
    """Add *name* to the class's interface list."""
    return ClassTransform.add_interface(name)


def remove_interface(name: str) -> ClassTransform:
    """Remove *name* from the class's interface list."""
    return ClassTransform.remove_interface(name)


def remove_method(name: str, descriptor: str | None = None) -> ClassTransform:
    """Remove method by *name*, optionally filtered by *descriptor*."""
    return ClassTransform.remove_method(name, descriptor)


def remove_field(name: str, descriptor: str | None = None) -> ClassTransform:
    """Remove field by *name*, optionally filtered by *descriptor*."""
    return ClassTransform.remove_field(name, descriptor)


def rename_method(from_name: str, to_name: str) -> ClassTransform:
    """Rename all methods named *from_name* to *to_name*."""
    return ClassTransform.rename_method(from_name, to_name)


def rename_field(from_name: str, to_name: str) -> ClassTransform:
    """Rename all fields named *from_name* to *to_name*."""
    return ClassTransform.rename_field(from_name, to_name)


def set_method_access_flags(name: str, flags: int) -> ClassTransform:
    """Set access flags of method *name* to *flags*."""
    return ClassTransform.set_method_access_flags(name, flags)


def set_field_access_flags(name: str, flags: int) -> ClassTransform:
    """Set access flags of field *name* to *flags*."""
    return ClassTransform.set_field_access_flags(name, flags)


def sequence(*transforms: ClassTransform) -> ClassTransform:
    """Apply multiple transforms in order."""
    return ClassTransform.sequence(list(transforms))
