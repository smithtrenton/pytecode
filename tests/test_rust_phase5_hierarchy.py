from __future__ import annotations

from pathlib import Path

import pytest

import pytecode.analysis.hierarchy as hierarchy_module
from pytecode.analysis.hierarchy import (
    JAVA_LANG_OBJECT,
    MappingClassResolver,
    ResolvedClass,
    ResolvedMethod,
)
from pytecode.classfile.constants import ClassAccessFlag, MethodAccessFlag
from pytecode.classfile.info import ClassFile
from pytecode.classfile.reader import ClassReader
from pytecode.edit.model import ClassModel, MethodModel
from tests.helpers import TEST_RESOURCES, compile_java_sources


def _unexpected_python_path(*_args: object, **_kwargs: object) -> object:
    raise AssertionError("expected the public hierarchy wrapper to use the Rust-backed path")


def _load_fixture_classfiles(tmp_path: Path) -> list[ClassFile]:
    classes_dir = compile_java_sources(tmp_path, [TEST_RESOURCES / "HierarchyFixture.java"])
    return [ClassReader(path.read_bytes()).class_info for path in sorted(classes_dir.rglob("*.class"))]


@pytest.mark.skipif(
    not hierarchy_module._RUST_HIERARCHY_AVAILABLE,
    reason="Rust hierarchy backend is not installed in this environment",
)
def test_resolved_class_from_classfile_wrapper_uses_rust(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    classfile = _load_fixture_classfiles(tmp_path)[0]
    monkeypatch.setattr(
        hierarchy_module,
        "_resolved_class_from_classfile_python",
        _unexpected_python_path,
    )

    resolved = ResolvedClass.from_classfile(classfile)

    assert resolved.name.startswith("fixture/hierarchy/")


@pytest.mark.skipif(
    not hierarchy_module._RUST_HIERARCHY_AVAILABLE,
    reason="Rust hierarchy backend is not installed in this environment",
)
def test_hierarchy_query_wrappers_use_rust(monkeypatch: pytest.MonkeyPatch) -> None:
    resolver = MappingClassResolver(
        [
            ResolvedClass("example/Base", JAVA_LANG_OBJECT, (), ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER),
            ResolvedClass(
                "example/Sub",
                "example/Base",
                (),
                ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
                methods=(ResolvedMethod("hook", "()V", MethodAccessFlag.PUBLIC),),
            ),
        ]
    )
    monkeypatch.setattr(hierarchy_module, "_iter_superclasses_python", _unexpected_python_path)
    monkeypatch.setattr(hierarchy_module, "_iter_supertypes_python", _unexpected_python_path)
    monkeypatch.setattr(hierarchy_module, "_is_subtype_python", _unexpected_python_path)
    monkeypatch.setattr(hierarchy_module, "_common_superclass_python", _unexpected_python_path)
    monkeypatch.setattr(hierarchy_module, "_find_overridden_methods_python", _unexpected_python_path)

    assert [resolved.name for resolved in hierarchy_module.iter_superclasses(resolver, "example/Sub")] == [
        "example/Base",
        JAVA_LANG_OBJECT,
    ]
    assert [resolved.name for resolved in hierarchy_module.iter_supertypes(resolver, "example/Sub")] == [
        "example/Base",
        JAVA_LANG_OBJECT,
    ]
    assert hierarchy_module.is_subtype(resolver, "example/Sub", "example/Base")
    assert hierarchy_module.common_superclass(resolver, "example/Sub", "example/Base") == "example/Base"
    assert (
        hierarchy_module.find_overridden_methods(
            resolver,
            "example/Sub",
            ResolvedMethod("hook", "()V", MethodAccessFlag.PUBLIC),
        )
        == ()
    )


def test_hierarchy_wrappers_fall_back_to_python(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[str] = []

    original = hierarchy_module._iter_superclasses_python

    def wrapped_python_path(
        resolver: MappingClassResolver,
        class_name: str,
        *,
        include_self: bool = False,
    ) -> object:
        calls.append(f"{class_name}:{include_self}")
        return original(resolver, class_name, include_self=include_self)

    monkeypatch.setattr(hierarchy_module, "_rust_iter_superclasses", None)
    monkeypatch.setattr(hierarchy_module, "_iter_superclasses_python", wrapped_python_path)

    resolver = MappingClassResolver.from_models(
        [
            ClassModel(
                version=(52, 0),
                access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
                name="example/Base",
                super_name=JAVA_LANG_OBJECT,
                interfaces=[],
                fields=[],
                methods=[MethodModel(MethodAccessFlag.PUBLIC, "hook", "()V", None, [])],
                attributes=[],
            ),
            ClassModel(
                version=(52, 0),
                access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
                name="example/Sub",
                super_name="example/Base",
                interfaces=[],
                fields=[],
                methods=[],
                attributes=[],
            ),
        ]
    )

    names = [resolved.name for resolved in hierarchy_module.iter_superclasses(resolver, "example/Sub")]

    assert calls == ["example/Sub:False"]
    assert names == ["example/Base", JAVA_LANG_OBJECT]
