"""Tests for pytecode.transforms — composable transform helpers."""

from __future__ import annotations

import copy
import re
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest

from pytecode.constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from pytecode.jar import JarFile
from pytecode.model import ClassModel, CodeModel, FieldModel, MethodModel
from pytecode.transforms import (
    ClassPredicate,
    FieldPredicate,
    Matcher,
    MethodPredicate,
    Pipeline,
    all_of,
    any_of,
    class_access,
    class_access_any,
    class_is_abstract,
    class_is_annotation,
    class_is_enum,
    class_is_final,
    class_is_interface,
    class_is_module,
    class_is_package_private,
    class_is_public,
    class_is_synthetic,
    class_name_matches,
    class_named,
    class_version,
    class_version_at_least,
    class_version_below,
    extends,
    field_access,
    field_access_any,
    field_descriptor,
    field_descriptor_matches,
    field_is_enum_constant,
    field_is_final,
    field_is_package_private,
    field_is_private,
    field_is_protected,
    field_is_public,
    field_is_static,
    field_is_synthetic,
    field_is_transient,
    field_is_volatile,
    field_name_matches,
    field_named,
    has_code,
    implements,
    is_constructor,
    is_static_initializer,
    method_access,
    method_access_any,
    method_descriptor,
    method_descriptor_matches,
    method_is_abstract,
    method_is_bridge,
    method_is_final,
    method_is_native,
    method_is_package_private,
    method_is_private,
    method_is_protected,
    method_is_public,
    method_is_static,
    method_is_strict,
    method_is_synchronized,
    method_is_synthetic,
    method_is_varargs,
    method_name_matches,
    method_named,
    method_returns,
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

    def remove_field(field: FieldModel, _owner: ClassModel) -> None:
        visited.append(field.name)
        model.fields.remove(field)

    on_fields(remove_field)(model)

    assert visited == ["first", "second"]
    assert model.fields == []


def test_on_fields_applies_where_predicate() -> None:
    target = _field("target")
    other = _field("other")
    model = _class(fields=[target, other])

    def make_static(field: FieldModel, _owner: ClassModel) -> None:
        field.access_flags |= FieldAccessFlag.STATIC

    on_fields(make_static, where=field_named("target"))(model)

    assert FieldAccessFlag.STATIC in target.access_flags
    assert FieldAccessFlag.STATIC not in other.access_flags


def test_on_fields_rejects_non_none_return() -> None:
    model = _class(fields=[_field("target")])

    def bad(field: FieldModel, _owner: ClassModel) -> Any:
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

    def make_final(method: MethodModel, _owner: ClassModel) -> None:
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

    def bad(method: MethodModel, _owner: ClassModel) -> Any:
        return method.name

    with pytest.raises(TypeError, match="Method transforms must mutate MethodModel in place and return None"):
        on_methods(bad)(model)


def test_on_code_skips_methods_without_code_without_where_filter() -> None:
    concrete = _method("main")
    abstract = _method("shape", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT, code=None)
    native = _method("nativeCall", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.NATIVE, code=None)
    model = _class(methods=[concrete, abstract, native])
    visited: list[str] = []

    def grow_stack(code: CodeModel, _method: MethodModel, _owner: ClassModel) -> None:
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

    def grow_stack(code: CodeModel, _method: MethodModel, _owner: ClassModel) -> None:
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

    def bad(code: CodeModel, _method: MethodModel, _owner: ClassModel) -> Any:
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

    def make_main_final(method: MethodModel, _owner: ClassModel) -> None:
        method.access_flags |= MethodAccessFlag.FINAL

    transform = pipeline(
        on_methods(
            make_main_final,
            where=method_name_matches(r"main") & method_is_public() & method_is_static(),
            owner=class_named("HelloWorld"),
        )
    )
    jar.rewrite(out_path, transform=transform)

    rewritten = JarFile(out_path)
    model = ClassModel.from_bytes(rewritten.files["HelloWorld.class"].bytes)
    methods = {method.name: method for method in model.methods}

    assert MethodAccessFlag.FINAL in methods["main"].access_flags
    assert MethodAccessFlag.FINAL not in methods["<init>"].access_flags


def test_matcher_of_is_idempotent_and_can_override_description() -> None:
    def starts_with_run(method: MethodModel) -> bool:
        return method.name.startswith("run")

    matcher = Matcher.of(starts_with_run)

    assert Matcher.of(matcher) is matcher
    assert matcher(_method("runNow")) is True
    assert repr(matcher) == "Matcher[starts_with_run]"

    custom = Matcher.of(matcher, "starts_with_run()")

    assert custom is not matcher
    assert custom(_method("runLater")) is True
    assert repr(custom) == "Matcher[starts_with_run()]"


def test_matcher_of_rejects_non_callable_predicates() -> None:
    with pytest.raises(TypeError, match="Matcher predicates must be callable"):
        Matcher.of(1)  # pyright: ignore[reportArgumentType]


def test_matcher_operator_composition_supports_and_or_not() -> None:
    matcher = (method_named("run") & ~method_is_static()) | is_constructor()

    assert matcher(_method("run")) is True
    assert matcher(_method("<init>")) is True
    assert matcher(_method("run", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.STATIC)) is False
    assert repr(matcher) == "Matcher[((method_named('run') & ~method_is_static()) | is_constructor())]"


def test_matcher_operators_accept_plain_callables_on_either_side() -> None:
    def is_run(method: MethodModel) -> bool:
        return method.name == "run"

    left = is_run & has_code()
    right = method_is_static() | is_run

    assert left(_method("run")) is True
    assert left(_method("other")) is False
    assert right(_method("run", access_flags=MethodAccessFlag.PUBLIC)) is True
    assert right(_method("other", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.STATIC)) is True
    assert right(_method("other", access_flags=MethodAccessFlag.PUBLIC)) is False


def test_all_of_and_any_of_handle_empty_input() -> None:
    method = _method("run")

    assert all_of()(method) is True
    assert any_of()(method) is False


@pytest.mark.parametrize(
    ("matcher", "matching", "non_matching"),
    [
        (
            class_name_matches(r"example/.+"),
            _class("example/Test"),
            _class("other/Test"),
        ),
        (
            field_name_matches(r"value\d+"),
            _field("value7"),
            _field("prefix_value7"),
        ),
        (
            field_descriptor_matches(r"L.+;"),
            _field("value", "Ljava/lang/String;"),
            _field("value", "[Ljava/lang/String;"),
        ),
        (
            method_name_matches(r"get[A-Z].+"),
            _method("getValue"),
            _method("helper"),
        ),
        (
            method_descriptor_matches(r"\(I\)V"),
            _method("run", "(I)V"),
            _method("run", "(II)V"),
        ),
    ],
)
def test_regex_matchers_use_fullmatch_semantics(
    matcher: Matcher[object],
    matching: object,
    non_matching: object,
) -> None:
    assert matcher(matching) is True
    assert matcher(non_matching) is False


@pytest.mark.parametrize(
    "factory",
    [
        class_name_matches,
        field_name_matches,
        field_descriptor_matches,
        method_name_matches,
        method_descriptor_matches,
    ],
)
def test_regex_matchers_raise_for_invalid_patterns(factory: Any) -> None:
    with pytest.raises(re.error):
        factory("[")


def test_access_any_matchers_require_any_requested_flag() -> None:
    assert class_access_any(ClassAccessFlag.PUBLIC | ClassAccessFlag.FINAL)(
        _class(access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER)
    )
    assert not class_access_any(ClassAccessFlag.FINAL | ClassAccessFlag.INTERFACE)(_class())

    assert field_access_any(FieldAccessFlag.STATIC | FieldAccessFlag.FINAL)(
        _field("value", access_flags=FieldAccessFlag.PRIVATE | FieldAccessFlag.STATIC)
    )
    assert not field_access_any(FieldAccessFlag.PROTECTED | FieldAccessFlag.ENUM)(_field("value"))

    assert method_access_any(MethodAccessFlag.STATIC | MethodAccessFlag.FINAL)(
        _method("run", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.STATIC)
    )
    assert not method_access_any(MethodAccessFlag.ABSTRACT | MethodAccessFlag.NATIVE)(_method("run"))


@pytest.mark.parametrize(
    ("matcher_factory", "matching_flags", "non_matching_flags"),
    [
        (
            class_is_public,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            ClassAccessFlag.SUPER,
        ),
        (
            class_is_package_private,
            ClassAccessFlag.SUPER,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        ),
        (
            class_is_final,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.FINAL,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        ),
        (
            class_is_interface,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.INTERFACE | ClassAccessFlag.ABSTRACT,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        ),
        (
            class_is_abstract,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        ),
        (
            class_is_synthetic,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SYNTHETIC,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        ),
        (
            class_is_annotation,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.ANNOTATION | ClassAccessFlag.INTERFACE,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        ),
        (
            class_is_enum,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.ENUM,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        ),
        (
            class_is_module,
            ClassAccessFlag.MODULE,
            ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        ),
    ],
)
def test_class_access_convenience_matchers(
    matcher_factory: Callable[[], ClassPredicate],
    matching_flags: ClassAccessFlag,
    non_matching_flags: ClassAccessFlag,
) -> None:
    assert matcher_factory()(_class(access_flags=matching_flags)) is True
    assert matcher_factory()(_class(access_flags=non_matching_flags)) is False


@pytest.mark.parametrize(
    ("matcher_factory", "matching_flags", "non_matching_flags"),
    [
        (
            field_is_public,
            FieldAccessFlag.PUBLIC,
            FieldAccessFlag.PRIVATE,
        ),
        (
            field_is_private,
            FieldAccessFlag.PRIVATE,
            FieldAccessFlag.PUBLIC,
        ),
        (
            field_is_protected,
            FieldAccessFlag.PROTECTED,
            FieldAccessFlag.PUBLIC,
        ),
        (
            field_is_package_private,
            FieldAccessFlag.STATIC,
            FieldAccessFlag.PUBLIC,
        ),
        (
            field_is_static,
            FieldAccessFlag.STATIC,
            FieldAccessFlag.PRIVATE,
        ),
        (
            field_is_final,
            FieldAccessFlag.FINAL,
            FieldAccessFlag.PRIVATE,
        ),
        (
            field_is_volatile,
            FieldAccessFlag.VOLATILE,
            FieldAccessFlag.PRIVATE,
        ),
        (
            field_is_transient,
            FieldAccessFlag.TRANSIENT,
            FieldAccessFlag.PRIVATE,
        ),
        (
            field_is_synthetic,
            FieldAccessFlag.SYNTHETIC,
            FieldAccessFlag.PRIVATE,
        ),
        (
            field_is_enum_constant,
            FieldAccessFlag.ENUM,
            FieldAccessFlag.PRIVATE,
        ),
    ],
)
def test_field_access_convenience_matchers(
    matcher_factory: Callable[[], FieldPredicate],
    matching_flags: FieldAccessFlag,
    non_matching_flags: FieldAccessFlag,
) -> None:
    assert matcher_factory()(_field("value", access_flags=matching_flags)) is True
    assert matcher_factory()(_field("value", access_flags=non_matching_flags)) is False


@pytest.mark.parametrize(
    ("matcher_factory", "matching_flags", "non_matching_flags"),
    [
        (
            method_is_public,
            MethodAccessFlag.PUBLIC,
            MethodAccessFlag.PRIVATE,
        ),
        (
            method_is_private,
            MethodAccessFlag.PRIVATE,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_protected,
            MethodAccessFlag.PROTECTED,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_package_private,
            MethodAccessFlag.FINAL,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_static,
            MethodAccessFlag.STATIC,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_final,
            MethodAccessFlag.FINAL,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_synchronized,
            MethodAccessFlag.SYNCHRONIZED,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_bridge,
            MethodAccessFlag.BRIDGE,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_varargs,
            MethodAccessFlag.VARARGS,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_native,
            MethodAccessFlag.NATIVE,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_abstract,
            MethodAccessFlag.ABSTRACT,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_strict,
            MethodAccessFlag.STRICT,
            MethodAccessFlag.PUBLIC,
        ),
        (
            method_is_synthetic,
            MethodAccessFlag.SYNTHETIC,
            MethodAccessFlag.PUBLIC,
        ),
    ],
)
def test_method_access_convenience_matchers(
    matcher_factory: Callable[[], MethodPredicate],
    matching_flags: MethodAccessFlag,
    non_matching_flags: MethodAccessFlag,
) -> None:
    assert matcher_factory()(_method("run", access_flags=matching_flags)) is True
    assert matcher_factory()(_method("run", access_flags=non_matching_flags)) is False


def test_class_semantic_matchers_cover_super_interface_and_version() -> None:
    model = _class("example/Widget")
    model.super_name = "example/BaseWidget"
    model.interfaces = ["java/io/Serializable", "java/lang/Runnable"]
    model.version = (61, 0)

    assert extends("example/BaseWidget")(model) is True
    assert extends("java/lang/Object")(model) is False
    assert implements("java/io/Serializable")(model) is True
    assert implements("java/lang/Cloneable")(model) is False
    assert class_version(61)(model) is True
    assert class_version(52)(model) is False
    assert class_version_at_least(55)(model) is True
    assert class_version_below(65)(model) is True
    assert class_version_below(61)(model) is False


def test_method_semantic_matchers_cover_constructor_initializer_and_returns() -> None:
    constructor = _method("<init>", "(Ljava/lang/String;)V")
    initializer = _method(
        "<clinit>",
        "()V",
        access_flags=MethodAccessFlag.STATIC,
    )
    object_return = _method("name", "()Ljava/lang/String;")
    array_return = _method("matrix", "()[[I")
    invalid = _method("broken", "not-a-descriptor")
    missing_return = _method("missingReturn", "()")
    extra_close = _method("extraClose", "())V")

    assert is_constructor()(constructor) is True
    assert is_constructor()(initializer) is False
    assert is_static_initializer()(initializer) is True
    assert is_static_initializer()(constructor) is False
    assert method_returns("V")(constructor) is True
    assert method_returns("Ljava/lang/String;")(object_return) is True
    assert method_returns("[[I")(array_return) is True
    assert method_returns("I")(object_return) is False
    assert method_returns("V")(invalid) is False
    assert method_returns("")(missing_return) is False
    assert method_returns("V")(extra_close) is False


def test_special_method_name_matchers_remain_lightweight() -> None:
    odd_constructor = _method("<init>", "()I")
    odd_initializer = _method("<clinit>", "()I")

    assert is_constructor()(odd_constructor) is True
    assert is_static_initializer()(odd_initializer) is True


def test_on_fields_owner_filters_classes() -> None:
    target = _class(
        "example/Target",
        fields=[_field("value")],
        access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER | ClassAccessFlag.ENUM,
    )
    other = _class("example/Other", fields=[_field("value")])

    def make_static(field: FieldModel, _owner: ClassModel) -> None:
        field.access_flags |= FieldAccessFlag.STATIC

    transform = on_fields(
        make_static,
        where=field_named("value"),
        owner=class_is_enum(),
    )
    transform(target)
    transform(other)

    assert FieldAccessFlag.STATIC in target.fields[0].access_flags
    assert FieldAccessFlag.STATIC not in other.fields[0].access_flags


def test_on_methods_owner_short_circuits_before_where_evaluation() -> None:
    model = _class("example/Other", methods=[_method("run")])

    def fail_if_called(method: MethodModel) -> bool:
        raise AssertionError(f"unexpected where evaluation for {method.name}")

    on_methods(
        lambda method, _owner: None,
        where=fail_if_called,
        owner=class_named("example/Target"),
    )(model)


def test_on_code_owner_filters_classes() -> None:
    target = _class("example/Target", methods=[_method("run")])
    other = _class("example/Other", methods=[_method("run")])

    def grow_stack(code: CodeModel, _method: MethodModel, _owner: ClassModel) -> None:
        code.max_stack += 1

    transform = on_code(
        grow_stack,
        where=method_is_public() & has_code(),
        owner=class_named("example/Target"),
    )
    transform(target)
    transform(other)

    assert target.methods[0].code is not None
    assert other.methods[0].code is not None
    assert target.methods[0].code.max_stack == 2
    assert other.methods[0].code.max_stack == 1


def test_where_and_owner_accept_plain_callable_predicates() -> None:
    target = _class("example/Target", methods=[_method("run")])
    other = _class("example/Other", methods=[_method("run")])

    def make_final(method: MethodModel, _owner: ClassModel) -> None:
        method.access_flags |= MethodAccessFlag.FINAL

    transform = on_methods(
        make_final,
        where=lambda method: method.name == "run",
        owner=lambda model: model.name == "example/Target",
    )
    transform(target)
    transform(other)

    assert MethodAccessFlag.FINAL in target.methods[0].access_flags
    assert MethodAccessFlag.FINAL not in other.methods[0].access_flags


# ---------------------------------------------------------------------------
# Context-aware transform tests
# ---------------------------------------------------------------------------


def test_on_code_passes_method_and_class_context() -> None:
    """on_code should pass the owning MethodModel and ClassModel to the transform."""
    run = _method("run", "(I)V")
    model = _class("example/Widget", methods=[run])
    captured: list[tuple[str, str, str]] = []

    def record_context(code: CodeModel, method: MethodModel, owner: ClassModel) -> None:
        captured.append((owner.name, method.name, method.descriptor))

    on_code(record_context)(model)

    assert captured == [("example/Widget", "run", "(I)V")]


def test_on_methods_passes_class_context() -> None:
    """on_methods should pass the owning ClassModel to the transform."""
    model = _class("example/Service", methods=[_method("start"), _method("stop")])
    captured: list[tuple[str, str]] = []

    def record_context(method: MethodModel, owner: ClassModel) -> None:
        captured.append((owner.name, method.name))

    on_methods(record_context)(model)

    assert captured == [("example/Service", "start"), ("example/Service", "stop")]


def test_on_fields_passes_class_context() -> None:
    """on_fields should pass the owning ClassModel to the transform."""
    model = _class("example/Config", fields=[_field("host"), _field("port", "I")])
    captured: list[tuple[str, str]] = []

    def record_context(field: FieldModel, owner: ClassModel) -> None:
        captured.append((owner.name, field.name))

    on_fields(record_context)(model)

    assert captured == [("example/Config", "host"), ("example/Config", "port")]


def test_on_code_context_with_where_and_owner_predicates() -> None:
    """Predicates should filter before the context-receiving transform is called."""
    target = _class("example/Target", methods=[_method("run"), _method("stop")])
    other = _class("example/Other", methods=[_method("run")])
    captured: list[tuple[str, str]] = []

    def record_context(code: CodeModel, method: MethodModel, owner: ClassModel) -> None:
        captured.append((owner.name, method.name))

    transform = on_code(
        record_context,
        where=method_named("run"),
        owner=class_named("example/Target"),
    )
    transform(target)
    transform(other)

    assert captured == [("example/Target", "run")]


def test_on_methods_context_with_where_predicate() -> None:
    """where predicate should filter methods before context is passed."""
    model = _class("example/App", methods=[_method("init"), _method("run"), _method("cleanup")])
    captured: list[str] = []

    def record(method: MethodModel, _owner: ClassModel) -> None:
        captured.append(method.name)

    on_methods(record, where=method_name_matches(r"init|cleanup"))(model)

    assert captured == ["init", "cleanup"]


def test_context_field_transform_rejects_non_none_return() -> None:
    model = _class(fields=[_field("value")])

    def bad(field: FieldModel, owner: ClassModel) -> Any:
        return (field.name, owner.name)

    with pytest.raises(TypeError, match="Field transforms must mutate FieldModel in place and return None"):
        on_fields(bad)(model)


def test_context_method_transform_rejects_non_none_return() -> None:
    model = _class(methods=[_method("run")])

    def bad(method: MethodModel, owner: ClassModel) -> Any:
        return (method.name, owner.name)

    with pytest.raises(TypeError, match="Method transforms must mutate MethodModel in place and return None"):
        on_methods(bad)(model)


def test_context_code_transform_rejects_non_none_return() -> None:
    model = _class(methods=[_method("run")])

    def bad(code: CodeModel, method: MethodModel, owner: ClassModel) -> Any:
        return (code.max_stack, method.name, owner.name)

    with pytest.raises(TypeError, match="Code transforms must mutate CodeModel in place and return None"):
        on_code(bad)(model)


def test_on_fields_context_uses_snapshot_iteration() -> None:
    """Removing fields during traversal should not skip elements."""
    model = _class(fields=[_field("a"), _field("b"), _field("c")])
    visited: list[str] = []

    def remove_and_record(field: FieldModel, _owner: ClassModel) -> None:
        visited.append(field.name)
        model.fields.remove(field)

    on_fields(remove_and_record)(model)

    assert visited == ["a", "b", "c"]
    assert model.fields == []


def test_on_code_skips_abstract_and_native_methods_with_context() -> None:
    """on_code should skip methods without code even when context is expected."""
    concrete = _method("run")
    abstract = _method("shape", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT, code=None)
    native = _method("nativeOp", access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.NATIVE, code=None)
    model = _class(methods=[concrete, abstract, native])
    visited: list[str] = []

    def record(code: CodeModel, method: MethodModel, _owner: ClassModel) -> None:
        visited.append(method.name)

    on_code(record)(model)

    assert visited == ["run"]


def test_context_transforms_on_empty_class() -> None:
    """Context transforms should be no-ops on a class with no fields or methods."""
    model = _class("example/Empty", fields=[], methods=[])
    field_calls: list[str] = []
    method_calls: list[str] = []
    code_calls: list[str] = []

    on_fields(lambda f, o: field_calls.append(f.name))(model)
    on_methods(lambda m, o: method_calls.append(m.name))(model)
    on_code(lambda c, m, o: code_calls.append(m.name))(model)

    assert field_calls == []
    assert method_calls == []
    assert code_calls == []


def test_context_transform_composes_with_pipeline() -> None:
    """Context-aware transforms lifted via on_* should compose in a Pipeline."""
    model = _class("example/Widget", fields=[_field("value")], methods=[_method("run")])
    log: list[str] = []

    def field_xform(field: FieldModel, owner: ClassModel) -> None:
        log.append(f"field:{owner.name}/{field.name}")
        field.access_flags |= FieldAccessFlag.FINAL

    def method_xform(method: MethodModel, owner: ClassModel) -> None:
        log.append(f"method:{owner.name}/{method.name}")
        method.access_flags |= MethodAccessFlag.FINAL

    transform = pipeline(on_fields(field_xform), on_methods(method_xform))
    transform(model)

    assert log == ["field:example/Widget/value", "method:example/Widget/run"]
    assert FieldAccessFlag.FINAL in model.fields[0].access_flags
    assert MethodAccessFlag.FINAL in model.methods[0].access_flags


def test_context_transform_with_jarfile_rewrite(tmp_path: Path) -> None:
    """Context-aware transforms should work through JarFile.rewrite()."""
    jar_path = make_compiled_jar(
        tmp_path,
        [TEST_RESOURCES / "HelloWorld.java"],
        extra_files={"README.txt": b"fixture"},
    )
    jar = JarFile(jar_path)
    out_path = tmp_path / "context.jar"

    captured_contexts: list[tuple[str, str]] = []

    def record_and_finalize(method: MethodModel, owner: ClassModel) -> None:
        captured_contexts.append((owner.name, method.name))
        if method.name == "main":
            method.access_flags |= MethodAccessFlag.FINAL

    transform = pipeline(
        on_methods(
            record_and_finalize,
            where=method_is_public(),
            owner=class_named("HelloWorld"),
        )
    )
    jar.rewrite(out_path, transform=transform)

    assert any(ctx == ("HelloWorld", "main") for ctx in captured_contexts)

    rewritten = JarFile(out_path)
    model = ClassModel.from_bytes(rewritten.files["HelloWorld.class"].bytes)
    methods = {m.name: m for m in model.methods}
    assert MethodAccessFlag.FINAL in methods["main"].access_flags
