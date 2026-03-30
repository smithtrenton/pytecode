"""Tests for pytecode.transforms — composable transform helpers."""

from __future__ import annotations

import copy
from pathlib import Path
from typing import Any

import pytest

from pytecode.constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from pytecode.jar import JarFile
from pytecode.model import ClassModel, CodeModel, FieldModel, MethodModel
from pytecode.transforms import (
    Pipeline,
    all_of,
    any_of,
    class_access,
    class_named,
    field_access,
    field_descriptor,
    field_named,
    has_code,
    method_access,
    method_descriptor,
    method_named,
    not_,
    on_classes,
    on_code,
    on_fields,
    on_methods,
    pipeline,
)
from tests.helpers import TEST_RESOURCES, make_compiled_jar


def _field(
    name: str,
    descriptor: str = "I",
    *,
    access_flags: FieldAccessFlag = FieldAccessFlag.PRIVATE,
) -> FieldModel:
    return FieldModel(
        access_flags=access_flags,
        name=name,
        descriptor=descriptor,
        attributes=[],
    )


def _method(
    name: str,
    descriptor: str = "()V",
    *,
    access_flags: MethodAccessFlag = MethodAccessFlag.PUBLIC,
    code: CodeModel | None = None,
) -> MethodModel:
    default_code = code
    if (
        default_code is None
        and MethodAccessFlag.ABSTRACT not in access_flags
        and MethodAccessFlag.NATIVE not in access_flags
    ):
        default_code = CodeModel(max_stack=1, max_locals=1)

    return MethodModel(
        access_flags=access_flags,
        name=name,
        descriptor=descriptor,
        code=default_code,
        attributes=[],
    )


def _class(
    name: str = "example/Test",
    *,
    fields: list[FieldModel] | None = None,
    methods: list[MethodModel] | None = None,
    access_flags: ClassAccessFlag = ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
) -> ClassModel:
    return ClassModel(
        version=(52, 0),
        access_flags=access_flags,
        name=name,
        super_name="java/lang/Object",
        interfaces=[],
        fields=[] if fields is None else fields,
        methods=[] if methods is None else methods,
        attributes=[],
    )


def test_pipeline_applies_transforms_in_order() -> None:
    model = _class()
    events: list[str] = []

    def rename(model: ClassModel) -> None:
        events.append("rename")
        model.name = "example/Renamed"

    def retarget_super(model: ClassModel) -> None:
        events.append(model.name)
        model.super_name = "example/Base"

    transform = pipeline(rename, retarget_super)
    transform(model)

    assert events == ["rename", "example/Renamed"]
    assert model.super_name == "example/Base"


def test_pipeline_then_flattens_nested_pipelines() -> None:
    model = _class()
    events: list[str] = []

    def first(model: ClassModel) -> None:
        events.append("first")

    def second(model: ClassModel) -> None:
        events.append("second")

    def third(model: ClassModel) -> None:
        events.append("third")

    transform = Pipeline.of(first).then(pipeline(second), third)
    transform(model)

    assert events == ["first", "second", "third"]
    assert len(transform.transforms) == 3


def test_pipeline_empty_is_noop() -> None:
    model = _class(fields=[_field("value")], methods=[_method("run")])
    snapshot = copy.deepcopy((model.name, model.super_name, model.fields, model.methods, model.attributes))

    pipeline()(model)

    assert (model.name, model.super_name, model.fields, model.methods, model.attributes) == snapshot


def test_pipeline_rejects_non_none_class_return() -> None:
    model = _class()

    def bad(model: ClassModel) -> Any:
        return 1

    with pytest.raises(TypeError, match="Class transforms must mutate ClassModel in place and return None"):
        pipeline(bad)(model)


def test_on_classes_applies_conditionally() -> None:
    target = _class("example/Target")
    other = _class("example/Other")

    def make_final(model: ClassModel) -> None:
        model.access_flags |= ClassAccessFlag.FINAL

    transform = on_classes(
        make_final,
        where=all_of(
            class_named("example/Target"),
            class_access(ClassAccessFlag.PUBLIC),
        ),
    )
    transform(target)
    transform(other)

    assert ClassAccessFlag.FINAL in target.access_flags
    assert ClassAccessFlag.FINAL not in other.access_flags


def test_on_fields_uses_snapshot_iteration_when_collection_changes() -> None:
    fields = [_field("first"), _field("second")]
    model = _class(fields=fields.copy())
    visited: list[str] = []

    def remove_field(field: FieldModel) -> None:
        visited.append(field.name)
        model.fields.remove(field)

    on_fields(remove_field)(model)

    assert visited == ["first", "second"]
    assert model.fields == []


