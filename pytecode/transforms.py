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
from typing import Protocol, cast

from .constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from .descriptors import VoidType, parse_method_descriptor, to_descriptor
from .model import ClassModel, CodeModel, FieldModel, MethodModel

type Predicate[T] = Callable[[T], bool]
type ClassPredicate = Predicate[ClassModel]
type FieldPredicate = Predicate[FieldModel]
type MethodPredicate = Predicate[MethodModel]

_FIELD_VISIBILITY_FLAGS = FieldAccessFlag.PUBLIC | FieldAccessFlag.PRIVATE | FieldAccessFlag.PROTECTED
_METHOD_VISIBILITY_FLAGS = MethodAccessFlag.PUBLIC | MethodAccessFlag.PRIVATE | MethodAccessFlag.PROTECTED


@dataclass(frozen=True, slots=True)
class Matcher[T]:
    """Composable predicate wrapper used by the transform-selection DSL."""

    _predicate: Predicate[T]
    _description: str

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
        return Matcher(
            lambda value: self(value) and rhs(value),
            _join_descriptions("&", self._description, rhs._description),
        )

    def __rand__(self, other: Predicate[T] | Matcher[T], /) -> Matcher[T]:
        """Support ``predicate & matcher`` with a plain callable on the left."""
        lhs = _ensure_matcher(other)
        return lhs & self

    def __or__(self, other: Predicate[T] | Matcher[T], /) -> Matcher[T]:
        """Combine with *other* using logical OR."""
        rhs = _ensure_matcher(other)
        return Matcher(
            lambda value: self(value) or rhs(value),
            _join_descriptions("|", self._description, rhs._description),
        )

    def __ror__(self, other: Predicate[T] | Matcher[T], /) -> Matcher[T]:
        """Support ``predicate | matcher`` with a plain callable on the left."""
        lhs = _ensure_matcher(other)
        return lhs | self

    def __invert__(self) -> Matcher[T]:
        """Return the logical negation of this matcher."""
        return Matcher(
            lambda value: not self(value),
            f"~{_parenthesize_description(self._description)}",
        )

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

    return Matcher(combined, _combine_variadic_descriptions("&", matchers))


def any_of[T](*predicates: Predicate[T] | Matcher[T]) -> Matcher[T]:
    """Return a matcher that matches when any predicate matches."""

    matchers = tuple(_ensure_matcher(predicate) for predicate in predicates)
    if not matchers:
        return Matcher(lambda _value: False, "any_of()")

    def combined(value: T) -> bool:
        return any(matcher(value) for matcher in matchers)

    return Matcher(combined, _combine_variadic_descriptions("|", matchers))


def not_[T](predicate: Predicate[T] | Matcher[T]) -> Matcher[T]:
    """Return the negation of *predicate*."""

    return ~_ensure_matcher(predicate)


def class_named(name: str) -> ClassMatcher:
    """Match classes by internal name."""

    return _equals_matcher(lambda model: model.name, name, f"class_named({name!r})")


def class_name_matches(pattern: str) -> ClassMatcher:
    """Match classes by internal-name regex."""

    return _regex_matcher(
        lambda model: model.name,
        pattern,
        f"class_name_matches({pattern!r})",
    )


def class_access(flags: ClassAccessFlag) -> ClassMatcher:
    """Match classes containing all requested access flags."""

    return _all_flags_matcher(
        lambda model: model.access_flags,
        flags,
        f"class_access({flags!r})",
    )


def class_access_any(flags: ClassAccessFlag) -> ClassMatcher:
    """Match classes containing any requested access flag."""

    return _any_flags_matcher(
        lambda model: model.access_flags,
        flags,
        f"class_access_any({flags!r})",
    )


def class_is_public() -> ClassMatcher:
    """Match public classes."""

    return Matcher(
        lambda model: ClassAccessFlag.PUBLIC in model.access_flags,
        "class_is_public()",
    )


def class_is_package_private() -> ClassMatcher:
    """Match package-private classes."""

    return Matcher(
        lambda model: ClassAccessFlag.PUBLIC not in model.access_flags,
        "class_is_package_private()",
    )


def class_is_final() -> ClassMatcher:
    """Match final classes."""

    return _ensure_matcher(class_access(ClassAccessFlag.FINAL), "class_is_final()")


def class_is_interface() -> ClassMatcher:
    """Match interface classes."""

    return _ensure_matcher(class_access(ClassAccessFlag.INTERFACE), "class_is_interface()")


def class_is_abstract() -> ClassMatcher:
    """Match abstract classes."""

    return _ensure_matcher(class_access(ClassAccessFlag.ABSTRACT), "class_is_abstract()")


def class_is_synthetic() -> ClassMatcher:
    """Match synthetic classes."""

    return _ensure_matcher(class_access(ClassAccessFlag.SYNTHETIC), "class_is_synthetic()")


