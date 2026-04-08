"""Tests for Rust-backed declarative transforms and pipeline.

Tests that:
1. All transform factories produce valid RustClassTransform objects
2. Pipeline builder constructs multi-step pipelines
3. Matchers + transforms compose correctly in pipelines
"""

from __future__ import annotations

from pytecode._rust import RustClassTransform, RustPipeline
from pytecode.transforms.rust_matchers import (
    class_name_matches,
    class_named,
    field_named,
    has_code,
    method_named,
)
from pytecode.transforms.rust_pipeline import RustPipelineBuilder
from pytecode.transforms.rust_transforms import (
    add_access_flags,
    add_interface,
    remove_access_flags,
    remove_field,
    remove_interface,
    remove_method,
    rename_class,
    rename_field,
    rename_method,
    sequence,
    set_access_flags,
    set_field_access_flags,
    set_method_access_flags,
    set_super_class,
)

# ---------------------------------------------------------------------------
# Transform factory tests
# ---------------------------------------------------------------------------


class TestTransformFactories:
    """Each factory returns a valid RustClassTransform."""

    def test_rename_class(self) -> None:
        t = rename_class("com/example/Bar")
        assert isinstance(t, RustClassTransform)
        assert "rename_class" in str(t)

    def test_set_access_flags(self) -> None:
        t = set_access_flags(0x0021)
        assert isinstance(t, RustClassTransform)
        assert "set_access_flags" in str(t)

    def test_add_access_flags(self) -> None:
        t = add_access_flags(0x0010)
        assert isinstance(t, RustClassTransform)
        assert "add_access_flags" in str(t)

    def test_remove_access_flags(self) -> None:
        t = remove_access_flags(0x0010)
        assert isinstance(t, RustClassTransform)
        assert "remove_access_flags" in str(t)

    def test_set_super_class(self) -> None:
        t = set_super_class("com/example/Base")
        assert isinstance(t, RustClassTransform)
        assert "set_super_class" in str(t)

    def test_add_interface(self) -> None:
        t = add_interface("java/io/Serializable")
        assert isinstance(t, RustClassTransform)
        assert "add_interface" in str(t)

    def test_remove_interface(self) -> None:
        t = remove_interface("java/io/Serializable")
        assert isinstance(t, RustClassTransform)
        assert "remove_interface" in str(t)

    def test_remove_method_name_only(self) -> None:
        t = remove_method("foo")
        assert isinstance(t, RustClassTransform)
        assert "remove_method" in str(t)

    def test_remove_method_with_descriptor(self) -> None:
        t = remove_method("foo", "()V")
        assert isinstance(t, RustClassTransform)
        assert "remove_method" in str(t)

    def test_remove_field_name_only(self) -> None:
        t = remove_field("bar")
        assert isinstance(t, RustClassTransform)

    def test_remove_field_with_descriptor(self) -> None:
        t = remove_field("bar", "I")
        assert isinstance(t, RustClassTransform)

    def test_rename_method(self) -> None:
        t = rename_method("old", "new")
        assert isinstance(t, RustClassTransform)
        assert "rename_method" in str(t)

    def test_rename_field(self) -> None:
        t = rename_field("old", "new")
        assert isinstance(t, RustClassTransform)
        assert "rename_field" in str(t)

    def test_set_method_access_flags(self) -> None:
        t = set_method_access_flags("foo", 0x0001)
        assert isinstance(t, RustClassTransform)
        assert "set_method_access_flags" in str(t)

    def test_set_field_access_flags(self) -> None:
        t = set_field_access_flags("bar", 0x0002)
        assert isinstance(t, RustClassTransform)
        assert "set_field_access_flags" in str(t)

    def test_sequence(self) -> None:
        t = sequence(
            rename_class("com/example/New"),
            add_access_flags(0x0010),
        )
        assert isinstance(t, RustClassTransform)
        assert "sequence" in str(t)

    def test_repr(self) -> None:
        t = rename_class("Foo")
        assert "RustClassTransform" in repr(t)


# ---------------------------------------------------------------------------
# Pipeline builder tests
# ---------------------------------------------------------------------------


class TestPipelineBuilder:
    """Pipeline builder assembles steps correctly."""

    def test_empty_pipeline(self) -> None:
        p = RustPipelineBuilder().build()
        assert isinstance(p, RustPipeline)
        assert len(p) == 0

    def test_single_class_step(self) -> None:
        p = (
            RustPipelineBuilder()
            .on_classes(class_named("Foo"), rename_class("Bar"))
            .build()
        )
        assert len(p) == 1

    def test_multiple_steps(self) -> None:
        p = (
            RustPipelineBuilder()
            .on_classes(class_named("Foo"), rename_class("Bar"))
            .on_classes(
                class_name_matches(".*Test"),
                add_access_flags(0x0010),
            )
            .build()
        )
        assert len(p) == 2

    def test_method_step(self) -> None:
        p = (
            RustPipelineBuilder()
            .on_methods(method_named("foo"), remove_method("foo"))
            .build()
        )
        assert len(p) == 1

    def test_method_step_with_owner(self) -> None:
        p = (
            RustPipelineBuilder()
            .on_methods(
                method_named("foo"),
                remove_method("foo"),
                owner_matcher=class_named("Bar"),
            )
            .build()
        )
        assert len(p) == 1

    def test_field_step(self) -> None:
        p = (
            RustPipelineBuilder()
            .on_fields(field_named("x"), remove_field("x"))
            .build()
        )
        assert len(p) == 1

    def test_field_step_with_owner(self) -> None:
        p = (
            RustPipelineBuilder()
            .on_fields(
                field_named("x"),
                remove_field("x"),
                owner_matcher=class_named("Baz"),
            )
            .build()
        )
        assert len(p) == 1

    def test_fluent_chaining(self) -> None:
        builder = RustPipelineBuilder()
        result = builder.on_classes(class_named("A"), rename_class("B"))
        assert result is builder

    def test_repr(self) -> None:
        b = RustPipelineBuilder()
        b.on_classes(class_named("A"), rename_class("B"))
        assert "steps=1" in repr(b)

    def test_mixed_steps(self) -> None:
        p = (
            RustPipelineBuilder()
            .on_classes(class_named("Foo"), rename_class("Bar"))
            .on_methods(has_code(), remove_method("init"))
            .on_fields(field_named("x"), set_field_access_flags("x", 0x0002))
            .build()
        )
        assert len(p) == 3


# ---------------------------------------------------------------------------
# Combined matcher + transform composition tests
# ---------------------------------------------------------------------------


class TestComposition:
    """Matchers and transforms compose correctly."""

    def test_combinator_matcher_in_pipeline(self) -> None:
        matcher = class_named("Foo") | class_named("Bar")
        p = (
            RustPipelineBuilder()
            .on_classes(matcher, rename_class("Baz"))
            .build()
        )
        assert len(p) == 1

    def test_negated_matcher_in_pipeline(self) -> None:
        matcher = ~class_named("Foo")
        p = (
            RustPipelineBuilder()
            .on_classes(matcher, remove_access_flags(0x0010))
            .build()
        )
        assert len(p) == 1

    def test_sequence_transform_in_pipeline(self) -> None:
        t = sequence(
            rename_class("NewName"),
            set_super_class("java/lang/Object"),
            add_interface("java/io/Serializable"),
        )
        p = (
            RustPipelineBuilder()
            .on_classes(class_name_matches(".*"), t)
            .build()
        )
        assert len(p) == 1
