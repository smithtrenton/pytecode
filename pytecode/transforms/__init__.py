"""Composable transform pipeline and matcher DSL for JVM class-file models.

Provides a declarative API for selecting and transforming classes, fields,
methods, and code within JVM ``.class`` files.  Core building blocks are
:class:`Matcher` (a composable predicate wrapper), the four transform
protocols (:class:`ClassTransform`, :class:`FieldTransform`,
:class:`MethodTransform`, :class:`CodeTransform`), and :class:`Pipeline`
which sequences transforms into a single pass.
"""

from __future__ import annotations

import re
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from enum import IntFlag
from typing import Any, Protocol, cast

from .._rust import RustClassMatcher as RustClassMatcher  # type: ignore[import-untyped]  # noqa: I001
from .._rust import RustClassTransform as RustClassTransform  # type: ignore[import-untyped]
from .._rust import RustFieldMatcher as RustFieldMatcher  # type: ignore[import-untyped]
from .._rust import RustMethodMatcher as RustMethodMatcher  # type: ignore[import-untyped]
from .._rust import RustPipeline as RustPipeline  # type: ignore[import-untyped]
from ..classfile.constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from ..classfile.descriptors import VoidType, parse_method_descriptor, to_descriptor
from ..edit.model import ClassModel, CodeModel, FieldModel, MethodModel
from .rust_pipeline import RustPipelineBuilder as RustPipelineBuilder

type Predicate[T] = Callable[[T], bool]
type ClassPredicate = Predicate[ClassModel]
type FieldPredicate = Predicate[FieldModel]
type MethodPredicate = Predicate[MethodModel]

_FIELD_VISIBILITY_FLAGS = FieldAccessFlag.PUBLIC | FieldAccessFlag.PRIVATE | FieldAccessFlag.PROTECTED
_METHOD_VISIBILITY_FLAGS = MethodAccessFlag.PUBLIC | MethodAccessFlag.PRIVATE | MethodAccessFlag.PROTECTED


@dataclass(frozen=True, slots=True)
class Matcher[T]:
    """Composable predicate wrapper used by the transform-selection DSL.

    Each ``Matcher`` carries a Python predicate for evaluating against Python
    model objects and an optional ``_rust_spec`` for use with the Rust-backed
    pipeline (:class:`~pytecode.transforms.rust_pipeline.RustPipelineBuilder`).
    Factory functions (e.g. :func:`class_named`, :func:`method_is_public`)
    populate both fields automatically.  User-supplied closures produce a
    ``Matcher`` with ``_rust_spec=None``.
    """

    _predicate: Predicate[T]
    _description: str
    _rust_spec: Any = None

    @staticmethod
    def of[U](
        predicate: Predicate[U] | Matcher[U],
        description: str | None = None,
    ) -> Matcher[U]:
        """Wrap a predicate as a ``Matcher``.

        Args:
            predicate: Callable or existing matcher to wrap.
            description: Human-readable label shown in ``repr`` output.
        """
        return _ensure_matcher(predicate, description)

    def __call__(self, value: T, /) -> bool:
        """Evaluate this matcher against *value*."""
        return self._predicate(value)

    def __and__(self, other: Predicate[T] | Matcher[T], /) -> Matcher[T]:
        """Combine with *other* using logical AND."""
        rhs = _ensure_matcher(other)
        rust_spec = _combine_rust_specs("and", self._rust_spec, rhs._rust_spec)
        return Matcher(
            lambda value: self(value) and rhs(value),
            _join_descriptions("&", self._description, rhs._description),
            rust_spec,
        )

    def __rand__(self, other: Predicate[T] | Matcher[T], /) -> Matcher[T]:
        """Support ``predicate & matcher`` with a plain callable on the left."""
        lhs = _ensure_matcher(other)
        return lhs & self

    def __or__(self, other: Predicate[T] | Matcher[T], /) -> Matcher[T]:
        """Combine with *other* using logical OR."""
        rhs = _ensure_matcher(other)
        rust_spec = _combine_rust_specs("or", self._rust_spec, rhs._rust_spec)
        return Matcher(
            lambda value: self(value) or rhs(value),
            _join_descriptions("|", self._description, rhs._description),
            rust_spec,
        )

    def __ror__(self, other: Predicate[T] | Matcher[T], /) -> Matcher[T]:
        """Support ``predicate | matcher`` with a plain callable on the left."""
        lhs = _ensure_matcher(other)
        return lhs | self

    def __invert__(self) -> Matcher[T]:
        """Return the logical negation of this matcher."""
        rust_spec = ~self._rust_spec if self._rust_spec is not None else None
        return Matcher(
            lambda value: not self(value),
            f"~{_parenthesize_description(self._description)}",
            rust_spec,
        )

    @property
    def has_rust_spec(self) -> bool:
        """Whether this matcher carries a Rust-backed spec for native evaluation."""
        return self._rust_spec is not None

    def __repr__(self) -> str:
        """Return a human-readable description of this matcher."""
        return f"Matcher[{self._description}]"