def class_is_annotation() -> ClassMatcher:
    """Match annotation classes."""

    return _ensure_matcher(class_access(ClassAccessFlag.ANNOTATION), "class_is_annotation()")


def class_is_enum() -> ClassMatcher:
    """Match enum classes."""

    return _ensure_matcher(class_access(ClassAccessFlag.ENUM), "class_is_enum()")


def class_is_module() -> ClassMatcher:
    """Match module-info classes."""

    return _ensure_matcher(class_access(ClassAccessFlag.MODULE), "class_is_module()")


def extends(name: str) -> ClassMatcher:
    """Match classes with the given direct superclass."""

    return _equals_matcher(lambda model: model.super_name, name, f"extends({name!r})")


def implements(name: str) -> ClassMatcher:
    """Match classes declaring the given direct interface."""

    return _contains_matcher(
        lambda model: model.interfaces,
        name,
        f"implements({name!r})",
    )


def class_version(major: int) -> ClassMatcher:
    """Match classes with the given major version."""

    return _equals_matcher(lambda model: model.version[0], major, f"class_version({major})")


def class_version_at_least(major: int) -> ClassMatcher:
    """Match classes whose major version is at least *major*."""

    return Matcher(
        lambda model: model.version[0] >= major,
        f"class_version_at_least({major})",
    )


def class_version_below(major: int) -> ClassMatcher:
    """Match classes whose major version is below *major*."""

    return Matcher(
        lambda model: model.version[0] < major,
        f"class_version_below({major})",
    )


def field_named(name: str) -> FieldMatcher:
    """Match fields by name."""

    return _equals_matcher(lambda field: field.name, name, f"field_named({name!r})")


def field_name_matches(pattern: str) -> FieldMatcher:
    """Match fields by name regex."""

    return _regex_matcher(
        lambda field: field.name,
        pattern,
        f"field_name_matches({pattern!r})",
    )


def field_descriptor(descriptor: str) -> FieldMatcher:
    """Match fields by descriptor."""

    return _equals_matcher(
        lambda field: field.descriptor,
        descriptor,
        f"field_descriptor({descriptor!r})",
    )


def field_descriptor_matches(pattern: str) -> FieldMatcher:
    """Match fields by descriptor regex."""

    return _regex_matcher(
        lambda field: field.descriptor,
        pattern,
        f"field_descriptor_matches({pattern!r})",
    )


def field_access(flags: FieldAccessFlag) -> FieldMatcher:
    """Match fields containing all requested access flags."""

    return _all_flags_matcher(
        lambda field: field.access_flags,
        flags,
        f"field_access({flags!r})",
    )


def field_access_any(flags: FieldAccessFlag) -> FieldMatcher:
    """Match fields containing any requested access flag."""

    return _any_flags_matcher(
        lambda field: field.access_flags,
        flags,
        f"field_access_any({flags!r})",
    )


def field_is_public() -> FieldMatcher:
    """Match public fields."""

    return _ensure_matcher(field_access(FieldAccessFlag.PUBLIC), "field_is_public()")


def field_is_private() -> FieldMatcher:
    """Match private fields."""

    return _ensure_matcher(field_access(FieldAccessFlag.PRIVATE), "field_is_private()")


def field_is_protected() -> FieldMatcher:
    """Match protected fields."""

    return _ensure_matcher(field_access(FieldAccessFlag.PROTECTED), "field_is_protected()")


def field_is_package_private() -> FieldMatcher:
    """Match package-private fields."""

    return Matcher(
        lambda field: not (field.access_flags & _FIELD_VISIBILITY_FLAGS),
        "field_is_package_private()",
    )


def field_is_static() -> FieldMatcher:
    """Match static fields."""

    return _ensure_matcher(field_access(FieldAccessFlag.STATIC), "field_is_static()")


def field_is_final() -> FieldMatcher:
    """Match final fields."""

    return _ensure_matcher(field_access(FieldAccessFlag.FINAL), "field_is_final()")


def field_is_volatile() -> FieldMatcher:
    """Match volatile fields."""

    return _ensure_matcher(field_access(FieldAccessFlag.VOLATILE), "field_is_volatile()")


def field_is_transient() -> FieldMatcher:
    """Match transient fields."""

    return _ensure_matcher(field_access(FieldAccessFlag.TRANSIENT), "field_is_transient()")


def field_is_synthetic() -> FieldMatcher:
    """Match synthetic fields."""

    return _ensure_matcher(field_access(FieldAccessFlag.SYNTHETIC), "field_is_synthetic()")


def field_is_enum_constant() -> FieldMatcher:
    """Match enum-constant fields."""

    return _ensure_matcher(field_access(FieldAccessFlag.ENUM), "field_is_enum_constant()")


def method_named(name: str) -> MethodMatcher:
    """Match methods by name."""

    return _equals_matcher(lambda method: method.name, name, f"method_named({name!r})")


