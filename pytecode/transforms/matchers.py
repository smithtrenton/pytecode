"""Declarative class/field/method matchers.

Factory functions that return ``ClassMatcher``, ``FieldMatcher``, and
``MethodMatcher`` instances evaluated natively by the Rust engine::

    from pytecode.transforms.matchers import class_named, has_code
"""

from __future__ import annotations

from pytecode._rust import RustClassMatcher, RustFieldMatcher, RustMethodMatcher

# ---------------------------------------------------------------------------
# Class matchers
# ---------------------------------------------------------------------------


def class_named(name: str) -> RustClassMatcher:
    """Match by exact class name (internal JVM format)."""
    return RustClassMatcher.named(name)


def class_name_matches(pattern: str) -> RustClassMatcher:
    """Match by regex against class name."""
    return RustClassMatcher.name_matches(pattern)


def class_access(flags: int) -> RustClassMatcher:
    """Match when all given access flag bits are set."""
    return RustClassMatcher.access_all(flags)


def class_access_any(flags: int) -> RustClassMatcher:
    """Match when any of the given access flag bits are set."""
    return RustClassMatcher.access_any(flags)


def class_is_public() -> RustClassMatcher:
    """Match public classes."""
    return RustClassMatcher.access_all(0x0001)


def class_is_package_private() -> RustClassMatcher:
    """Match package-private classes."""
    return RustClassMatcher.is_package_private()


def class_is_final() -> RustClassMatcher:
    """Match final classes."""
    return RustClassMatcher.access_all(0x0010)


def class_is_interface() -> RustClassMatcher:
    """Match interface classes."""
    return RustClassMatcher.access_all(0x0200)


def class_is_abstract() -> RustClassMatcher:
    """Match abstract classes."""
    return RustClassMatcher.access_all(0x0400)


def class_is_synthetic() -> RustClassMatcher:
    """Match synthetic classes."""
    return RustClassMatcher.access_all(0x1000)


def class_is_annotation() -> RustClassMatcher:
    """Match annotation classes."""
    return RustClassMatcher.access_all(0x2000)


def class_is_enum() -> RustClassMatcher:
    """Match enum classes."""
    return RustClassMatcher.access_all(0x4000)


def class_is_module() -> RustClassMatcher:
    """Match module classes."""
    return RustClassMatcher.access_all(0x8000)


def extends(name: str) -> RustClassMatcher:
    """Match by exact super-class name."""
    return RustClassMatcher.extends(name)


def implements(name: str) -> RustClassMatcher:
    """Match when the class implements the named interface."""
    return RustClassMatcher.implements(name)


def class_version(major: int) -> RustClassMatcher:
    """Match by exact major version."""
    return RustClassMatcher.version(major)


def class_version_at_least(major: int) -> RustClassMatcher:
    """Match classes with major version >= *major*."""
    return RustClassMatcher.version_at_least(major)


def class_version_below(major: int) -> RustClassMatcher:
    """Match classes with major version < *major*."""
    return RustClassMatcher.version_below(major)


# ---------------------------------------------------------------------------
# Field matchers
# ---------------------------------------------------------------------------


def field_named(name: str) -> RustFieldMatcher:
    """Match by exact field name."""
    return RustFieldMatcher.named(name)


def field_name_matches(pattern: str) -> RustFieldMatcher:
    """Match by regex against field name."""
    return RustFieldMatcher.name_matches(pattern)


def field_descriptor(descriptor: str) -> RustFieldMatcher:
    """Match by exact field descriptor."""
    return RustFieldMatcher.descriptor(descriptor)


def field_descriptor_matches(pattern: str) -> RustFieldMatcher:
    """Match by regex against field descriptor."""
    return RustFieldMatcher.descriptor_matches(pattern)


def field_access(flags: int) -> RustFieldMatcher:
    """Match when all given access flag bits are set."""
    return RustFieldMatcher.access_all(flags)


def field_access_any(flags: int) -> RustFieldMatcher:
    """Match when any of the given access flag bits are set."""
    return RustFieldMatcher.access_any(flags)


def field_is_public() -> RustFieldMatcher:
    """Match public fields."""
    return RustFieldMatcher.access_all(0x0001)


def field_is_private() -> RustFieldMatcher:
    """Match private fields."""
    return RustFieldMatcher.access_all(0x0002)


def field_is_protected() -> RustFieldMatcher:
    """Match protected fields."""
    return RustFieldMatcher.access_all(0x0004)