type ClassMatcher = Matcher[ClassModel]
type FieldMatcher = Matcher[FieldModel]
type MethodMatcher = Matcher[MethodModel]


class ClassTransform(Protocol):
    """In-place transform applied to a ``ClassModel``."""

    def __call__(self, model: ClassModel, /) -> None:
        """Apply this transform to a class model."""
        ...


class FieldTransform(Protocol):
    """In-place transform applied to a ``FieldModel``."""

    def __call__(self, field: FieldModel, owner: ClassModel, /) -> None:
        """Apply this transform to a field.

        Args:
            field: The field to transform in place.
            owner: The class that declares the field.
        """
        ...


class MethodTransform(Protocol):
    """In-place transform applied to a ``MethodModel``."""

    def __call__(self, method: MethodModel, owner: ClassModel, /) -> None:
        """Apply this transform to a method.

        Args:
            method: The method to transform in place.
            owner: The class that declares the method.
        """
        ...


class CodeTransform(Protocol):
    """In-place transform applied to a ``CodeModel``."""

    def __call__(self, code: CodeModel, method: MethodModel, owner: ClassModel, /) -> None:
        """Apply this transform to a code attribute.

        Args:
            code: The code model to transform in place.
            method: The method that owns the code attribute.
            owner: The class that declares the method.
        """
        ...


@dataclass(frozen=True)
class Pipeline:
    """Composable sequence of class-level transforms applied in order.

    Pipelines are themselves callable and can be passed anywhere a
    ``ClassTransform`` is accepted, including ``JarFile.rewrite()``.

    Attributes:
        transforms: The ordered tuple of class transforms in this pipeline.
    """

    transforms: tuple[ClassTransform, ...] = ()

    @classmethod
    def of(cls, *transforms: ClassTransform | Pipeline) -> Pipeline:
        """Build a pipeline from class transforms and/or nested pipelines."""

        return cls(_flatten_transforms(transforms))

    def then(self, *transforms: ClassTransform | Pipeline) -> Pipeline:
        """Return a new pipeline with *transforms* appended."""

        return Pipeline((*self.transforms, *_flatten_transforms(transforms)))

    def __call__(self, model: ClassModel, /) -> None:
        """Apply all transforms in sequence to a class model."""
        for transform in self.transforms:
            _expect_none(
                transform(model),
                "Class transforms must mutate ClassModel in place and return None",
            )


def pipeline(*transforms: ClassTransform | Pipeline) -> Pipeline:
    """Return a callable ``Pipeline`` from class transforms and/or pipelines."""

    return Pipeline.of(*transforms)


def on_classes(
    transform: ClassTransform,
    *,
    where: ClassPredicate | None = None,
) -> ClassTransform:
    """Conditionally apply a class transform.

    Args:
        transform: The class-level transform to apply.
        where: Optional predicate that must match for the transform to run.
    """

    def lifted(model: ClassModel) -> None:
        if where is not None and not where(model):
            return
        _expect_none(
            transform(model),
            "Class transforms must mutate ClassModel in place and return None",
        )

    return lifted


def on_fields(
    transform: FieldTransform,
    *,
    where: FieldPredicate | None = None,
    owner: ClassPredicate | None = None,
) -> ClassTransform:
    """Lift a field transform into a class transform.

    Iterates over a snapshot of ``ClassModel.fields`` so that collection
    edits do not affect which fields are visited within the current pass.

    Args:
        transform: The field-level transform to apply.
        where: Optional predicate selecting which fields are visited.
        owner: Optional predicate gating traversal at the class level.
    """

    def lifted(model: ClassModel) -> None:
        if owner is not None and not owner(model):
            return
        for field in tuple(model.fields):
            if where is not None and not where(field):
                continue
            _expect_none(
                transform(field, model),
                "Field transforms must mutate FieldModel in place and return None",
            )

    return lifted