def test_on_fields_applies_where_predicate() -> None:
    target = _field("target")
    other = _field("other")
    model = _class(fields=[target, other])

    def make_static(field: FieldModel) -> None:
        field.access_flags |= FieldAccessFlag.STATIC

    on_fields(make_static, where=field_named("target"))(model)

    assert FieldAccessFlag.STATIC in target.access_flags
    assert FieldAccessFlag.STATIC not in other.access_flags


def test_on_fields_rejects_non_none_return() -> None:
    model = _class(fields=[_field("target")])

    def bad(field: FieldModel) -> Any:
        return field.name

    with pytest.raises(TypeError, match="Field transforms must mutate FieldModel in place and return None"):
        on_fields(bad)(model)


def test_on_methods_filters_by_name_descriptor_and_access() -> None:
    target = _method(
        "target",
        "(I)V",
        access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.STATIC,
    )
    other = _method("other", "(I)V", access_flags=MethodAccessFlag.PUBLIC)
    model = _class(methods=[target, other])

    def make_final(method: MethodModel) -> None:
        method.access_flags |= MethodAccessFlag.FINAL

    transform = on_methods(
        make_final,
        where=all_of(
            method_named("target"),
            method_descriptor("(I)V"),
            method_access(MethodAccessFlag.PUBLIC | MethodAccessFlag.STATIC),
        ),
    )
    transform(model)

    assert MethodAccessFlag.FINAL in target.access_flags
    assert MethodAccessFlag.FINAL not in other.access_flags


def test_on_methods_rejects_non_none_return() -> None:
    model = _class(methods=[_method("target")])

    def bad(method: MethodModel) -> Any:
        return method.name

    with pytest.raises(TypeError, match="Method transforms must mutate MethodModel in place and return None"):
        on_methods(bad)(model)


def test_on_code_skips_methods_without_code_without_where_filter() -> None:
    concrete = _method("main")
    abstract = _method("shape", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT, code=None)
    native = _method("nativeCall", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.NATIVE, code=None)
    model = _class(methods=[concrete, abstract, native])
    visited: list[str] = []

    def grow_stack(code: CodeModel) -> None:
        visited.append("code")
        code.max_stack += 1

    on_code(grow_stack)(model)

    assert visited == ["code"]
    assert concrete.code is not None
    assert concrete.code.max_stack == 2
    assert abstract.code is None
    assert native.code is None


def test_on_code_filters_by_method_predicate() -> None:
    target = _method("main")
    other = _method("helper")
    abstract = _method("shape", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT, code=None)
    model = _class(methods=[target, other, abstract])
    visited: list[str] = []

    def grow_stack(code: CodeModel) -> None:
        visited.append("code")
        code.max_stack += 1

    on_code(grow_stack, where=all_of(method_named("main"), has_code()))(model)

    assert visited == ["code"]
    assert target.code is not None
    assert target.code.max_stack == 2
    assert other.code is not None
    assert other.code.max_stack == 1
    assert abstract.code is None


def test_on_code_rejects_non_none_return() -> None:
    model = _class(methods=[_method("target")])

    def bad(code: CodeModel) -> Any:
        return code.max_stack

    with pytest.raises(TypeError, match="Code transforms must mutate CodeModel in place and return None"):
        on_code(bad)(model)


def test_predicate_combinators_cover_field_and_method_helpers() -> None:
    field = _field("count", "I", access_flags=FieldAccessFlag.PRIVATE | FieldAccessFlag.STATIC)
    method = _method("run", "(I)V", access_flags=MethodAccessFlag.PUBLIC)

    field_predicate = all_of(
        field_named("count"),
        field_descriptor("I"),
        field_access(FieldAccessFlag.PRIVATE),
    )
    method_predicate = any_of(
        method_named("other"),
        all_of(
            has_code(),
            method_named("run"),
            method_descriptor("(I)V"),
            not_(method_access(MethodAccessFlag.STATIC)),
        ),
    )

    assert field_predicate(field) is True
    assert method_predicate(method) is True


def test_jarfile_rewrite_accepts_pipeline_transform(tmp_path: Path) -> None:
    jar_path = make_compiled_jar(
        tmp_path,
        [TEST_RESOURCES / "HelloWorld.java"],
        extra_files={"README.txt": b"fixture"},
    )
    jar = JarFile(jar_path)
    out_path = tmp_path / "pipeline.jar"

    def make_main_final(method: MethodModel) -> None:
        method.access_flags |= MethodAccessFlag.FINAL

    transform = pipeline(on_methods(make_main_final, where=method_named("main")))
    jar.rewrite(out_path, transform=transform)

    rewritten = JarFile(out_path)
    model = ClassModel.from_bytes(rewritten.files["HelloWorld.class"].bytes)
    methods = {method.name: method for method in model.methods}

    assert MethodAccessFlag.FINAL in methods["main"].access_flags
    assert MethodAccessFlag.FINAL not in methods["<init>"].access_flags
