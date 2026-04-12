"""Factory helpers for Rust-evaluated class, field, and method matchers.

These helpers return matcher objects that can be combined with ``&``, ``|``,
and ``~`` and then reused across pipelines or direct Rust transform APIs::

    from pytecode.transforms.matchers import class_named, has_code
"""

from __future__ import annotations

from pytecode._rust import ClassMatcher, FieldMatcher, MethodMatcher

# ---------------------------------------------------------------------------
# Class matchers
# ---------------------------------------------------------------------------


def class_named(name: str) -> ClassMatcher:
    """Match classes whose internal JVM name is exactly ``name``."""
    return ClassMatcher.named(name)


def class_name_matches(pattern: str) -> ClassMatcher:
    """Match classes whose internal JVM name fully matches ``pattern``."""
    return ClassMatcher.name_matches(pattern)


def class_access(flags: int) -> ClassMatcher:
    """Match classes that contain every bit in ``flags``."""
    return ClassMatcher.access_all(flags)


def class_access_any(flags: int) -> ClassMatcher:
    """Match classes that contain at least one bit from ``flags``."""
    return ClassMatcher.access_any(flags)


def class_is_public() -> ClassMatcher:
    """Match classes with the ``public`` access flag."""
    return ClassMatcher.access_all(0x0001)


def class_is_package_private() -> ClassMatcher:
    """Match classes that declare no explicit public/protected/private flag."""
    return ClassMatcher.is_package_private()


def class_is_final() -> ClassMatcher:
    """Match classes marked ``final``."""
    return ClassMatcher.access_all(0x0010)


def class_is_interface() -> ClassMatcher:
    """Match interfaces."""
    return ClassMatcher.access_all(0x0200)


def class_is_abstract() -> ClassMatcher:
    """Match classes marked ``abstract``."""
    return ClassMatcher.access_all(0x0400)


def class_is_synthetic() -> ClassMatcher:
    """Match compiler-synthetic classes."""
    return ClassMatcher.access_all(0x1000)


def class_is_annotation() -> ClassMatcher:
    """Match annotation types."""
    return ClassMatcher.access_all(0x2000)


def class_is_enum() -> ClassMatcher:
    """Match enum classes."""
    return ClassMatcher.access_all(0x4000)


def class_is_module() -> ClassMatcher:
    """Match ``module-info`` classes."""
    return ClassMatcher.access_all(0x8000)


def extends(name: str) -> ClassMatcher:
    """Match classes whose direct superclass name is exactly ``name``."""
    return ClassMatcher.extends(name)


def implements(name: str) -> ClassMatcher:
    """Match classes that directly list ``name`` in their interface set."""
    return ClassMatcher.implements(name)


def class_version(major: int) -> ClassMatcher:
    """Match classes whose major version equals ``major``."""
    return ClassMatcher.version(major)


def class_version_at_least(major: int) -> ClassMatcher:
    """Match classes whose major version is at least ``major``."""
    return ClassMatcher.version_at_least(major)


def class_version_below(major: int) -> ClassMatcher:
    """Match classes whose major version is below ``major``."""
    return ClassMatcher.version_below(major)


# ---------------------------------------------------------------------------
# Field matchers
# ---------------------------------------------------------------------------


def field_named(name: str) -> FieldMatcher:
    """Match fields whose name is exactly ``name``."""
    return FieldMatcher.named(name)


def field_name_matches(pattern: str) -> FieldMatcher:
    """Match fields whose name fully matches ``pattern``."""
    return FieldMatcher.name_matches(pattern)


def field_descriptor(descriptor: str) -> FieldMatcher:
    """Match fields whose JVM descriptor is exactly ``descriptor``."""
    return FieldMatcher.descriptor(descriptor)


def field_descriptor_matches(pattern: str) -> FieldMatcher:
    """Match fields whose descriptor fully matches ``pattern``."""
    return FieldMatcher.descriptor_matches(pattern)


def field_access(flags: int) -> FieldMatcher:
    """Match fields that contain every bit in ``flags``."""
    return FieldMatcher.access_all(flags)


def field_access_any(flags: int) -> FieldMatcher:
    """Match fields that contain at least one bit from ``flags``."""
    return FieldMatcher.access_any(flags)


def field_is_public() -> FieldMatcher:
    """Match ``public`` fields."""
    return FieldMatcher.access_all(0x0001)


def field_is_private() -> FieldMatcher:
    """Match ``private`` fields."""
    return FieldMatcher.access_all(0x0002)


