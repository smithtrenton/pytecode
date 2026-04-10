"""Tests for Rust-backed declarative transforms and pipeline.

Tests that:
1. All transform factories produce valid RustClassTransform objects
2. Pipeline builder constructs multi-step pipelines
3. Matchers + transforms compose correctly in pipelines
"""

from __future__ import annotations

import pytest

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
        p = RustPipelineBuilder().on_classes(class_named("Foo"), rename_class("Bar")).build()
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
        p = RustPipelineBuilder().on_methods(method_named("foo"), remove_method("foo")).build()
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
        p = RustPipelineBuilder().on_fields(field_named("x"), remove_field("x")).build()
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
        p = RustPipelineBuilder().on_classes(matcher, rename_class("Baz")).build()
        assert len(p) == 1

    def test_negated_matcher_in_pipeline(self) -> None:
        matcher = ~class_named("Foo")
        p = RustPipelineBuilder().on_classes(matcher, remove_access_flags(0x0010)).build()
        assert len(p) == 1

    def test_sequence_transform_in_pipeline(self) -> None:
        t = sequence(
            rename_class("NewName"),
            set_super_class("java/lang/Object"),
            add_interface("java/io/Serializable"),
        )
        p = RustPipelineBuilder().on_classes(class_name_matches(".*"), t).build()
        assert len(p) == 1


# ---------------------------------------------------------------------------
# End-to-end integration tests (using real class bytes)
# ---------------------------------------------------------------------------


@pytest.fixture()
def class_bytes() -> bytes:
    """First .class from the byte-buddy fixture jar."""
    import os
    import zipfile

    jar = os.path.join(
        "crates",
        "pytecode-engine",
        "fixtures",
        "jars",
        "byte-buddy-1.17.5.jar",
    )
    with zipfile.ZipFile(jar) as z:
        names = [n for n in z.namelist() if n.endswith(".class")]
        return z.read(names[0])


@pytest.fixture()
def multi_class_bytes() -> list[bytes]:
    """First 10 .class entries from the byte-buddy fixture jar."""
    import os
    import zipfile

    jar = os.path.join(
        "crates",
        "pytecode-engine",
        "fixtures",
        "jars",
        "byte-buddy-1.17.5.jar",
    )
    with zipfile.ZipFile(jar) as z:
        names = [n for n in z.namelist() if n.endswith(".class")][:10]
        return [z.read(n) for n in names]


