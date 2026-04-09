"""Rust-backed declarative transform pipeline.

Build pipelines from Rust matchers and transforms::

    from pytecode.transforms.rust_matchers import class_named
    from pytecode.transforms.rust_transforms import rename_class
    from pytecode.transforms.rust_pipeline import RustPipelineBuilder

    p = (
        RustPipelineBuilder()
        .on_classes(class_named("com/example/Foo"), rename_class("com/example/Bar"))
        .build()
    )
"""

from __future__ import annotations

from pytecode._rust import (
    RustClassMatcher,
    RustClassModel,
    RustClassTransform,
    RustCompiledPipeline,
    RustFieldMatcher,
    RustMethodMatcher,
    RustPipeline,
)


class RustPipelineBuilder:
    """Fluent builder for a ``RustPipeline``."""

    def __init__(self) -> None:
        self._pipeline = RustPipeline()

    def on_classes(
        self,
        matcher: RustClassMatcher,
        transform: RustClassTransform,
    ) -> RustPipelineBuilder:
        """Add a class-level step."""
        self._pipeline.on_classes(matcher, transform)
        return self

    def on_fields(
        self,
        field_matcher: RustFieldMatcher,
        transform: RustClassTransform,
        *,
        owner_matcher: RustClassMatcher | None = None,
    ) -> RustPipelineBuilder:
        """Add a field-level step (optional class-level guard)."""
        self._pipeline.on_fields(field_matcher, transform, owner_matcher)
        return self

    def on_methods(
        self,
        method_matcher: RustMethodMatcher,
        transform: RustClassTransform,
        *,
        owner_matcher: RustClassMatcher | None = None,
    ) -> RustPipelineBuilder:
        """Add a method-level step (optional class-level guard)."""
        self._pipeline.on_methods(method_matcher, transform, owner_matcher)
        return self

    # -- custom callback variants --

    def on_classes_custom(
        self,
        matcher: RustClassMatcher,
        callback: object,
    ) -> RustPipelineBuilder:
        """Class step with custom Python callback (receives RustClassModel).

        Collection properties on ``RustClassModel`` are live views, not eager
        snapshot lists. Use ``list(model.methods)`` / ``list(model.interfaces)``
        when a detached snapshot is actually wanted.
        """
        self._pipeline.on_classes_custom(matcher, callback)
        return self

    def on_fields_custom(
        self,
        field_matcher: RustFieldMatcher,
        callback: object,
        *,
        owner_matcher: RustClassMatcher | None = None,
    ) -> RustPipelineBuilder:
        """Field step with custom Python callback.

        Nested Rust bridge collections also use live views; ``list(...)`` is the
        explicit materialization boundary.
        """
        self._pipeline.on_fields_custom(field_matcher, callback, owner_matcher)
        return self

    def on_methods_custom(
        self,
        method_matcher: RustMethodMatcher,
        callback: object,
        *,
        owner_matcher: RustClassMatcher | None = None,
    ) -> RustPipelineBuilder:
        """Method step with custom Python callback.

        Nested Rust bridge collections also use live views; ``list(...)`` is the
        explicit materialization boundary.
        """
        self._pipeline.on_methods_custom(method_matcher, callback, owner_matcher)
        return self

    def build(self) -> RustPipeline:
        """Return the constructed pipeline."""
        return self._pipeline

    def apply(self, model: RustClassModel) -> None:
        """Apply pipeline to a single RustClassModel (mutates in-place)."""
        self._pipeline.apply(model)

    def apply_all(self, models: list[RustClassModel]) -> None:
        """Apply pipeline to many RustClassModel objects (mutates in-place)."""
        self._pipeline.apply_all(models)

    def compile(self) -> RustCompiledPipeline:
        """Compile for hot-path repeated use (pre-compiles regexes)."""
        return self._pipeline.compile()

    def __len__(self) -> int:
        return len(self._pipeline)

    def __repr__(self) -> str:
        return f"RustPipelineBuilder(steps={len(self._pipeline)})"