def field_is_protected() -> FieldMatcher:
    """Match ``protected`` fields."""
    return FieldMatcher.access_all(0x0004)


def field_is_package_private() -> FieldMatcher:
    """Match fields with no explicit visibility flag."""
    return FieldMatcher.is_package_private()


def field_is_static() -> FieldMatcher:
    """Match ``static`` fields."""
    return FieldMatcher.access_all(0x0008)


def field_is_final() -> FieldMatcher:
    """Match ``final`` fields."""
    return FieldMatcher.access_all(0x0010)


def field_is_volatile() -> FieldMatcher:
    """Match ``volatile`` fields."""
    return FieldMatcher.access_all(0x0040)


def field_is_transient() -> FieldMatcher:
    """Match ``transient`` fields."""
    return FieldMatcher.access_all(0x0080)


def field_is_synthetic() -> FieldMatcher:
    """Match compiler-synthetic fields."""
    return FieldMatcher.access_all(0x1000)


def field_is_enum_constant() -> FieldMatcher:
    """Match enum constant backing fields."""
    return FieldMatcher.access_all(0x4000)


# ---------------------------------------------------------------------------
# Method matchers
# ---------------------------------------------------------------------------


def method_named(name: str) -> MethodMatcher:
    """Match methods whose name is exactly ``name``."""
    return MethodMatcher.named(name)


def method_name_matches(pattern: str) -> MethodMatcher:
    """Match methods whose name fully matches ``pattern``."""
    return MethodMatcher.name_matches(pattern)


def method_descriptor(descriptor: str) -> MethodMatcher:
    """Match methods whose descriptor is exactly ``descriptor``."""
    return MethodMatcher.descriptor(descriptor)


def method_descriptor_matches(pattern: str) -> MethodMatcher:
    """Match methods whose descriptor fully matches ``pattern``."""
    return MethodMatcher.descriptor_matches(pattern)


def method_access(flags: int) -> MethodMatcher:
    """Match methods that contain every bit in ``flags``."""
    return MethodMatcher.access_all(flags)


def method_access_any(flags: int) -> MethodMatcher:
    """Match methods that contain at least one bit from ``flags``."""
    return MethodMatcher.access_any(flags)


def method_is_public() -> MethodMatcher:
    """Match ``public`` methods."""
    return MethodMatcher.access_all(0x0001)


def method_is_private() -> MethodMatcher:
    """Match ``private`` methods."""
    return MethodMatcher.access_all(0x0002)


def method_is_protected() -> MethodMatcher:
    """Match ``protected`` methods."""
    return MethodMatcher.access_all(0x0004)


def method_is_package_private() -> MethodMatcher:
    """Match methods with no explicit visibility flag."""
    return MethodMatcher.is_package_private()


def method_is_static() -> MethodMatcher:
    """Match ``static`` methods."""
    return MethodMatcher.access_all(0x0008)


def method_is_final() -> MethodMatcher:
    """Match ``final`` methods."""
    return MethodMatcher.access_all(0x0010)


def method_is_synchronized() -> MethodMatcher:
    """Match ``synchronized`` methods."""
    return MethodMatcher.access_all(0x0020)


def method_is_bridge() -> MethodMatcher:
    """Match compiler-generated bridge methods."""
    return MethodMatcher.access_all(0x0040)


def method_is_varargs() -> MethodMatcher:
    """Match methods flagged as accepting varargs."""
    return MethodMatcher.access_all(0x0080)


def method_is_native() -> MethodMatcher:
    """Match ``native`` methods."""
    return MethodMatcher.access_all(0x0100)


def method_is_abstract() -> MethodMatcher:
    """Match ``abstract`` methods."""
    return MethodMatcher.access_all(0x0400)


def method_is_strict() -> MethodMatcher:
    """Match ``strictfp`` methods."""
    return MethodMatcher.access_all(0x0800)


def method_is_synthetic() -> MethodMatcher:
    """Match compiler-synthetic methods."""
    return MethodMatcher.access_all(0x1000)


def has_code() -> MethodMatcher:
    """Match methods that carry a ``Code`` attribute."""
    return MethodMatcher.has_code()


def is_constructor() -> MethodMatcher:
    """Match instance constructors (``<init>``)."""
    return MethodMatcher.is_constructor()


def is_static_initializer() -> MethodMatcher:
    """Match static initializers (``<clinit>``)."""
    return MethodMatcher.is_static_initializer()


def method_returns(descriptor: str) -> MethodMatcher:
    """Match methods whose return descriptor equals ``descriptor``."""
    return MethodMatcher.returns(descriptor)