def on_methods(
    transform: MethodTransform,
    *,
    where: MethodPredicate | None = None,
    owner: ClassPredicate | None = None,
) -> ClassTransform:
    """Lift a method transform into a class transform.

    Iterates over a snapshot of ``ClassModel.methods`` so that collection
    edits do not affect which methods are visited within the current pass.

    Args:
        transform: The method-level transform to apply.
        where: Optional predicate selecting which methods are visited.
        owner: Optional predicate gating traversal at the class level.
    """

    def lifted(model: ClassModel) -> None:
        if owner is not None and not owner(model):
            return
        for method in tuple(model.methods):
            if where is not None and not where(method):
                continue
            _expect_none(
                transform(method, model),
                "Method transforms must mutate MethodModel in place and return None",
            )

    return lifted


def on_code(
    transform: CodeTransform,
    *,
    where: MethodPredicate | None = None,
    owner: ClassPredicate | None = None,
) -> ClassTransform:
    """Lift a code transform into a class transform.

    Only methods that currently have a ``CodeModel`` are visited.

    Args:
        transform: The code-level transform to apply.
        where: Optional predicate selecting which methods' code is visited.
        owner: Optional predicate gating traversal at the class level.
    """

    def lifted(model: ClassModel) -> None:
        if owner is not None and not owner(model):
            return
        for method in tuple(model.methods):
            if where is not None and not where(method):
                continue
            code = method.code
            if code is None:
                continue
            _expect_none(
                transform(code, method, model),
                "Code transforms must mutate CodeModel in place and return None",
            )

    return lifted


def all_of[T](*predicates: Predicate[T] | Matcher[T]) -> Matcher[T]:
    """Return a matcher that requires every predicate to match."""

    matchers = tuple(_ensure_matcher(predicate) for predicate in predicates)
    if not matchers:
        return Matcher(lambda _value: True, "all_of()")

    def combined(value: T) -> bool:
        return all(matcher(value) for matcher in matchers)

    specs = [m._rust_spec for m in matchers]
    if all(s is not None for s in specs):
        rust_spec = specs[0]
        for s in specs[1:]:
            rust_spec = rust_spec & s  # type: ignore[operator]
    else:
        rust_spec = None

    return Matcher(combined, _combine_variadic_descriptions("&", matchers), rust_spec)


def any_of[T](*predicates: Predicate[T] | Matcher[T]) -> Matcher[T]:
    """Return a matcher that matches when any predicate matches."""

    matchers = tuple(_ensure_matcher(predicate) for predicate in predicates)
    if not matchers:
        return Matcher(lambda _value: False, "any_of()")

    def combined(value: T) -> bool:
        return any(matcher(value) for matcher in matchers)

    specs = [m._rust_spec for m in matchers]
    if all(s is not None for s in specs):
        rust_spec = specs[0]
        for s in specs[1:]:
            rust_spec = rust_spec | s  # type: ignore[operator]
    else:
        rust_spec = None

    return Matcher(combined, _combine_variadic_descriptions("|", matchers), rust_spec)


def not_[T](predicate: Predicate[T] | Matcher[T]) -> Matcher[T]:
    """Return the negation of *predicate*."""

    return ~_ensure_matcher(predicate)


def class_named(name: str) -> ClassMatcher:
    """Match classes by internal name."""

    rust_spec = RustClassMatcher.named(name)
    return Matcher(lambda model: model.name == name, f"class_named({name!r})", rust_spec)


def class_name_matches(pattern: str) -> ClassMatcher:
    """Match classes by internal-name regex."""
    try:
        rust_spec = RustClassMatcher.name_matches(pattern)
    except ValueError as e:
        raise re.error(str(e)) from e
    return _regex_matcher(
        lambda model: model.name,
        pattern,
        f"class_name_matches({pattern!r})",
        rust_spec=rust_spec,
    )


def class_access(flags: ClassAccessFlag) -> ClassMatcher:
    """Match classes containing all requested access flags."""

    rust_spec = RustClassMatcher.access_all(int(flags))
    return _all_flags_matcher(
        lambda model: model.access_flags,
        flags,
        f"class_access({flags!r})",
        rust_spec=rust_spec,
    )