def method_name_matches(pattern: str) -> MethodMatcher:
    """Match methods by name regex."""

    return _regex_matcher(
        lambda method: method.name,
        pattern,
        f"method_name_matches({pattern!r})",
    )


def method_descriptor(descriptor: str) -> MethodMatcher:
    """Match methods by descriptor."""

    return _equals_matcher(
        lambda method: method.descriptor,
        descriptor,
        f"method_descriptor({descriptor!r})",
    )


def method_descriptor_matches(pattern: str) -> MethodMatcher:
    """Match methods by descriptor regex."""

    return _regex_matcher(
        lambda method: method.descriptor,
        pattern,
        f"method_descriptor_matches({pattern!r})",
    )


def method_access(flags: MethodAccessFlag) -> MethodMatcher:
    """Match methods containing all requested access flags."""

    return _all_flags_matcher(
        lambda method: method.access_flags,
        flags,
        f"method_access({flags!r})",
    )


def method_access_any(flags: MethodAccessFlag) -> MethodMatcher:
    """Match methods containing any requested access flag."""

    return _any_flags_matcher(
        lambda method: method.access_flags,
        flags,
        f"method_access_any({flags!r})",
    )


def method_is_public() -> MethodMatcher:
    """Match public methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.PUBLIC), "method_is_public()")


def method_is_private() -> MethodMatcher:
    """Match private methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.PRIVATE), "method_is_private()")


def method_is_protected() -> MethodMatcher:
    """Match protected methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.PROTECTED), "method_is_protected()")


def method_is_package_private() -> MethodMatcher:
    """Match package-private methods."""

    return Matcher(
        lambda method: not (method.access_flags & _METHOD_VISIBILITY_FLAGS),
        "method_is_package_private()",
    )


def method_is_static() -> MethodMatcher:
    """Match static methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.STATIC), "method_is_static()")


def method_is_final() -> MethodMatcher:
    """Match final methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.FINAL), "method_is_final()")


def method_is_synchronized() -> MethodMatcher:
    """Match synchronized methods."""

    return _ensure_matcher(
        method_access(MethodAccessFlag.SYNCHRONIZED),
        "method_is_synchronized()",
    )


def method_is_bridge() -> MethodMatcher:
    """Match bridge methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.BRIDGE), "method_is_bridge()")


def method_is_varargs() -> MethodMatcher:
    """Match varargs methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.VARARGS), "method_is_varargs()")


def method_is_native() -> MethodMatcher:
    """Match native methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.NATIVE), "method_is_native()")


def method_is_abstract() -> MethodMatcher:
    """Match abstract methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.ABSTRACT), "method_is_abstract()")


def method_is_strict() -> MethodMatcher:
    """Match strictfp methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.STRICT), "method_is_strict()")


def method_is_synthetic() -> MethodMatcher:
    """Match synthetic methods."""

    return _ensure_matcher(method_access(MethodAccessFlag.SYNTHETIC), "method_is_synthetic()")


def has_code() -> MethodMatcher:
    """Match methods that currently have a ``CodeModel``."""

    return Matcher(lambda method: method.code is not None, "has_code()")


def is_constructor() -> MethodMatcher:
    """Match methods named ``<init>``."""

    return _ensure_matcher(method_named("<init>"), "is_constructor()")


def is_static_initializer() -> MethodMatcher:
    """Match methods named ``<clinit>``."""

    return _ensure_matcher(method_named("<clinit>"), "is_static_initializer()")


def method_returns(descriptor: str) -> MethodMatcher:
    """Match methods by return-type descriptor."""

    return Matcher(
        lambda method: _method_return_descriptor(method.descriptor) == descriptor,
        f"method_returns({descriptor!r})",
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
        return Matcher(existing._predicate, description)
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
) -> Matcher[T]:
    return Matcher(lambda value: accessor(value) == expected, description)


def _regex_matcher[T](
    accessor: Callable[[T], str],
    pattern: str,
    description: str,
) -> Matcher[T]:
    compiled = re.compile(pattern)
    return Matcher(lambda value: compiled.fullmatch(accessor(value)) is not None, description)


def _contains_matcher[T, U](
    accessor: Callable[[T], Iterable[U]],
    expected: U,
    description: str,
) -> Matcher[T]:
    return Matcher(lambda value: expected in accessor(value), description)


def _all_flags_matcher[T, F: IntFlag](
    accessor: Callable[[T], F],
    flags: F,
    description: str,
) -> Matcher[T]:
    return Matcher(lambda value: (accessor(value) & flags) == flags, description)


def _any_flags_matcher[T, F: IntFlag](
    accessor: Callable[[T], F],
    flags: F,
    description: str,
) -> Matcher[T]:
    return Matcher(lambda value: bool(accessor(value) & flags), description)


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
