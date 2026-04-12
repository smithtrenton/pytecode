"""Declarative transform pipeline.

Build pipelines from matchers and transforms::

    from pytecode.transforms.matchers import class_named
    from pytecode.transforms.class_transforms import rename_class
    from pytecode.transforms.pipeline import PipelineBuilder

    p = (
        PipelineBuilder()
        .on_classes(class_named("com/example/Foo"), rename_class("com/example/Bar"))
        .build()
    )
"""

from __future__ import annotations

from collections.abc import Callable

from pytecode._rust import (
    ClassMatcher,
    ClassModel,
    ClassTransform,
    CodeTransform,
    CompiledPipeline,
    FieldMatcher,
    MethodMatcher,
    Pipeline,
)


class PipelineBuilder:
    """Fluent builder for a transform pipeline."""

    def __init__(self) -> None:
        self._pipeline = Pipeline()

    def on_classes(
        self,
        matcher: ClassMatcher,
        transform: ClassTransform,
    ) -> PipelineBuilder:
        """Add a class-level step."""
        self._pipeline.on_classes(matcher, transform)
        return self

    def on_fields(
        self,
        field_matcher: FieldMatcher,
        transform: ClassTransform,
        *,
        owner_matcher: ClassMatcher | None = None,
    ) -> PipelineBuilder:
        """Add a field-level step (optional class-level guard)."""
        self._pipeline.on_fields(field_matcher, transform, owner_matcher)
        return self

    def on_methods(
        self,
        method_matcher: MethodMatcher,
        transform: ClassTransform,
        *,
        owner_matcher: ClassMatcher | None = None,
    ) -> PipelineBuilder:
        """Add a method-level step (optional class-level guard)."""
        self._pipeline.on_methods(method_matcher, transform, owner_matcher)
        return self

    def on_code(
        self,
        method_matcher: MethodMatcher,
        transform: CodeTransform,
        *,
        owner_matcher: ClassMatcher | None = None,
    ) -> PipelineBuilder:
        """Add a code-level step for methods whose bodies should be rewritten."""
        self._pipeline.on_code(method_matcher, transform, owner_matcher)
        return self

    # -- custom callback variants --

    def on_classes_custom(
        self,
        matcher: ClassMatcher,
        callback: Callable[[ClassModel], None],
    ) -> PipelineBuilder:
        """Class step with custom Python callback (receives ``ClassModel``).

        The callback fires at most once per class that matches *matcher*.
        If the callback raises an exception, the exception is propagated by
        ``apply``/``apply_all`` after the current model finishes processing.

        Collection properties on ``ClassModel`` are live views, not eager
        snapshot lists. Use ``list(model.methods)`` / ``list(model.interfaces)``
        when a detached snapshot is actually wanted.
        """
        self._pipeline.on_classes_custom(matcher, callback)
        return self

    def on_fields_custom(
        self,
        field_matcher: FieldMatcher,
        callback: Callable[[ClassModel], None],
        *,
        owner_matcher: ClassMatcher | None = None,
    ) -> PipelineBuilder:
        """Field step with custom Python callback (receives ``ClassModel``).

        The callback receives the *class* model (not the individual matched
        field) and fires at most once per class where any field matches
        *field_matcher*.  This is class-scoped semantics: use the class model
        to inspect or mutate whichever fields you need.

        If the callback raises an exception, the exception is propagated by
        ``apply``/``apply_all`` after the current model finishes processing.

        Nested bridge collections also use live views; ``list(...)`` is the
        explicit materialization boundary.
        """
        self._pipeline.on_fields_custom(field_matcher, callback, owner_matcher)
        return self

    def on_methods_custom(
        self,
        method_matcher: MethodMatcher,
        callback: Callable[[ClassModel], None],
        *,
        owner_matcher: ClassMatcher | None = None,
    ) -> PipelineBuilder:
        """Method step with custom Python callback (receives ``ClassModel``).

        The callback receives the *class* model (not the individual matched
        method) and fires at most once per class where any method matches
        *method_matcher*.  This is class-scoped semantics: use the class model
        to inspect or mutate whichever methods you need.

        If the callback raises an exception, the exception is propagated by
        ``apply``/``apply_all`` after the current model finishes processing.

        Nested bridge collections also use live views; ``list(...)`` is the
        explicit materialization boundary.
        """
        self._pipeline.on_methods_custom(method_matcher, callback, owner_matcher)
        return self

    def build(self) -> Pipeline:
        """Return the constructed pipeline."""
        return self._pipeline

    def apply(self, model: ClassModel) -> None:
        """Apply pipeline to a single ClassModel (mutates in-place)."""
        self._pipeline.apply(model)

    def apply_all(self, models: list[ClassModel]) -> None:
        """Apply pipeline to many ClassModel objects (mutates in-place)."""
        self._pipeline.apply_all(models)

    def compile(self) -> CompiledPipeline:
        """Compile for hot-path repeated use (pre-compiles regexes)."""
        return self._pipeline.compile()

    def __len__(self) -> int:
        return len(self._pipeline)

    def __repr__(self) -> str:
        return f"PipelineBuilder(steps={len(self._pipeline)})"