def class_access_any(flags: ClassAccessFlag) -> ClassMatcher:
    """Match classes containing any requested access flag."""

    rust_spec = RustClassMatcher.access_any(int(flags))
    return _any_flags_matcher(
        lambda model: model.access_flags,
        flags,
        f"class_access_any({flags!r})",
        rust_spec=rust_spec,
    )


def class_is_public() -> ClassMatcher:
    """Match public classes."""

    rust_spec = RustClassMatcher.access_all(0x0001)
    return Matcher(
        lambda model: ClassAccessFlag.PUBLIC in model.access_flags,
        "class_is_public()",
        rust_spec,
    )


def class_is_package_private() -> ClassMatcher:
    """Match package-private classes."""

    rust_spec = RustClassMatcher.is_package_private()
    return Matcher(
        lambda model: ClassAccessFlag.PUBLIC not in model.access_flags,
        "class_is_package_private()",
        rust_spec,
    )


def class_is_final() -> ClassMatcher:
    """Match final classes."""

    rust_spec = RustClassMatcher.access_all(0x0010)
    m = class_access(ClassAccessFlag.FINAL)
    return Matcher(m._predicate, "class_is_final()", rust_spec)


def class_is_interface() -> ClassMatcher:
    """Match interface classes."""

    rust_spec = RustClassMatcher.access_all(0x0200)
    m = class_access(ClassAccessFlag.INTERFACE)
    return Matcher(m._predicate, "class_is_interface()", rust_spec)


def class_is_abstract() -> ClassMatcher:
    """Match abstract classes."""

    rust_spec = RustClassMatcher.access_all(0x0400)
    m = class_access(ClassAccessFlag.ABSTRACT)
    return Matcher(m._predicate, "class_is_abstract()", rust_spec)


def class_is_synthetic() -> ClassMatcher:
    """Match synthetic classes."""

    rust_spec = RustClassMatcher.access_all(0x1000)
    m = class_access(ClassAccessFlag.SYNTHETIC)
    return Matcher(m._predicate, "class_is_synthetic()", rust_spec)


def class_is_annotation() -> ClassMatcher:
    """Match annotation classes."""

    rust_spec = RustClassMatcher.access_all(0x2000)
    m = class_access(ClassAccessFlag.ANNOTATION)
    return Matcher(m._predicate, "class_is_annotation()", rust_spec)


def class_is_enum() -> ClassMatcher:
    """Match enum classes."""

    rust_spec = RustClassMatcher.access_all(0x4000)
    m = class_access(ClassAccessFlag.ENUM)
    return Matcher(m._predicate, "class_is_enum()", rust_spec)


def class_is_module() -> ClassMatcher:
    """Match module-info classes."""

    rust_spec = RustClassMatcher.access_all(0x8000)
    m = class_access(ClassAccessFlag.MODULE)
    return Matcher(m._predicate, "class_is_module()", rust_spec)


def extends(name: str) -> ClassMatcher:
    """Match classes with the given direct superclass."""

    rust_spec = RustClassMatcher.extends(name)
    return _equals_matcher(lambda model: model.super_name, name, f"extends({name!r})", rust_spec=rust_spec)


def implements(name: str) -> ClassMatcher:
    """Match classes declaring the given direct interface."""

    rust_spec = RustClassMatcher.implements(name)
    return _contains_matcher(
        lambda model: model.interfaces,
        name,
        f"implements({name!r})",
        rust_spec=rust_spec,
    )


def class_version(major: int) -> ClassMatcher:
    """Match classes with the given major version."""

    rust_spec = RustClassMatcher.version(major)
    return _equals_matcher(lambda model: model.version[0], major, f"class_version({major})", rust_spec=rust_spec)


def class_version_at_least(major: int) -> ClassMatcher:
    """Match classes whose major version is at least *major*."""

    rust_spec = RustClassMatcher.version_at_least(major)
    return Matcher(
        lambda model: model.version[0] >= major,
        f"class_version_at_least({major})",
        rust_spec,
    )


def class_version_below(major: int) -> ClassMatcher:
    """Match classes whose major version is below *major*."""

    rust_spec = RustClassMatcher.version_below(major)
    return Matcher(
        lambda model: model.version[0] < major,
        f"class_version_below({major})",
        rust_spec,
    )


def field_named(name: str) -> FieldMatcher:
    """Match fields by name."""

    rust_spec = RustFieldMatcher.named(name)
    return _equals_matcher(lambda field: field.name, name, f"field_named({name!r})", rust_spec=rust_spec)