def field_is_package_private() -> RustFieldMatcher:
    """Match package-private fields."""
    return RustFieldMatcher.is_package_private()


def field_is_static() -> RustFieldMatcher:
    """Match static fields."""
    return RustFieldMatcher.access_all(0x0008)


def field_is_final() -> RustFieldMatcher:
    """Match final fields."""
    return RustFieldMatcher.access_all(0x0010)


def field_is_volatile() -> RustFieldMatcher:
    """Match volatile fields."""
    return RustFieldMatcher.access_all(0x0040)


def field_is_transient() -> RustFieldMatcher:
    """Match transient fields."""
    return RustFieldMatcher.access_all(0x0080)


def field_is_synthetic() -> RustFieldMatcher:
    """Match synthetic fields."""
    return RustFieldMatcher.access_all(0x1000)


def field_is_enum_constant() -> RustFieldMatcher:
    """Match enum constant fields."""
    return RustFieldMatcher.access_all(0x4000)


# ---------------------------------------------------------------------------
# Method matchers
# ---------------------------------------------------------------------------


def method_named(name: str) -> RustMethodMatcher:
    """Match by exact method name."""
    return RustMethodMatcher.named(name)


def method_name_matches(pattern: str) -> RustMethodMatcher:
    """Match by regex against method name."""
    return RustMethodMatcher.name_matches(pattern)


def method_descriptor(descriptor: str) -> RustMethodMatcher:
    """Match by exact method descriptor."""
    return RustMethodMatcher.descriptor(descriptor)


def method_descriptor_matches(pattern: str) -> RustMethodMatcher:
    """Match by regex against method descriptor."""
    return RustMethodMatcher.descriptor_matches(pattern)


def method_access(flags: int) -> RustMethodMatcher:
    """Match when all given access flag bits are set."""
    return RustMethodMatcher.access_all(flags)


def method_access_any(flags: int) -> RustMethodMatcher:
    """Match when any of the given access flag bits are set."""
    return RustMethodMatcher.access_any(flags)


def method_is_public() -> RustMethodMatcher:
    """Match public methods."""
    return RustMethodMatcher.access_all(0x0001)


def method_is_private() -> RustMethodMatcher:
    """Match private methods."""
    return RustMethodMatcher.access_all(0x0002)


def method_is_protected() -> RustMethodMatcher:
    """Match protected methods."""
    return RustMethodMatcher.access_all(0x0004)


def method_is_package_private() -> RustMethodMatcher:
    """Match package-private methods."""
    return RustMethodMatcher.is_package_private()


def method_is_static() -> RustMethodMatcher:
    """Match static methods."""
    return RustMethodMatcher.access_all(0x0008)


def method_is_final() -> RustMethodMatcher:
    """Match final methods."""
    return RustMethodMatcher.access_all(0x0010)


def method_is_synchronized() -> RustMethodMatcher:
    """Match synchronized methods."""
    return RustMethodMatcher.access_all(0x0020)


def method_is_bridge() -> RustMethodMatcher:
    """Match bridge methods."""
    return RustMethodMatcher.access_all(0x0040)


def method_is_varargs() -> RustMethodMatcher:
    """Match varargs methods."""
    return RustMethodMatcher.access_all(0x0080)


def method_is_native() -> RustMethodMatcher:
    """Match native methods."""
    return RustMethodMatcher.access_all(0x0100)


def method_is_abstract() -> RustMethodMatcher:
    """Match abstract methods."""
    return RustMethodMatcher.access_all(0x0400)


def method_is_strict() -> RustMethodMatcher:
    """Match strictfp methods."""
    return RustMethodMatcher.access_all(0x0800)


def method_is_synthetic() -> RustMethodMatcher:
    """Match synthetic methods."""
    return RustMethodMatcher.access_all(0x1000)


def has_code() -> RustMethodMatcher:
    """Match methods with a code attribute."""
    return RustMethodMatcher.has_code()


def is_constructor() -> RustMethodMatcher:
    """Match ``<init>`` methods."""
    return RustMethodMatcher.is_constructor()


def is_static_initializer() -> RustMethodMatcher:
    """Match ``<clinit>`` methods."""
    return RustMethodMatcher.is_static_initializer()


def method_returns(descriptor: str) -> RustMethodMatcher:
    """Match by return-type descriptor."""
    return RustMethodMatcher.returns(descriptor)
