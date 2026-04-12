"""Tests for declarative transforms and pipeline.

Tests that:
1. All transform factories produce valid ClassTransform objects
2. Pipeline builder constructs multi-step pipelines
3. Matchers + transforms compose correctly in pipelines
"""

from __future__ import annotations

from pathlib import Path

import pytest

import pytecode.model as model_api
from pytecode._rust import ClassTransform, Pipeline
from pytecode.transforms import (
    CodeTransform,
    InsnMatcher,
    PipelineBuilder,
    add_access_flags,
    add_interface,
    class_named,
    method_named,
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
from pytecode.transforms.matchers import class_name_matches, field_named, has_code
from tests.helpers import TEST_RESOURCES, compile_java_sources

SINGLE_CLASS_RESOURCE = "CfgFixture.java"
MULTI_CLASS_RESOURCES = (
    "HelloWorld.java",
    "AnnotatedClass.java",
    "TypeAnnotationShowcase.java",
    "SimpleInterface.java",
    "MultiInterface.java",
    "Outer.java",
)

CodeItem = (
    model_api.Label
    | model_api.RawInsn
    | model_api.ByteInsn
    | model_api.ShortInsn
    | model_api.NewArrayInsn
    | model_api.FieldInsn
    | model_api.MethodInsn
    | model_api.InterfaceMethodInsn
    | model_api.TypeInsn
    | model_api.VarInsn
    | model_api.IIncInsn
    | model_api.LdcInsn
    | model_api.InvokeDynamicInsn
    | model_api.MultiANewArrayInsn
    | model_api.BranchInsn
    | model_api.LookupSwitchInsn
    | model_api.TableSwitchInsn
)
CODE_ITEM_TYPES = (
    model_api.Label,
    model_api.RawInsn,
    model_api.ByteInsn,
    model_api.ShortInsn,
    model_api.NewArrayInsn,
    model_api.FieldInsn,
    model_api.MethodInsn,
    model_api.InterfaceMethodInsn,
    model_api.TypeInsn,
    model_api.VarInsn,
    model_api.IIncInsn,
    model_api.LdcInsn,
    model_api.InvokeDynamicInsn,
    model_api.MultiANewArrayInsn,
    model_api.BranchInsn,
    model_api.LookupSwitchInsn,
    model_api.TableSwitchInsn,
)

# ---------------------------------------------------------------------------
# Transform factory tests
# ---------------------------------------------------------------------------


def test_transform_api_exports_builder_and_helpers() -> None:
    assert callable(PipelineBuilder)
    assert callable(class_named)
    assert callable(method_named)
    assert callable(add_access_flags)
    assert callable(InsnMatcher)
    assert callable(CodeTransform)
    assert callable(rename_class)


class TestTransformFactories:
    """Each factory returns a valid ClassTransform."""

    def test_rename_class(self) -> None:
        t = rename_class("com/example/Bar")
        assert isinstance(t, ClassTransform)
        assert "rename_class" in str(t)

    def test_set_access_flags(self) -> None:
        t = set_access_flags(0x0021)
        assert isinstance(t, ClassTransform)
        assert "set_access_flags" in str(t)

    def test_add_access_flags(self) -> None:
        t = add_access_flags(0x0010)
        assert isinstance(t, ClassTransform)
        assert "add_access_flags" in str(t)

    def test_remove_access_flags(self) -> None:
        t = remove_access_flags(0x0010)
        assert isinstance(t, ClassTransform)
        assert "remove_access_flags" in str(t)

    def test_set_super_class(self) -> None:
        t = set_super_class("com/example/Base")
        assert isinstance(t, ClassTransform)
        assert "set_super_class" in str(t)

    def test_add_interface(self) -> None:
        t = add_interface("java/io/Serializable")
        assert isinstance(t, ClassTransform)
        assert "add_interface" in str(t)

    def test_remove_interface(self) -> None:
        t = remove_interface("java/io/Serializable")
        assert isinstance(t, ClassTransform)
        assert "remove_interface" in str(t)

    def test_remove_method_name_only(self) -> None:
        t = remove_method("foo")
        assert isinstance(t, ClassTransform)
        assert "remove_method" in str(t)

    def test_remove_method_with_descriptor(self) -> None:
        t = remove_method("foo", "()V")
        assert isinstance(t, ClassTransform)
        assert "remove_method" in str(t)

    def test_remove_field_name_only(self) -> None:
        t = remove_field("bar")
        assert isinstance(t, ClassTransform)

    def test_remove_field_with_descriptor(self) -> None:
        t = remove_field("bar", "I")
        assert isinstance(t, ClassTransform)

    def test_rename_method(self) -> None:
        t = rename_method("old", "new")
        assert isinstance(t, ClassTransform)
        assert "rename_method" in str(t)

    def test_rename_field(self) -> None:
        t = rename_field("old", "new")
        assert isinstance(t, ClassTransform)
        assert "rename_field" in str(t)

    def test_set_method_access_flags(self) -> None:
        t = set_method_access_flags("foo", 0x0001)
        assert isinstance(t, ClassTransform)
        assert "set_method_access_flags" in str(t)

    def test_set_field_access_flags(self) -> None:
        t = set_field_access_flags("bar", 0x0002)
        assert isinstance(t, ClassTransform)
        assert "set_field_access_flags" in str(t)

    def test_sequence(self) -> None:
        t = sequence(
            rename_class("com/example/New"),
            add_access_flags(0x0010),
        )
        assert isinstance(t, ClassTransform)
        assert "sequence" in str(t)

    def test_repr(self) -> None:
        t = rename_class("Foo")
        assert "ClassTransform" in repr(t)

    def test_code_transform_factories(self) -> None:
        matcher = InsnMatcher.opcode(0x00)
        replacement = [model_api.RawInsn(0x57)]
        t = CodeTransform.sequence(
            [
                CodeTransform.replace_insn(matcher, replacement),
                CodeTransform.remove_sequence([InsnMatcher.opcode(0x2A), InsnMatcher.opcode(0xB7)]),
                CodeTransform.insert_before(matcher, replacement),
                CodeTransform.insert_after(matcher, replacement),
            ]
        )
        assert isinstance(t, CodeTransform)
        assert "sequence" in str(t)

    def test_class_code_transform_factory(self) -> None:
        t = ClassTransform.code_transform(
            CodeTransform.remove_insn(InsnMatcher.is_label()),
            method_name="main",
            method_descriptor="([Ljava/lang/String;)V",
        )
        assert isinstance(t, ClassTransform)
        assert "code_transform" in str(t)


# ---------------------------------------------------------------------------
# Pipeline builder tests
# ---------------------------------------------------------------------------


class TestPipelineBuilder:
    """Pipeline builder assembles steps correctly."""

    def test_empty_pipeline(self) -> None:
        p = PipelineBuilder().build()
        assert isinstance(p, Pipeline)
        assert len(p) == 0

    def test_single_class_step(self) -> None:
        p = PipelineBuilder().on_classes(class_named("Foo"), rename_class("Bar")).build()
        assert len(p) == 1

    def test_multiple_steps(self) -> None:
        p = (
            PipelineBuilder()
            .on_classes(class_named("Foo"), rename_class("Bar"))
            .on_classes(
                class_name_matches(".*Test"),
                add_access_flags(0x0010),
            )
            .build()
        )
        assert len(p) == 2

    def test_method_step(self) -> None:
        p = PipelineBuilder().on_methods(method_named("foo"), remove_method("foo")).build()
        assert len(p) == 1

    def test_method_step_with_owner(self) -> None:
        p = (
            PipelineBuilder()
            .on_methods(
                method_named("foo"),
                remove_method("foo"),
                owner_matcher=class_named("Bar"),
            )
            .build()
        )
        assert len(p) == 1

    def test_field_step(self) -> None:
        p = PipelineBuilder().on_fields(field_named("x"), remove_field("x")).build()
        assert len(p) == 1

    def test_field_step_with_owner(self) -> None:
        p = (
            PipelineBuilder()
            .on_fields(
                field_named("x"),
                remove_field("x"),
                owner_matcher=class_named("Baz"),
            )
            .build()
        )
        assert len(p) == 1

    def test_fluent_chaining(self) -> None:
        builder = PipelineBuilder()
        result = builder.on_classes(class_named("A"), rename_class("B"))
        assert result is builder

    def test_repr(self) -> None:
        b = PipelineBuilder()
        b.on_classes(class_named("A"), rename_class("B"))
        assert "steps=1" in repr(b)

    def test_mixed_steps(self) -> None:
        p = (
            PipelineBuilder()
            .on_classes(class_named("Foo"), rename_class("Bar"))
            .on_methods(has_code(), remove_method("init"))
            .on_fields(field_named("x"), set_field_access_flags("x", 0x0002))
            .build()
        )
        assert len(p) == 3

    def test_code_step(self) -> None:
        p = (
            PipelineBuilder()
            .on_code(
                method_named("main"),
                CodeTransform.remove_insn(InsnMatcher.is_label()),
                owner_matcher=class_named("Example"),
            )
            .build()
        )
        assert len(p) == 1


# ---------------------------------------------------------------------------
# Combined matcher + transform composition tests
# ---------------------------------------------------------------------------


class TestComposition:
    """Matchers and transforms compose correctly."""

    def test_combinator_matcher_in_pipeline(self) -> None:
        matcher = class_named("Foo") | class_named("Bar")
        p = PipelineBuilder().on_classes(matcher, rename_class("Baz")).build()
        assert len(p) == 1

    def test_negated_matcher_in_pipeline(self) -> None:
        matcher = ~class_named("Foo")
        p = PipelineBuilder().on_classes(matcher, remove_access_flags(0x0010)).build()
        assert len(p) == 1

    def test_sequence_transform_in_pipeline(self) -> None:
        t = sequence(
            rename_class("NewName"),
            set_super_class("java/lang/Object"),
            add_interface("java/io/Serializable"),
        )
        p = PipelineBuilder().on_classes(class_name_matches(".*"), t).build()
        assert len(p) == 1


# ---------------------------------------------------------------------------
# End-to-end integration tests (using real class bytes)
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def compiled_test_classes(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """Compile Python-owned Java resources once for this module."""

    temp_dir = tmp_path_factory.mktemp("rust-transform-fixtures")
    source_files = [TEST_RESOURCES / SINGLE_CLASS_RESOURCE]
    source_files.extend(TEST_RESOURCES / resource for resource in MULTI_CLASS_RESOURCES)
    return compile_java_sources(temp_dir, source_files)


@pytest.fixture()
def class_bytes(compiled_test_classes: Path) -> bytes:
    """Compiled class bytes for one Python-owned fixture class."""

    return (compiled_test_classes / "CfgFixture.class").read_bytes()


@pytest.fixture()
def multi_class_bytes(compiled_test_classes: Path) -> list[bytes]:
    """Compiled class bytes for a Python-owned multi-class fixture corpus."""

    return [path.read_bytes() for path in sorted(compiled_test_classes.rglob("*.class"))]


def _matcher_for_item(item: object) -> InsnMatcher | None:
    item_type = str(getattr(item, "kind", ""))
    if item_type in {"raw", "byte", "short"}:
        return InsnMatcher.opcode(int(getattr(item, "opcode")))
    if item_type == "newarray":
        return InsnMatcher.opcode(0xBC)
    if item_type == "field":
        return InsnMatcher.field(
            str(getattr(item, "owner")),
            str(getattr(item, "name")),
            str(getattr(item, "descriptor")),
        )
    if item_type == "method":
        return InsnMatcher.method(
            str(getattr(item, "owner")),
            str(getattr(item, "name")),
            str(getattr(item, "descriptor")),
        )
    if item_type == "interface_method":
        return (
            InsnMatcher.method_owner(str(getattr(item, "owner")))
            & InsnMatcher.method_named(str(getattr(item, "name")))
            & InsnMatcher.method_descriptor(str(getattr(item, "descriptor")))
        )
    if item_type == "type":
        return InsnMatcher.opcode(int(getattr(item, "opcode"))) & InsnMatcher.type_descriptor(
            str(getattr(item, "descriptor"))
        )
    if item_type == "var":
        return InsnMatcher.opcode(int(getattr(item, "opcode"))) & InsnMatcher.var_slot(int(getattr(item, "slot")))
    if item_type == "iinc":
        return InsnMatcher.opcode(0x84) & InsnMatcher.var_slot(int(getattr(item, "slot")))
    if item_type == "ldc":
        value_type = getattr(item, "value_type", None)
        if value_type == "string":
            return InsnMatcher.ldc_string(str(getattr(item, "value")))
        return InsnMatcher.is_ldc()
    if item_type == "label":
        return InsnMatcher.is_label()
    if item_type == "branch":
        return InsnMatcher.is_branch() & InsnMatcher.opcode(int(getattr(item, "opcode")))
    if item_type == "multianewarray":
        return InsnMatcher.opcode(0xC5) & InsnMatcher.type_descriptor(str(getattr(item, "descriptor")))
    if item_type == "invokedynamic":
        return InsnMatcher.opcode(0xBA)
    return None


def _clone_item(item: object) -> CodeItem:
    kind = str(getattr(item, "kind", ""))
    if kind == "label":
        return _as_code_item(item)
    if kind == "raw":
        return model_api.RawInsn(int(getattr(item, "opcode")))
    if kind == "byte":
        return model_api.ByteInsn(int(getattr(item, "opcode")), int(getattr(item, "value")))
    if kind == "short":
        return model_api.ShortInsn(int(getattr(item, "opcode")), int(getattr(item, "value")))
    if kind == "newarray":
        return model_api.NewArrayInsn(int(getattr(item, "atype")))
    if kind == "field":
        return model_api.FieldInsn(
            int(getattr(item, "opcode")),
            str(getattr(item, "owner")),
            str(getattr(item, "name")),
            str(getattr(item, "descriptor")),
        )
    if kind == "method":
        return model_api.MethodInsn(
            int(getattr(item, "opcode")),
            str(getattr(item, "owner")),
            str(getattr(item, "name")),
            str(getattr(item, "descriptor")),
            bool(getattr(item, "is_interface")),
        )
    if kind == "interface_method":
        return model_api.InterfaceMethodInsn(
            str(getattr(item, "owner")),
            str(getattr(item, "name")),
            str(getattr(item, "descriptor")),
        )
    if kind == "type":
        return model_api.TypeInsn(int(getattr(item, "opcode")), str(getattr(item, "descriptor")))
    if kind == "var":
        return model_api.VarInsn(int(getattr(item, "opcode")), int(getattr(item, "slot")))
    if kind == "iinc":
        return model_api.IIncInsn(int(getattr(item, "slot")), int(getattr(item, "value")))
    if kind == "ldc":
        value_type = str(getattr(item, "value_type"))
        value = getattr(item, "value")
        if value_type == "method_handle":
            return model_api.LdcInsn.method_handle(
                int(getattr(value, "reference_kind")),
                str(getattr(value, "owner")),
                str(getattr(value, "name")),
                str(getattr(value, "descriptor")),
                bool(getattr(value, "is_interface")),
            )
        if value_type == "dynamic":
            return model_api.LdcInsn.dynamic(
                int(getattr(value, "bootstrap_method_attr_index")),
                str(getattr(value, "name")),
                str(getattr(value, "descriptor")),
            )
        if value_type == "int":
            return model_api.LdcInsn.int(int(value))
        if value_type == "float_bits":
            return model_api.LdcInsn.float_bits(int(value))
        if value_type == "long":
            return model_api.LdcInsn.long(int(value))
        if value_type == "double_bits":
            return model_api.LdcInsn.double_bits(int(value))
        if value_type == "string":
            return model_api.LdcInsn.string(str(value))
        if value_type == "class":
            return model_api.LdcInsn.class_value(str(value))
        if value_type == "method_type":
            return model_api.LdcInsn.method_type(str(value))
        raise TypeError(f"unsupported ldc value_type {value_type!r}")
    if kind == "invokedynamic":
        return model_api.InvokeDynamicInsn(
            int(getattr(item, "bootstrap_method_attr_index")),
            str(getattr(item, "name")),
            str(getattr(item, "descriptor")),
        )
    if kind == "multianewarray":
        return model_api.MultiANewArrayInsn(
            str(getattr(item, "descriptor")),
            int(getattr(item, "dimensions")),
        )
    if kind == "branch":
        return model_api.BranchInsn(int(getattr(item, "opcode")), getattr(item, "target"))
    if kind == "lookupswitch":
        return model_api.LookupSwitchInsn(getattr(item, "default_target"), list(getattr(item, "pairs")))
    if kind == "tableswitch":
        return model_api.TableSwitchInsn(
            getattr(item, "default_target"),
            int(getattr(item, "low")),
            int(getattr(item, "high")),
            list(getattr(item, "targets")),
        )
    raise TypeError(f"unsupported instruction item {type(item)!r}")


def _as_code_item(item: object) -> CodeItem:
    if isinstance(item, CODE_ITEM_TYPES):
        return item
    raise TypeError(f"unsupported code item {type(item)!r}")


def _find_unique_sequence(code: model_api.CodeModel) -> tuple[int, list[InsnMatcher], list[CodeItem]]:
    raw_items = [_as_code_item(item) for item in code.instructions.to_list()]
    for window_size in (2, 3):
        for start in range(len(raw_items) - window_size):
            window = raw_items[start : start + window_size]
            matchers = [_matcher_for_item(item) for item in window]
            if any(matcher is None for matcher in matchers):
                continue
            typed_matchers = [matcher for matcher in matchers if matcher is not None]
            if code.find_sequences(typed_matchers) == [start]:
                replacement = [_clone_item(raw_items[start + window_size])]
                return start, typed_matchers, replacement
    raise AssertionError("fixture did not contain a uniquely matchable instruction sequence")


class TestPipelineApply:
    """End-to-end pipeline application on real ClassModel objects."""

    def test_apply_rename(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        original = m.name

        p = PipelineBuilder()
        p.on_classes(class_named(original), rename_class("com/example/Renamed"))
        p.build().apply(m)

        assert m.name == "com/example/Renamed"

    def test_apply_no_match(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        original_name = m.name

        p = PipelineBuilder()
        p.on_classes(class_named("nonexistent/Class"), rename_class("should/not/apply"))
        p.build().apply(m)

        assert m.name == original_name

    def test_apply_add_access_flags(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        orig_flags = m.access_flags

        p = PipelineBuilder()
        p.on_classes(class_named(m.name), add_access_flags(0x0010))
        p.build().apply(m)

        assert m.access_flags == (orig_flags | 0x0010)

    def test_apply_sequence_replace_via_on_code(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        model = ClassModel.from_bytes(class_bytes)
        method = next(
            method for method in model.methods if method.code is not None and len(method.code.instructions) >= 4
        )
        code = method.code
        assert code is not None
        start, pattern, replacement = _find_unique_sequence(code)
        original_len = len(code.instructions)

        (
            PipelineBuilder()
            .on_code(
                method_named(method.name),
                CodeTransform.replace_sequence(pattern, replacement),
                owner_matcher=class_named(model.name),
            )
            .build()
            .apply(model)
        )

        updated_code = next(m for m in model.methods if m.name == method.name).code
        assert updated_code is not None
        assert len(updated_code.instructions) == original_len - len(pattern) + len(replacement)
        assert start not in updated_code.find_sequences(pattern)

    def test_apply_class_code_transform_export(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        model = ClassModel.from_bytes(class_bytes)
        method = next(
            method for method in model.methods if method.code is not None and len(method.code.instructions) >= 4
        )
        code = method.code
        assert code is not None
        _, pattern, _ = _find_unique_sequence(code)

        (
            PipelineBuilder()
            .on_classes(
                class_named(model.name),
                ClassTransform.code_transform(
                    CodeTransform.remove_sequence(pattern),
                    method_name=method.name,
                    method_descriptor=method.descriptor,
                ),
            )
            .build()
            .apply(model)
        )

        updated_code = next(m for m in model.methods if m.name == method.name).code
        assert updated_code is not None
        assert not updated_code.find_sequences(pattern)

    def test_apply_set_super_class(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)

        p = PipelineBuilder()
        p.on_classes(class_named(m.name), set_super_class("com/example/NewBase"))
        p.build().apply(m)

        assert m.super_name == "com/example/NewBase"

    def test_apply_add_interface(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        orig_ifaces = list(m.interfaces)

        p = PipelineBuilder()
        p.on_classes(class_named(m.name), add_interface("java/io/Serializable"))
        p.build().apply(m)

        assert "java/io/Serializable" in m.interfaces
        assert len(m.interfaces) == len(orig_ifaces) + 1

    def test_apply_remove_interface(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        # First add, then remove
        p1 = PipelineBuilder()
        p1.on_classes(class_named(m.name), add_interface("java/io/Serializable"))
        p1.build().apply(m)
        assert "java/io/Serializable" in m.interfaces

        p2 = PipelineBuilder()
        p2.on_classes(class_named(m.name), remove_interface("java/io/Serializable"))
        p2.build().apply(m)
        assert "java/io/Serializable" not in m.interfaces

    def test_apply_sequence(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)

        t = sequence(
            rename_class("com/example/SequenceTest"),
            set_super_class("com/example/Base"),
            add_interface("java/lang/Runnable"),
        )
        p = PipelineBuilder()
        p.on_classes(class_named(m.name), t)
        p.build().apply(m)

        assert m.name == "com/example/SequenceTest"
        assert m.super_name == "com/example/Base"
        assert "java/lang/Runnable" in m.interfaces

    def test_apply_regex_matcher(self, multi_class_bytes: list[bytes]) -> None:
        from pytecode._rust import ClassModel

        models = [ClassModel.from_bytes(b) for b in multi_class_bytes]

        p = PipelineBuilder()
        p.on_classes(
            class_name_matches(".*Interface.*"),
            set_super_class("com/example/InterfaceBase"),
        )
        pipeline = p.build()

        for m in models:
            pipeline.apply(m)

        for m in models:
            if "Interface" in m.name:
                assert m.super_name == "com/example/InterfaceBase"

    def test_apply_all_batch(self, multi_class_bytes: list[bytes]) -> None:
        from pytecode._rust import ClassModel

        models = [ClassModel.from_bytes(b) for b in multi_class_bytes]
        p = PipelineBuilder()
        p.on_classes(class_name_matches(".*"), add_access_flags(0x0010))
        p.build().apply_all(models)

        for m in models:
            assert m.access_flags & 0x0010

    def test_compiled_pipeline(self, multi_class_bytes: list[bytes]) -> None:
        from pytecode._rust import ClassModel

        models = [ClassModel.from_bytes(b) for b in multi_class_bytes]

        p = PipelineBuilder()
        p.on_classes(
            class_name_matches(".*Annotation.*"),
            add_interface("java/io/Serializable"),
        )
        compiled = p.compile()

        for m in models:
            compiled.apply(m)

        for m in models:
            if "Annotation" in m.name:
                assert "java/io/Serializable" in m.interfaces

    def test_roundtrip_after_transform(self, class_bytes: bytes) -> None:
        """Transform then serialize — should not crash."""
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        p = PipelineBuilder()
        p.on_classes(class_named(m.name), add_access_flags(0x0010))
        p.build().apply(m)

        # Should serialize without error
        result = m.to_bytes()
        assert isinstance(result, bytes)
        assert len(result) > 0

    def test_rename_method_all_matching(self, class_bytes: bytes) -> None:
        """rename_method renames every method with the given name (rename-all)."""
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        methods = list(m.methods)
        if not methods:
            pytest.skip("fixture class has no methods")

        original_name = methods[0].name
        count_before = sum(1 for mth in methods if mth.name == original_name)

        p = PipelineBuilder()
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
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)

        def rename(model: object) -> None:
            model.name = "com/callback/Renamed"  # type: ignore[attr-defined]

        p = PipelineBuilder()
        p.on_classes_custom(class_named(m.name), rename)
        p.build().apply(m)

        assert m.name == "com/callback/Renamed"

    def test_class_callback_no_match(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        original = m.name
        called: list[bool] = []

        def should_not_run(model: object) -> None:
            called.append(True)

        p = PipelineBuilder()
        p.on_classes_custom(class_named("nonexistent/Class"), should_not_run)
        p.build().apply(m)

        assert m.name == original
        assert not called

    def test_mixed_builtin_and_callback(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        original_name = m.name

        def add_iface(model: ClassModel) -> None:
            ifaces = list(model.interfaces)
            ifaces.append("com/callback/Added")
            model.interfaces = ifaces

        p = PipelineBuilder()
        # Built-in step first
        p.on_classes(class_named(original_name), add_access_flags(0x0010))
        # Custom callback second
        p.on_classes_custom(class_named(original_name), add_iface)
        p.build().apply(m)

        assert m.access_flags & 0x0010
        assert "com/callback/Added" in m.interfaces

    def test_callback_on_methods_guard(self, multi_class_bytes: list[bytes]) -> None:
        from pytecode._rust import ClassModel

        models = [ClassModel.from_bytes(b) for b in multi_class_bytes]
        transformed: list[str] = []

        def track(model: object) -> None:
            transformed.append(model.name)  # type: ignore[attr-defined]

        p = PipelineBuilder()
        p.on_methods_custom(method_named("<init>"), track)
        pipeline = p.build()
        for m in models:
            pipeline.apply(m)

        # At least some classes should have <init>
        assert len(transformed) > 0

    def test_callback_exception_propagates(self, class_bytes: bytes) -> None:
        """Python exception in callback must propagate to the caller."""
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        original = m.name

        def bad_callback(model: object) -> None:
            raise ValueError("intentional error")

        p = PipelineBuilder()
        p.on_classes_custom(class_named(original), bad_callback)
        with pytest.raises(ValueError, match="intentional error"):
            p.build().apply(m)

    def test_callback_exception_after_mutation_propagates(self, class_bytes: bytes) -> None:
        """Callback that mutates then raises: error propagates, model reflects mutation."""
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        original_name = m.name

        def mutate_then_raise(model: object) -> None:
            model.name = "com/partial/Renamed"  # type: ignore[attr-defined]
            raise RuntimeError("mutation then error")

        p = PipelineBuilder()
        p.on_classes_custom(class_named(original_name), mutate_then_raise)
        with pytest.raises(RuntimeError, match="mutation then error"):
            p.build().apply(m)
        # Mutation happened before the raise; model reflects it
        assert m.name == "com/partial/Renamed"

    def test_on_methods_custom_class_scoped(self, class_bytes: bytes) -> None:
        """on_methods_custom fires at most once per class (class-scoped, not per-method)."""
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        invocations: list[int] = []

        def track(model: object) -> None:
            invocations.append(1)

        p = PipelineBuilder()
        p.on_methods_custom(has_code(), track)
        p.build().apply(m)

        # Fires at most once per class regardless of how many methods have code
        assert len(invocations) <= 1

    def test_invalid_regex_raises_at_construction(self) -> None:
        """Invalid regex pattern should raise ValueError at matcher construction time."""
        from pytecode._rust import ClassMatcher, FieldMatcher, MethodMatcher

        with pytest.raises(ValueError, match="invalid regex"):
            ClassMatcher.name_matches("[invalid")
        with pytest.raises(ValueError, match="invalid regex"):
            FieldMatcher.name_matches("[invalid")
        with pytest.raises(ValueError, match="invalid regex"):
            FieldMatcher.descriptor_matches("[invalid")
        with pytest.raises(ValueError, match="invalid regex"):
            MethodMatcher.name_matches("[invalid")
        with pytest.raises(ValueError, match="invalid regex"):
            MethodMatcher.descriptor_matches("[invalid")

    def test_compiled_with_callback(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)

        def rename(model: object) -> None:
            model.name = "com/compiled/Callback"  # type: ignore[attr-defined]

        p = PipelineBuilder()
        p.on_classes_custom(class_named(m.name), rename)
        compiled = p.compile()
        compiled.apply(m)

        assert m.name == "com/compiled/Callback"

    def test_interfaces_view_invalidates_after_replace(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        view = m.interfaces
        original = list(view)

        m.interfaces = [*original, "com/callback/Added"]

        with pytest.raises(RuntimeError, match="stale"):
            len(view)

    def test_method_ref_invalidates_after_method_list_replace(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        methods = m.methods
        first = methods[0]

        m.methods = list(methods)

        with pytest.raises(RuntimeError, match="stale"):
            _ = first.name

    def test_constant_pool_view_mutates_owner(self, class_bytes: bytes) -> None:
        from pytecode._rust import ClassModel

        m = ClassModel.from_bytes(class_bytes)
        cp = m.constant_pool
        before = cp.count()

        added = cp.add_utf8("bridge/ViewTest")

        assert added >= before
        assert cp.count() == before + 1
        assert len(m.to_bytes()) > 0