def field_name_matches(pattern: str) -> FieldMatcher:
    """Match fields by name regex."""
    try:
        rust_spec = RustFieldMatcher.name_matches(pattern)
    except ValueError as e:
        raise re.error(str(e)) from e
    return _regex_matcher(
        lambda field: field.name,
        pattern,
        f"field_name_matches({pattern!r})",
        rust_spec=rust_spec,
    )


def field_descriptor(descriptor: str) -> FieldMatcher:
    """Match fields by descriptor."""

    rust_spec = RustFieldMatcher.descriptor(descriptor)
    return _equals_matcher(
        lambda field: field.descriptor,
        descriptor,
        f"field_descriptor({descriptor!r})",
        rust_spec=rust_spec,
    )


def field_descriptor_matches(pattern: str) -> FieldMatcher:
    """Match fields by descriptor regex."""
    try:
        rust_spec = RustFieldMatcher.descriptor_matches(pattern)
    except ValueError as e:
        raise re.error(str(e)) from e
    return _regex_matcher(
        lambda field: field.descriptor,
        pattern,
        f"field_descriptor_matches({pattern!r})",
        rust_spec=rust_spec,
    )


def field_access(flags: FieldAccessFlag) -> FieldMatcher:
    """Match fields containing all requested access flags."""

    rust_spec = RustFieldMatcher.access_all(int(flags))
    return _all_flags_matcher(
        lambda field: field.access_flags,
        flags,
        f"field_access({flags!r})",
        rust_spec=rust_spec,
    )


def field_access_any(flags: FieldAccessFlag) -> FieldMatcher:
    """Match fields containing any requested access flag."""

    rust_spec = RustFieldMatcher.access_any(int(flags))
    return _any_flags_matcher(
        lambda field: field.access_flags,
        flags,
        f"field_access_any({flags!r})",
        rust_spec=rust_spec,
    )


def field_is_public() -> FieldMatcher:
    """Match public fields."""

    rust_spec = RustFieldMatcher.access_all(0x0001)
    m = field_access(FieldAccessFlag.PUBLIC)
    return Matcher(m._predicate, "field_is_public()", rust_spec)


def field_is_private() -> FieldMatcher:
    """Match private fields."""

    rust_spec = RustFieldMatcher.access_all(0x0002)
    m = field_access(FieldAccessFlag.PRIVATE)
    return Matcher(m._predicate, "field_is_private()", rust_spec)


def field_is_protected() -> FieldMatcher:
    """Match protected fields."""

    rust_spec = RustFieldMatcher.access_all(0x0004)
    m = field_access(FieldAccessFlag.PROTECTED)
    return Matcher(m._predicate, "field_is_protected()", rust_spec)


def field_is_package_private() -> FieldMatcher:
    """Match package-private fields."""

    rust_spec = RustFieldMatcher.is_package_private()
    return Matcher(
        lambda field: not (field.access_flags & _FIELD_VISIBILITY_FLAGS),
        "field_is_package_private()",
        rust_spec,
    )


def field_is_static() -> FieldMatcher:
    """Match static fields."""

    rust_spec = RustFieldMatcher.access_all(0x0008)
    m = field_access(FieldAccessFlag.STATIC)
    return Matcher(m._predicate, "field_is_static()", rust_spec)


def field_is_final() -> FieldMatcher:
    """Match final fields."""

    rust_spec = RustFieldMatcher.access_all(0x0010)
    m = field_access(FieldAccessFlag.FINAL)
    return Matcher(m._predicate, "field_is_final()", rust_spec)


def field_is_volatile() -> FieldMatcher:
    """Match volatile fields."""

    rust_spec = RustFieldMatcher.access_all(0x0040)
    m = field_access(FieldAccessFlag.VOLATILE)
    return Matcher(m._predicate, "field_is_volatile()", rust_spec)


def field_is_transient() -> FieldMatcher:
    """Match transient fields."""

    rust_spec = RustFieldMatcher.access_all(0x0080)
    m = field_access(FieldAccessFlag.TRANSIENT)
    return Matcher(m._predicate, "field_is_transient()", rust_spec)


def field_is_synthetic() -> FieldMatcher:
    """Match synthetic fields."""

    rust_spec = RustFieldMatcher.access_all(0x1000)
    m = field_access(FieldAccessFlag.SYNTHETIC)
    return Matcher(m._predicate, "field_is_synthetic()", rust_spec)