class TestPipelineApply:
    """End-to-end pipeline application on real RustClassModel objects."""

    def test_apply_rename(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        original = m.name

        p = RustPipelineBuilder()
        p.on_classes(class_named(original), rename_class("com/example/Renamed"))
        p.build().apply(m)

        assert m.name == "com/example/Renamed"

    def test_apply_no_match(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        original_name = m.name

        p = RustPipelineBuilder()
        p.on_classes(class_named("nonexistent/Class"), rename_class("should/not/apply"))
        p.build().apply(m)

        assert m.name == original_name

    def test_apply_add_access_flags(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        orig_flags = m.access_flags

        p = RustPipelineBuilder()
        p.on_classes(class_named(m.name), add_access_flags(0x0010))
        p.build().apply(m)

        assert m.access_flags == (orig_flags | 0x0010)

    def test_apply_set_super_class(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)

        p = RustPipelineBuilder()
        p.on_classes(class_named(m.name), set_super_class("com/example/NewBase"))
        p.build().apply(m)

        assert m.super_name == "com/example/NewBase"

    def test_apply_add_interface(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        orig_ifaces = list(m.interfaces)

        p = RustPipelineBuilder()
        p.on_classes(class_named(m.name), add_interface("java/io/Serializable"))
        p.build().apply(m)

        assert "java/io/Serializable" in m.interfaces
        assert len(m.interfaces) == len(orig_ifaces) + 1

    def test_apply_remove_interface(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        # First add, then remove
        p1 = RustPipelineBuilder()
        p1.on_classes(class_named(m.name), add_interface("java/io/Serializable"))
        p1.build().apply(m)
        assert "java/io/Serializable" in m.interfaces

        p2 = RustPipelineBuilder()
        p2.on_classes(class_named(m.name), remove_interface("java/io/Serializable"))
        p2.build().apply(m)
        assert "java/io/Serializable" not in m.interfaces

    def test_apply_sequence(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)

        t = sequence(
            rename_class("com/example/SequenceTest"),
            set_super_class("com/example/Base"),
            add_interface("java/lang/Runnable"),
        )
        p = RustPipelineBuilder()
        p.on_classes(class_named(m.name), t)
        p.build().apply(m)

        assert m.name == "com/example/SequenceTest"
        assert m.super_name == "com/example/Base"
        assert "java/lang/Runnable" in m.interfaces

    def test_apply_regex_matcher(self, multi_class_bytes: list[bytes]) -> None:
        from pytecode._rust import RustClassModel

        models = [RustClassModel.from_bytes(b) for b in multi_class_bytes]

        p = RustPipelineBuilder()
        p.on_classes(
            class_name_matches(".*Writer"),
            set_super_class("com/example/WriterBase"),
        )
        pipeline = p.build()

        for m in models:
            pipeline.apply(m)

        for m in models:
            if "Writer" in m.name:
                assert m.super_name == "com/example/WriterBase"

    def test_apply_all_batch(self, multi_class_bytes: list[bytes]) -> None:
        from pytecode._rust import RustClassModel

        models = [RustClassModel.from_bytes(b) for b in multi_class_bytes]
        p = RustPipelineBuilder()
        p.on_classes(class_name_matches(".*"), add_access_flags(0x0010))
        p.build().apply_all(models)

        for m in models:
            assert m.access_flags & 0x0010

    def test_compiled_pipeline(self, multi_class_bytes: list[bytes]) -> None:
        from pytecode._rust import RustClassModel

        models = [RustClassModel.from_bytes(b) for b in multi_class_bytes]

        p = RustPipelineBuilder()
        p.on_classes(
            class_name_matches(".*Attribute.*"),
            add_interface("java/io/Serializable"),
        )
        compiled = p.compile()

        for m in models:
            compiled.apply(m)

        for m in models:
            if "Attribute" in m.name:
                assert "java/io/Serializable" in m.interfaces

    def test_roundtrip_after_transform(self, class_bytes: bytes) -> None:
        """Transform then serialize — should not crash."""
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        p = RustPipelineBuilder()
        p.on_classes(class_named(m.name), add_access_flags(0x0010))
        p.build().apply(m)

        # Should serialize without error
        result = m.to_bytes()
        assert isinstance(result, bytes)
        assert len(result) > 0

    def test_rename_method_all_matching(self, class_bytes: bytes) -> None:
        """rename_method renames every method with the given name (rename-all)."""
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        methods = list(m.methods)
        if not methods:
            pytest.skip("fixture class has no methods")

        original_name = methods[0].name
        count_before = sum(1 for mth in methods if mth.name == original_name)

        p = RustPipelineBuilder()
        p.on_classes(class_named(m.name), rename_method(original_name, "__renamed__"))
        p.build().apply(m)

        renamed = [mth for mth in list(m.methods) if mth.name == "__renamed__"]
        assert len(renamed) == count_before
        # No method should still have the original name
        assert not any(mth.name == original_name for mth in list(m.methods))


# ---------------------------------------------------------------------------
# Custom callback tests (T4)
# ---------------------------------------------------------------------------


class TestCustomCallbacks:
    """Pipeline with custom Python callbacks in the hot path."""

    def test_class_callback_modifies_name(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)

        def rename(model: object) -> None:
            model.name = "com/callback/Renamed"  # type: ignore[attr-defined]

        p = RustPipelineBuilder()
        p.on_classes_custom(class_named(m.name), rename)
        p.build().apply(m)

        assert m.name == "com/callback/Renamed"

    def test_class_callback_no_match(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        original = m.name
        called: list[bool] = []

        def should_not_run(model: object) -> None:
            called.append(True)

        p = RustPipelineBuilder()
        p.on_classes_custom(class_named("nonexistent/Class"), should_not_run)
        p.build().apply(m)

        assert m.name == original
        assert not called

    def test_mixed_builtin_and_callback(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        original_name = m.name

        def add_iface(model: RustClassModel) -> None:
            ifaces = list(model.interfaces)
            ifaces.append("com/callback/Added")
            model.interfaces = ifaces

        p = RustPipelineBuilder()
        # Built-in step first
        p.on_classes(class_named(original_name), add_access_flags(0x0010))
        # Custom callback second
        p.on_classes_custom(class_named(original_name), add_iface)
        p.build().apply(m)

        assert m.access_flags & 0x0010
        assert "com/callback/Added" in m.interfaces

    def test_callback_on_methods_guard(self, multi_class_bytes: list[bytes]) -> None:
        from pytecode._rust import RustClassModel

        models = [RustClassModel.from_bytes(b) for b in multi_class_bytes]
        transformed: list[str] = []

        def track(model: object) -> None:
            transformed.append(model.name)  # type: ignore[attr-defined]

        p = RustPipelineBuilder()
        p.on_methods_custom(method_named("<init>"), track)
        pipeline = p.build()
        for m in models:
            pipeline.apply(m)

        # At least some classes should have <init>
        assert len(transformed) > 0

    def test_callback_exception_propagates(self, class_bytes: bytes) -> None:
        """Python exception in callback must propagate to the caller."""
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        original = m.name

        def bad_callback(model: object) -> None:
            raise ValueError("intentional error")

        p = RustPipelineBuilder()
        p.on_classes_custom(class_named(original), bad_callback)
        with pytest.raises(ValueError, match="intentional error"):
            p.build().apply(m)

    def test_callback_exception_after_mutation_propagates(self, class_bytes: bytes) -> None:
        """Callback that mutates then raises: error propagates, model reflects mutation."""
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        original_name = m.name

        def mutate_then_raise(model: object) -> None:
            model.name = "com/partial/Renamed"  # type: ignore[attr-defined]
            raise RuntimeError("mutation then error")

        p = RustPipelineBuilder()
        p.on_classes_custom(class_named(original_name), mutate_then_raise)
        with pytest.raises(RuntimeError, match="mutation then error"):
            p.build().apply(m)
        # Mutation happened before the raise; model reflects it
        assert m.name == "com/partial/Renamed"

    def test_on_methods_custom_class_scoped(self, class_bytes: bytes) -> None:
        """on_methods_custom fires at most once per class (class-scoped, not per-method)."""
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        invocations: list[int] = []

        def track(model: object) -> None:
            invocations.append(1)

        p = RustPipelineBuilder()
        p.on_methods_custom(has_code(), track)
        p.build().apply(m)

        # Fires at most once per class regardless of how many methods have code
        assert len(invocations) <= 1

    def test_invalid_regex_raises_at_construction(self) -> None:
        """Invalid regex pattern should raise ValueError at matcher construction time."""
        from pytecode._rust import RustClassMatcher, RustFieldMatcher, RustMethodMatcher

        with pytest.raises(ValueError, match="invalid regex"):
            RustClassMatcher.name_matches("[invalid")
        with pytest.raises(ValueError, match="invalid regex"):
            RustFieldMatcher.name_matches("[invalid")
        with pytest.raises(ValueError, match="invalid regex"):
            RustFieldMatcher.descriptor_matches("[invalid")
        with pytest.raises(ValueError, match="invalid regex"):
            RustMethodMatcher.name_matches("[invalid")
        with pytest.raises(ValueError, match="invalid regex"):
            RustMethodMatcher.descriptor_matches("[invalid")

    def test_compiled_with_callback(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)

        def rename(model: object) -> None:
            model.name = "com/compiled/Callback"  # type: ignore[attr-defined]

        p = RustPipelineBuilder()
        p.on_classes_custom(class_named(m.name), rename)
        compiled = p.compile()
        compiled.apply(m)

        assert m.name == "com/compiled/Callback"

    def test_interfaces_view_invalidates_after_replace(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        view = m.interfaces
        original = list(view)

        m.interfaces = [*original, "com/callback/Added"]

        with pytest.raises(RuntimeError, match="stale"):
            len(view)

    def test_method_ref_invalidates_after_method_list_replace(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        methods = m.methods
        first = methods[0]

        m.methods = list(methods)

        with pytest.raises(RuntimeError, match="stale"):
            _ = first.name

    def test_constant_pool_view_mutates_owner(self, class_bytes: bytes) -> None:
        from pytecode._rust import RustClassModel

        m = RustClassModel.from_bytes(class_bytes)
        cp = m.constant_pool
        before = cp.count()

        added = cp.add_utf8("bridge/ViewTest")

        assert added >= before
        assert cp.count() == before + 1
        assert len(m.to_bytes()) > 0
