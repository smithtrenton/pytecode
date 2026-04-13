"""Factory helpers for matcher-driven class transforms.

Each helper returns a Rust-native :class:`ClassTransform` that can be plugged
into :class:`pytecode.transforms.PipelineBuilder` or used directly with the
lower-level transform APIs::

    from pytecode.transforms.class_transforms import rename_class, remove_method

    t = rename_class("com/example/NewName")
    t2 = remove_method("oldMethod")
"""

from __future__ import annotations

from pytecode._rust import ClassTransform


def rename_class(name: str) -> ClassTransform:
    """Rename the owning class to the internal JVM name ``name``."""
    return ClassTransform.rename_class(name)


def set_access_flags(flags: int) -> ClassTransform:
    """Replace the class access-flag bitset with ``flags``."""
    return ClassTransform.set_access_flags(flags)


def add_access_flags(flags: int) -> ClassTransform:
    """Set the bits in ``flags`` on the class access-flag bitset."""
    return ClassTransform.add_access_flags(flags)


def remove_access_flags(flags: int) -> ClassTransform:
    """Clear the bits in ``flags`` from the class access-flag bitset."""
    return ClassTransform.remove_access_flags(flags)


def set_super_class(name: str) -> ClassTransform:
    """Change the direct superclass to the internal JVM name ``name``."""
    return ClassTransform.set_super_class(name)


def add_interface(name: str) -> ClassTransform:
    """Append the interface ``name`` if it is not already present."""
    return ClassTransform.add_interface(name)


def remove_interface(name: str) -> ClassTransform:
    """Remove the interface ``name`` from the declared interface list."""
    return ClassTransform.remove_interface(name)


def remove_method(name: str, descriptor: str | None = None) -> ClassTransform:
    """Remove methods named ``name``, optionally restricted to ``descriptor``."""
    return ClassTransform.remove_method(name, descriptor)


def remove_field(name: str, descriptor: str | None = None) -> ClassTransform:
    """Remove fields named ``name``, optionally restricted to ``descriptor``."""
    return ClassTransform.remove_field(name, descriptor)


def rename_method(from_name: str, to_name: str) -> ClassTransform:
    """Rename every method whose current name is ``from_name`` to ``to_name``."""
    return ClassTransform.rename_method(from_name, to_name)


def rename_field(from_name: str, to_name: str) -> ClassTransform:
    """Rename every field whose current name is ``from_name`` to ``to_name``."""
    return ClassTransform.rename_field(from_name, to_name)


def set_method_access_flags(name: str, flags: int) -> ClassTransform:
    """Replace the access flags of methods named ``name`` with ``flags``."""
    return ClassTransform.set_method_access_flags(name, flags)


def set_field_access_flags(name: str, flags: int) -> ClassTransform:
    """Replace the access flags of fields named ``name`` with ``flags``."""
    return ClassTransform.set_field_access_flags(name, flags)


def sequence(*transforms: ClassTransform) -> ClassTransform:
    """Compose multiple class transforms and apply them in the given order."""
    return ClassTransform.sequence(list(transforms))
