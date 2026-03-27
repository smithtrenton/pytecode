"""Tests for ``pytecode.hierarchy``."""

from __future__ import annotations

from pathlib import Path

import pytest

from pytecode.class_reader import ClassReader
from pytecode.constants import ClassAccessFlag, MethodAccessFlag
from pytecode.hierarchy import (
    JAVA_LANG_OBJECT,
    HierarchyCycleError,
    InheritedMethod,
    MappingClassResolver,
    ResolvedClass,
    ResolvedMethod,
    UnresolvedClassError,
    common_superclass,
    find_overridden_methods,
    is_subtype,
    iter_superclasses,
    iter_supertypes,
)
from pytecode.model import ClassModel, MethodModel
from tests.helpers import TEST_RESOURCES, compile_java_sources

_PACKAGE_PRIVATE = MethodAccessFlag(0)
_FIXTURE_NAME = "fixture/hierarchy/HierarchyFixture"
_MAMMAL_NAME = "fixture/hierarchy/Mammal"
_ANIMAL_NAME = "fixture/hierarchy/Animal"
_PET_NAME = "fixture/hierarchy/Pet"
_TRAINABLE_NAME = "fixture/hierarchy/Trainable"


@pytest.fixture
def hierarchy_resolver(tmp_path: Path) -> MappingClassResolver:
    classes_dir = compile_java_sources(tmp_path, [TEST_RESOURCES / "HierarchyFixture.java"])
    classfiles = [ClassReader(path.read_bytes()).class_info for path in sorted(classes_dir.rglob("*.class"))]
    return MappingClassResolver.from_classfiles(classfiles)


def _method(names: tuple[InheritedMethod, ...]) -> set[str]:
    return {entry.owner for entry in names}


def test_resolved_class_from_classfiles_reads_symbolic_metadata(hierarchy_resolver: MappingClassResolver) -> None:
    resolved = hierarchy_resolver.resolve_class(_FIXTURE_NAME)
    assert resolved is not None
    assert resolved.name == _FIXTURE_NAME
    assert resolved.super_name == _MAMMAL_NAME
    assert resolved.interfaces == (_PET_NAME,)
    assert resolved.find_method("train", "()V") is not None


def test_mapping_resolver_from_models_preserves_symbolic_class_metadata() -> None:
    base = ClassModel(
        version=(52, 0),
        access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        name="example/Base",
        super_name=JAVA_LANG_OBJECT,
        interfaces=[],
        fields=[],
        methods=[MethodModel(MethodAccessFlag.PUBLIC, "hook", "()V", None, [])],
        attributes=[],
    )
    child = ClassModel(
        version=(52, 0),
        access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        name="example/Child",
        super_name="example/Base",
        interfaces=[],
        fields=[],
        methods=[],
        attributes=[],
    )
    resolver = MappingClassResolver.from_models([base, child])

    resolved = resolver.resolve_class("example/Base")
    assert resolved is not None
    assert resolved.find_method("hook", "()V") is not None
    assert is_subtype(resolver, "example/Child", "example/Base")


def test_iter_superclasses_uses_implicit_object_root() -> None:
    resolver = MappingClassResolver(
        [
            ResolvedClass("example/Base", JAVA_LANG_OBJECT, (), ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER),
            ResolvedClass("example/Child", "example/Base", (), ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER),
        ]
    )

    assert [resolved.name for resolved in iter_superclasses(resolver, "example/Child")] == [
        "example/Base",
        JAVA_LANG_OBJECT,
    ]


def test_iter_supertypes_includes_superclasses_and_interfaces(
    hierarchy_resolver: MappingClassResolver,
) -> None:
    names = [resolved.name for resolved in iter_supertypes(hierarchy_resolver, _FIXTURE_NAME)]

    assert names[:3] == [_MAMMAL_NAME, _ANIMAL_NAME, JAVA_LANG_OBJECT]
    assert _TRAINABLE_NAME in names
    assert _PET_NAME in names


def test_is_subtype_traverses_class_and_interface_edges(hierarchy_resolver: MappingClassResolver) -> None:
    assert is_subtype(hierarchy_resolver, _FIXTURE_NAME, _MAMMAL_NAME)
    assert is_subtype(hierarchy_resolver, _FIXTURE_NAME, _ANIMAL_NAME)
    assert is_subtype(hierarchy_resolver, _FIXTURE_NAME, _PET_NAME)
    assert is_subtype(hierarchy_resolver, _FIXTURE_NAME, _TRAINABLE_NAME)
    assert is_subtype(hierarchy_resolver, _FIXTURE_NAME, JAVA_LANG_OBJECT)
    assert not is_subtype(hierarchy_resolver, _MAMMAL_NAME, _PET_NAME)


def test_common_superclass_follows_linear_superclass_chain(hierarchy_resolver: MappingClassResolver) -> None:
    assert common_superclass(hierarchy_resolver, _FIXTURE_NAME, _MAMMAL_NAME) == _MAMMAL_NAME
    assert common_superclass(hierarchy_resolver, _FIXTURE_NAME, _PET_NAME) == JAVA_LANG_OBJECT
    assert common_superclass(hierarchy_resolver, _PET_NAME, _TRAINABLE_NAME) == JAVA_LANG_OBJECT
    assert common_superclass(hierarchy_resolver, _PET_NAME, _PET_NAME) == _PET_NAME