def field_is_enum_constant() -> FieldMatcher:
    """Match enum-constant fields."""

    rust_spec = RustFieldMatcher.access_all(0x4000)
    m = field_access(FieldAccessFlag.ENUM)
    return Matcher(m._predicate, "field_is_enum_constant()", rust_spec)


def method_named(name: str) -> MethodMatcher:
    """Match methods by name."""

    rust_spec = RustMethodMatcher.named(name)
    return _equals_matcher(lambda method: method.name, name, f"method_named({name!r})", rust_spec=rust_spec)


def method_name_matches(pattern: str) -> MethodMatcher:
    """Match methods by name regex."""
    try:
        rust_spec = RustMethodMatcher.name_matches(pattern)
    except ValueError as e:
        raise re.error(str(e)) from e
    return _regex_matcher(
        lambda method: method.name,
        pattern,
        f"method_name_matches({pattern!r})",
        rust_spec=rust_spec,
    )


def method_descriptor(descriptor: str) -> MethodMatcher:
    """Match methods by descriptor."""

    rust_spec = RustMethodMatcher.descriptor(descriptor)
    return _equals_matcher(
        lambda method: method.descriptor,
        descriptor,
        f"method_descriptor({descriptor!r})",
        rust_spec=rust_spec,
    )


def method_descriptor_matches(pattern: str) -> MethodMatcher:
    """Match methods by descriptor regex."""
    try:
        rust_spec = RustMethodMatcher.descriptor_matches(pattern)
    except ValueError as e:
        raise re.error(str(e)) from e
    return _regex_matcher(
        lambda method: method.descriptor,
        pattern,
        f"method_descriptor_matches({pattern!r})",
        rust_spec=rust_spec,
    )


def method_access(flags: MethodAccessFlag) -> MethodMatcher:
    """Match methods containing all requested access flags."""

    rust_spec = RustMethodMatcher.access_all(int(flags))
    return _all_flags_matcher(
        lambda method: method.access_flags,
        flags,
        f"method_access({flags!r})",
        rust_spec=rust_spec,
    )


def method_access_any(flags: MethodAccessFlag) -> MethodMatcher:
    """Match methods containing any requested access flag."""

    rust_spec = RustMethodMatcher.access_any(int(flags))
    return _any_flags_matcher(
        lambda method: method.access_flags,
        flags,
        f"method_access_any({flags!r})",
        rust_spec=rust_spec,
    )


def method_is_public() -> MethodMatcher:
    """Match public methods."""

    rust_spec = RustMethodMatcher.access_all(0x0001)
    m = method_access(MethodAccessFlag.PUBLIC)
    return Matcher(m._predicate, "method_is_public()", rust_spec)


def method_is_private() -> MethodMatcher:
    """Match private methods."""

    rust_spec = RustMethodMatcher.access_all(0x0002)
    m = method_access(MethodAccessFlag.PRIVATE)
    return Matcher(m._predicate, "method_is_private()", rust_spec)


def method_is_protected() -> MethodMatcher:
    """Match protected methods."""

    rust_spec = RustMethodMatcher.access_all(0x0004)
    m = method_access(MethodAccessFlag.PROTECTED)
    return Matcher(m._predicate, "method_is_protected()", rust_spec)


def method_is_package_private() -> MethodMatcher:
    """Match package-private methods."""

    rust_spec = RustMethodMatcher.is_package_private()
    return Matcher(
        lambda method: not (method.access_flags & _METHOD_VISIBILITY_FLAGS),
        "method_is_package_private()",
        rust_spec,
    )


def method_is_static() -> MethodMatcher:
    """Match static methods."""

    rust_spec = RustMethodMatcher.access_all(0x0008)
    m = method_access(MethodAccessFlag.STATIC)
    return Matcher(m._predicate, "method_is_static()", rust_spec)


def method_is_final() -> MethodMatcher:
    """Match final methods."""

    rust_spec = RustMethodMatcher.access_all(0x0010)
    m = method_access(MethodAccessFlag.FINAL)
    return Matcher(m._predicate, "method_is_final()", rust_spec)


