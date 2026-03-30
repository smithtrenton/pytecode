from __future__ import annotations

from collections.abc import Callable, Iterable
from dataclasses import dataclass
from typing import Protocol

from .constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from .model import ClassModel, CodeModel, FieldModel, MethodModel

type Predicate[T] = Callable[[T], bool]
type ClassPredicate = Predicate[ClassModel]
type FieldPredicate = Predicate[FieldModel]
type MethodPredicate = Predicate[MethodModel]


class ClassTransform(Protocol):
    """In-place transform over ``ClassModel``."""

    def __call__(self, model: ClassModel, /) -> None: ...


class FieldTransform(Protocol):
    """In-place transform over ``FieldModel``."""

    def __call__(self, field: FieldModel, /) -> None: ...


class MethodTransform(Protocol):
    """In-place transform over ``MethodModel``."""

    def __call__(self, method: MethodModel, /) -> None: ...


class CodeTransform(Protocol):
    """In-place transform over ``CodeModel``."""

    def __call__(self, code: CodeModel, /) -> None: ...


@dataclass(frozen=True)
class Pipeline:
    """Composable sequence of class-level transforms applied in order.

    Pipelines are themselves callable, so they can be passed anywhere a
    ``ClassTransform`` is accepted, including ``JarFile.rewrite()``.
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
    """Conditionally apply a class transform."""

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
) -> ClassTransform:
    """Lift a field transform into a class transform.

    Traversal uses a snapshot of ``ClassModel.fields`` so collection edits do
    not change which original fields are visited within the current pass.
    """

    def lifted(model: ClassModel) -> None:
        for field in tuple(model.fields):
            if where is not None and not where(field):
                continue
            _expect_none(
                transform(field),
                "Field transforms must mutate FieldModel in place and return None",
            )

    return lifted


def on_methods(
    transform: MethodTransform,
    *,
    where: MethodPredicate | None = None,
) -> ClassTransform:
    """Lift a method transform into a class transform.

    Traversal uses a snapshot of ``ClassModel.methods`` so collection edits do
    not change which original methods are visited within the current pass.
    """

    def lifted(model: ClassModel) -> None:
        for method in tuple(model.methods):
            if where is not None and not where(method):
                continue
            _expect_none(
                transform(method),
                "Method transforms must mutate MethodModel in place and return None",
            )

    return lifted


def on_code(
    transform: CodeTransform,
    *,
    where: MethodPredicate | None = None,
) -> ClassTransform:
    """Lift a code transform into a class transform.

    Only methods that currently have code are visited. The optional *where*
    predicate is evaluated on the owning ``MethodModel``.
    """

    def lifted(model: ClassModel) -> None:
        for method in tuple(model.methods):
            if where is not None and not where(method):
                continue
            code = method.code
            if code is None:
                continue
            _expect_none(
                transform(code),
                "Code transforms must mutate CodeModel in place and return None",
            )

    return lifted


def all_of[T](*predicates: Predicate[T]) -> Predicate[T]:
    """Return a predicate that requires every predicate to match."""

    def combined(value: T) -> bool:
        return all(predicate(value) for predicate in predicates)

    return combined


def any_of[T](*predicates: Predicate[T]) -> Predicate[T]:
    """Return a predicate that matches when any predicate matches."""

    def combined(value: T) -> bool:
        return any(predicate(value) for predicate in predicates)

    return combined


def not_[T](predicate: Predicate[T]) -> Predicate[T]:
    """Return the negation of *predicate*."""

    def negated(value: T) -> bool:
        return not predicate(value)

    return negated


def class_named(name: str) -> ClassPredicate:
    """Match classes by internal name."""

    return lambda model: model.name == name


def field_named(name: str) -> FieldPredicate:
    """Match fields by name."""

    return lambda field: field.name == name


def method_named(name: str) -> MethodPredicate:
    """Match methods by name."""

    return lambda method: method.name == name


def field_descriptor(descriptor: str) -> FieldPredicate:
    """Match fields by descriptor."""

    return lambda field: field.descriptor == descriptor


def method_descriptor(descriptor: str) -> MethodPredicate:
    """Match methods by descriptor."""

    return lambda method: method.descriptor == descriptor


def class_access(flags: ClassAccessFlag) -> ClassPredicate:
    """Match classes containing all requested access flags."""

    return lambda model: (model.access_flags & flags) == flags


def field_access(flags: FieldAccessFlag) -> FieldPredicate:
    """Match fields containing all requested access flags."""

    return lambda field: (field.access_flags & flags) == flags


def method_access(flags: MethodAccessFlag) -> MethodPredicate:
    """Match methods containing all requested access flags."""

    return lambda method: (method.access_flags & flags) == flags


def has_code() -> MethodPredicate:
    """Match methods that currently have a ``CodeModel``."""

    return lambda method: method.code is not None


def _flatten_transforms(transforms: Iterable[ClassTransform | Pipeline]) -> tuple[ClassTransform, ...]:
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


__all__ = [
    "ClassPredicate",
    "ClassTransform",
    "CodeTransform",
    "FieldPredicate",
    "FieldTransform",
    "MethodPredicate",
    "MethodTransform",
    "Pipeline",
    "Predicate",
    "all_of",
    "any_of",
    "class_access",
    "class_named",
    "field_access",
    "field_descriptor",
    "field_named",
    "has_code",
    "method_access",
    "method_descriptor",
    "method_named",
    "not_",
    "on_classes",
    "on_code",
    "on_fields",
    "on_methods",
    "pipeline",
]