def test_find_overridden_methods_reports_same_package_and_interface_matches(
    hierarchy_resolver: MappingClassResolver,
) -> None:
    resolved = hierarchy_resolver.resolve_class(_FIXTURE_NAME)
    assert resolved is not None

    train = resolved.find_method("train", "()V")
    package_hook = resolved.find_method("packageHook", "()V")
    protected_hook = resolved.find_method("protectedHook", "()V")
    assert train is not None
    assert package_hook is not None
    assert protected_hook is not None

    assert _method(find_overridden_methods(hierarchy_resolver, _FIXTURE_NAME, train)) == {
        _MAMMAL_NAME,
        _TRAINABLE_NAME,
    }
    assert _method(find_overridden_methods(hierarchy_resolver, _FIXTURE_NAME, package_hook)) == {_MAMMAL_NAME}
    assert _method(find_overridden_methods(hierarchy_resolver, _FIXTURE_NAME, protected_hook)) == {_MAMMAL_NAME}


def test_find_overridden_methods_respects_access_and_non_overridable_flags() -> None:
    resolver = MappingClassResolver(
        [
            ResolvedClass(
                "base/Base",
                JAVA_LANG_OBJECT,
                (),
                ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
                methods=(
                    ResolvedMethod("publicHook", "()V", MethodAccessFlag.PUBLIC),
                    ResolvedMethod("protectedHook", "()V", MethodAccessFlag.PROTECTED),
                    ResolvedMethod("packageHook", "()V", _PACKAGE_PRIVATE),
                    ResolvedMethod("privateHook", "()V", MethodAccessFlag.PRIVATE),
                    ResolvedMethod("staticHook", "()V", MethodAccessFlag.PUBLIC | MethodAccessFlag.STATIC),
                    ResolvedMethod("finalHook", "()V", MethodAccessFlag.PUBLIC | MethodAccessFlag.FINAL),
                ),
            ),
            ResolvedClass("other/Sub", "base/Base", (), ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER),
            ResolvedClass("base/SamePackageSub", "base/Base", (), ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER),
        ]
    )

    assert _method(
        find_overridden_methods(resolver, "other/Sub", ResolvedMethod("publicHook", "()V", MethodAccessFlag.PUBLIC))
    ) == {"base/Base"}
    assert _method(
        find_overridden_methods(
            resolver,
            "other/Sub",
            ResolvedMethod("protectedHook", "()V", MethodAccessFlag.PUBLIC),
        )
    ) == {"base/Base"}
    assert find_overridden_methods(
        resolver,
        "other/Sub",
        ResolvedMethod("packageHook", "()V", MethodAccessFlag.PUBLIC),
    ) == ()
    assert _method(
        find_overridden_methods(
            resolver,
            "base/SamePackageSub",
            ResolvedMethod("packageHook", "()V", MethodAccessFlag.PUBLIC),
        )
    ) == {"base/Base"}
    assert find_overridden_methods(
        resolver,
        "other/Sub",
        ResolvedMethod("privateHook", "()V", MethodAccessFlag.PUBLIC),
    ) == ()
    assert find_overridden_methods(
        resolver,
        "other/Sub",
        ResolvedMethod("staticHook", "()V", MethodAccessFlag.PUBLIC),
    ) == ()
    assert find_overridden_methods(
        resolver,
        "other/Sub",
        ResolvedMethod("finalHook", "()V", MethodAccessFlag.PUBLIC),
    ) == ()


def test_find_overridden_methods_returns_empty_for_private_static_and_special_declarations() -> None:
    resolver = MappingClassResolver(
        [
            ResolvedClass(
                "base/Base",
                JAVA_LANG_OBJECT,
                (),
                ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
                methods=(ResolvedMethod("hook", "()V", MethodAccessFlag.PUBLIC),),
            ),
            ResolvedClass("base/Sub", "base/Base", (), ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER),
        ]
    )

    assert find_overridden_methods(
        resolver,
        "base/Sub",
        ResolvedMethod("hook", "()V", MethodAccessFlag.PRIVATE),
    ) == ()
    assert find_overridden_methods(
        resolver,
        "base/Sub",
        ResolvedMethod("hook", "()V", MethodAccessFlag.STATIC),
    ) == ()
    assert find_overridden_methods(
        resolver,
        "base/Sub",
        ResolvedMethod("<init>", "()V", MethodAccessFlag.PUBLIC),
    ) == ()
    assert find_overridden_methods(
        resolver,
        "base/Sub",
        ResolvedMethod("<clinit>", "()V", MethodAccessFlag.STATIC),
    ) == ()


def test_iter_superclasses_raises_on_cycle() -> None:
    resolver = MappingClassResolver(
        [
            ResolvedClass("cycle/A", "cycle/B", (), ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER),
            ResolvedClass("cycle/B", "cycle/A", (), ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER),
        ]
    )

    with pytest.raises(HierarchyCycleError, match="cycle/A -> cycle/B -> cycle/A") as exc_info:
        list(iter_superclasses(resolver, "cycle/A", include_self=True))

    assert exc_info.value.cycle == ("cycle/A", "cycle/B", "cycle/A")


def test_missing_classes_raise_explicit_errors() -> None:
    resolver = MappingClassResolver(
        [ResolvedClass("missing/Sub", "missing/Base", (), ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER)]
    )

    with pytest.raises(UnresolvedClassError, match="missing/Base"):
        list(iter_superclasses(resolver, "missing/Sub"))

    with pytest.raises(UnresolvedClassError, match="missing/Base"):
        is_subtype(resolver, "missing/Sub", "missing/Base")