def method_is_synchronized() -> MethodMatcher:
    """Match synchronized methods."""

    rust_spec = RustMethodMatcher.access_all(0x0020)
    m = method_access(MethodAccessFlag.SYNCHRONIZED)
    return Matcher(m._predicate, "method_is_synchronized()", rust_spec)


def method_is_bridge() -> MethodMatcher:
    """Match bridge methods."""

    rust_spec = RustMethodMatcher.access_all(0x0040)
    m = method_access(MethodAccessFlag.BRIDGE)
    return Matcher(m._predicate, "method_is_bridge()", rust_spec)


def method_is_varargs() -> MethodMatcher:
    """Match varargs methods."""

    rust_spec = RustMethodMatcher.access_all(0x0080)
    m = method_access(MethodAccessFlag.VARARGS)
    return Matcher(m._predicate, "method_is_varargs()", rust_spec)


def method_is_native() -> MethodMatcher:
    """Match native methods."""

    rust_spec = RustMethodMatcher.access_all(0x0100)
    m = method_access(MethodAccessFlag.NATIVE)
    return Matcher(m._predicate, "method_is_native()", rust_spec)


def method_is_abstract() -> MethodMatcher:
    """Match abstract methods."""

    rust_spec = RustMethodMatcher.access_all(0x0400)
    m = method_access(MethodAccessFlag.ABSTRACT)
    return Matcher(m._predicate, "method_is_abstract()", rust_spec)


def method_is_strict() -> MethodMatcher:
    """Match strictfp methods."""

    rust_spec = RustMethodMatcher.access_all(0x0800)
    m = method_access(MethodAccessFlag.STRICT)
    return Matcher(m._predicate, "method_is_strict()", rust_spec)


def method_is_synthetic() -> MethodMatcher:
    """Match synthetic methods."""

    rust_spec = RustMethodMatcher.access_all(0x1000)
    m = method_access(MethodAccessFlag.SYNTHETIC)
    return Matcher(m._predicate, "method_is_synthetic()", rust_spec)


def has_code() -> MethodMatcher:
    """Match methods that currently have a ``CodeModel``."""

    rust_spec = RustMethodMatcher.has_code()
    return Matcher(lambda method: method.code is not None, "has_code()", rust_spec)


def is_constructor() -> MethodMatcher:
    """Match methods named ``<init>``."""

    rust_spec = RustMethodMatcher.is_constructor()
    m = method_named("<init>")
    return Matcher(m._predicate, "is_constructor()", rust_spec)


def is_static_initializer() -> MethodMatcher:
    """Match methods named ``<clinit>``."""

    rust_spec = RustMethodMatcher.is_static_initializer()
    m = method_named("<clinit>")
    return Matcher(m._predicate, "is_static_initializer()", rust_spec)


def method_returns(descriptor: str) -> MethodMatcher:
    """Match methods by return-type descriptor."""

    rust_spec = RustMethodMatcher.returns(descriptor)
    return Matcher(
        lambda method: _method_return_descriptor(method.descriptor) == descriptor,
        f"method_returns({descriptor!r})",
        rust_spec,
    )


def _flatten_transforms(
    transforms: Iterable[ClassTransform | Pipeline],
) -> tuple[ClassTransform, ...]:
    flattened: list[ClassTransform] = []
    for transform in transforms:
        if isinstance(transform, Pipeline):
            flattened.extend(transform.transforms)
        else:
            flattened.append(transform)
    return tuple(flattened)


def _expect_none(result: object, message: str) -> None:
    if result is not None:
        raise TypeError(message)


def _ensure_matcher[T](
    predicate: Predicate[T] | Matcher[T],
    description: str | None = None,
) -> Matcher[T]:
    if isinstance(predicate, Matcher):
        existing = cast(Matcher[T], predicate)
        if description is None:
            return existing
        return Matcher(existing._predicate, description, existing._rust_spec)
    if not callable(predicate):
        raise TypeError("Matcher predicates must be callable")
    return Matcher(
        predicate,
        _describe_predicate(predicate) if description is None else description,
    )


def _describe_predicate(predicate: object) -> str:
    name = getattr(predicate, "__name__", None)
    if isinstance(name, str):
        return name
    return type(predicate).__name__


def _parenthesize_description(description: str) -> str:
    if description.startswith("(") and description.endswith(")"):
        return description
    if " & " not in description and " | " not in description:
        return description
    return f"({description})"


def _join_descriptions(operator: str, left: str, right: str) -> str:
    return f"({left} {operator} {right})"


def _combine_variadic_descriptions[T](
    operator: str,
    matchers: tuple[Matcher[T], ...],
) -> str:
    descriptions = tuple(matcher._description for matcher in matchers)
    if len(descriptions) == 1:
        return descriptions[0]
    joiner = f" {operator} "
    return f"({joiner.join(descriptions)})"


def _equals_matcher[T, U](
    accessor: Callable[[T], U],
    expected: U,
    description: str,
    *,
    rust_spec: Any = None,
) -> Matcher[T]:
    return Matcher(lambda value: accessor(value) == expected, description, rust_spec)


def _regex_matcher[T](
    accessor: Callable[[T], str],
    pattern: str,
    description: str,
    *,
    rust_spec: Any = None,
) -> Matcher[T]:
    compiled = re.compile(pattern)
    return Matcher(lambda value: compiled.fullmatch(accessor(value)) is not None, description, rust_spec)


def _contains_matcher[T, U](
    accessor: Callable[[T], Iterable[U]],
    expected: U,
    description: str,
    *,
    rust_spec: Any = None,
) -> Matcher[T]:
    return Matcher(lambda value: expected in accessor(value), description, rust_spec)


def _all_flags_matcher[T, F: IntFlag](
    accessor: Callable[[T], F],
    flags: F,
    description: str,
    *,
    rust_spec: Any = None,
) -> Matcher[T]:
    return Matcher(lambda value: (accessor(value) & flags) == flags, description, rust_spec)


def _any_flags_matcher[T, F: IntFlag](
    accessor: Callable[[T], F],
    flags: F,
    description: str,
    *,
    rust_spec: Any = None,
) -> Matcher[T]:
    return Matcher(lambda value: bool(accessor(value) & flags), description, rust_spec)


def _combine_rust_specs(op: str, left: Any, right: Any) -> Any:
    """Combine two optional Rust specs via ``&`` or ``|``, returning None if either is missing."""
    if left is None or right is None:
        return None
    if op == "and":
        return left & right
    return left | right


def _method_return_descriptor(descriptor: str) -> str | None:
    try:
        parsed = parse_method_descriptor(descriptor)
    except ValueError:
        return None

    return_type = parsed.return_type
    if isinstance(return_type, VoidType):
        return return_type.value
    return to_descriptor(return_type)


__all__ = [
    "ClassMatcher",
    "ClassPredicate",
    "ClassTransform",
    "CodeTransform",
    "FieldMatcher",
    "FieldPredicate",
    "FieldTransform",
    "Matcher",
    "MethodMatcher",
    "MethodPredicate",
    "MethodTransform",
    "Pipeline",
    "Predicate",
    "RustClassMatcher",
    "RustClassTransform",
    "RustFieldMatcher",
    "RustMethodMatcher",
    "RustPipeline",
    "RustPipelineBuilder",
    "all_of",
    "any_of",
    "class_access",
    "class_access_any",
    "class_is_abstract",
    "class_is_annotation",
    "class_is_enum",
    "class_is_final",
    "class_is_interface",
    "class_is_module",
    "class_is_package_private",
    "class_is_public",
    "class_is_synthetic",
    "class_name_matches",
    "class_named",
    "class_version",
    "class_version_at_least",
    "class_version_below",
    "extends",
    "field_access",
    "field_access_any",
    "field_descriptor",
    "field_descriptor_matches",
    "field_is_enum_constant",
    "field_is_final",
    "field_is_package_private",
    "field_is_private",
    "field_is_protected",
    "field_is_public",
    "field_is_static",
    "field_is_synthetic",
    "field_is_transient",
    "field_is_volatile",
    "field_name_matches",
    "field_named",
    "has_code",
    "implements",
    "is_constructor",
    "is_static_initializer",
    "method_access",
    "method_access_any",
    "method_descriptor",
    "method_descriptor_matches",
    "method_is_abstract",
    "method_is_bridge",
    "method_is_final",
    "method_is_native",
    "method_is_package_private",
    "method_is_private",
    "method_is_protected",
    "method_is_public",
    "method_is_static",
    "method_is_strict",
    "method_is_synchronized",
    "method_is_synthetic",
    "method_is_varargs",
    "method_name_matches",
    "method_named",
    "method_returns",
    "not_",
    "on_classes",
    "on_code",
    "on_fields",
    "on_methods",
    "pipeline",
]
