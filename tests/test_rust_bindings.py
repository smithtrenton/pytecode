from __future__ import annotations

from pathlib import Path

import pytest

import pytecode
import pytecode.analysis as analysis
import pytecode.classfile as classfile_api
import pytecode.model as model_api
from pytecode.analysis.hierarchy import (
    MappingClassResolver as HierarchyMappingClassResolver,
)
from pytecode.analysis.hierarchy import (
    ResolvedClass,
    common_superclass,
    is_subtype,
    iter_superclasses,
    iter_supertypes,
)
from pytecode.archive import FrameComputationMode
from tests.helpers import compile_java_resource, compile_java_resource_classes

rust = pytest.importorskip("pytecode._rust")


def test_top_level_rust_first_exports() -> None:
    assert pytecode.ClassReader is rust.ClassReader
    assert pytecode.ClassWriter is rust.ClassWriter
    assert pytecode.ClassModel is rust.ClassModel
    assert not hasattr(pytecode, "MappingClassResolver")
    assert not hasattr(pytecode, "Diagnostic")
    assert not hasattr(pytecode, "verify_classfile")
    assert not hasattr(pytecode, "verify_classmodel")
    assert not hasattr(pytecode, "RustClassReader")
    assert not hasattr(pytecode, "RustClassWriter")
    assert not hasattr(pytecode, "RustClassModel")
    assert not hasattr(pytecode, "LegacyClassReader")
    assert not hasattr(pytecode, "LegacyClassWriter")
    assert not hasattr(pytecode, "LegacyClassModel")


def test_analysis_package_rust_first_exports() -> None:
    assert analysis.MappingClassResolver is rust.MappingClassResolver
    assert analysis.Diagnostic is rust.Diagnostic
    assert callable(analysis.verify_classfile)
    assert callable(analysis.verify_classmodel)


def test_semantic_modules_reexport_rust_types() -> None:
    assert classfile_api.ArrayType.__module__ == "pytecode.classfile.bytecode"
    assert classfile_api.ClassFile is rust.ClassFile
    assert classfile_api.ClassReader is rust.ClassReader
    assert classfile_api.ClassWriter is rust.ClassWriter
    assert classfile_api.ExceptionInfo is rust.ExceptionInfo
    assert classfile_api.InsnInfo is rust.InsnInfo
    assert classfile_api.InsnInfoType.__module__ == "pytecode.classfile.bytecode"
    assert classfile_api.MatchOffsetPair is rust.MatchOffsetPair
    assert model_api.ClassModel is rust.ClassModel
    assert model_api.CodeModel is rust.CodeModel
    assert model_api.Label is rust.Label
    assert model_api.RawInsn is rust.RawInsn


def test_backend_info_surface() -> None:
    module_name, version, exports = rust.backend_info()
    assert module_name == "pytecode._rust"
    assert version
    assert "ClassReader" in exports
    assert "ClassWriter" in exports


def test_class_reader_roundtrip_smoke(tmp_path: Path) -> None:
    hello_world_class = compile_java_resource(tmp_path, "HelloWorld.java")
    class_bytes = hello_world_class.read_bytes()

    reader = rust.ClassReader.from_bytes(class_bytes)
    class_info = reader.class_info

    assert class_info.magic == 0xCAFEBABE
    assert class_info.major_version >= 52
    assert class_info.method_count > 0
    assert class_info.to_bytes() == class_bytes
    assert rust.ClassWriter.write(class_info) == class_bytes


def test_top_level_reader_writer_are_rust_backed(tmp_path: Path) -> None:
    hello_world_class = compile_java_resource(tmp_path, "HelloWorld.java")
    class_bytes = hello_world_class.read_bytes()

    reader = pytecode.ClassReader.from_bytes(class_bytes)

    assert type(reader) is rust.ClassReader
    assert pytecode.ClassWriter.write(reader.class_info) == class_bytes


def test_top_level_classmodel_roundtrip_smoke(tmp_path: Path) -> None:
    hello_world_class = compile_java_resource(tmp_path, "HelloWorld.java")
    class_bytes = hello_world_class.read_bytes()

    model = pytecode.ClassModel.from_bytes(class_bytes)
    model.access_flags |= 0x0010
    rewritten = model.to_bytes()
    lowered = model.to_classfile()

    class_info = pytecode.ClassReader.from_bytes(rewritten).class_info
    assert class_info.access_flags & 0x0010
    assert lowered.to_bytes() == rewritten


def test_model_lowering_matches_individual_lowering(tmp_path: Path) -> None:
    class_paths = compile_java_resource_classes(tmp_path, "HierarchyFixture.java")
    original_bytes = [path.read_bytes() for path in class_paths]
    models = [pytecode.ClassModel.from_bytes(class_bytes) for class_bytes in original_bytes]

    for model in models:
        model.access_flags |= 0x0010

    individual = [model.to_bytes() for model in models]
    lowered_classfiles = [model.to_classfile() for model in models]

    assert [classfile.to_bytes() for classfile in lowered_classfiles] == individual
    assert all(
        pytecode.ClassReader.from_bytes(class_bytes).class_info.access_flags & 0x0010 for class_bytes in individual
    )


def test_classmodel_option_helpers_accept_frame_mode_enum(tmp_path: Path) -> None:
    class_paths = compile_java_resource_classes(tmp_path, "HierarchyFixture.java")
    resolver = analysis.MappingClassResolver.from_bytes([path.read_bytes() for path in class_paths])
    model = pytecode.ClassModel.from_bytes(class_paths[0].read_bytes())

    rewritten = model.to_bytes_with_options(
        frame_mode=FrameComputationMode.RECOMPUTE,
        resolver=resolver,
    )
    lowered = model.to_classfile_with_options(
        frame_mode=FrameComputationMode.RECOMPUTE,
        resolver=resolver,
    )

    assert lowered.to_bytes() == rewritten


def test_analysis_entrypoints_accept_rust_classfile(tmp_path: Path) -> None:
    hello_world_class = compile_java_resource(tmp_path, "HelloWorld.java")
    rust_classfile = rust.ClassReader.from_file(hello_world_class).class_info

    resolved = ResolvedClass.from_classfile(rust_classfile)
    diagnostics = analysis.verify_classfile(rust_classfile)

    assert resolved.name == "HelloWorld"
    assert any(method.name == "main" for method in resolved.methods)
    assert not [diag for diag in diagnostics if diag.severity == "error"]


def test_top_level_verify_wrappers_accept_rust_inputs(tmp_path: Path) -> None:
    hello_world_class = compile_java_resource(tmp_path, "HelloWorld.java")
    class_bytes = hello_world_class.read_bytes()
    rust_classfile = rust.ClassReader.from_bytes(class_bytes).class_info
    rust_model = rust.ClassModel.from_bytes(class_bytes)

    class_diags = analysis.verify_classfile(rust_classfile)
    bytes_diags = analysis.verify_classfile(class_bytes)
    model_diags = analysis.verify_classmodel(rust_model)

    assert not [diag for diag in class_diags if diag.severity == "error"]
    assert not [diag for diag in bytes_diags if diag.severity == "error"]
    assert not [diag for diag in model_diags if diag.severity == "error"]


def test_hierarchy_helpers_accept_rust_resolver(tmp_path: Path) -> None:
    class_paths = compile_java_resource_classes(tmp_path, "HierarchyFixture.java")
    resolver = analysis.MappingClassResolver.from_bytes([path.read_bytes() for path in class_paths])

    fixture_name = "fixture/hierarchy/HierarchyFixture"
    mammal_name = "fixture/hierarchy/Mammal"
    animal_name = "fixture/hierarchy/Animal"
    pet_name = "fixture/hierarchy/Pet"
    trainable_name = "fixture/hierarchy/Trainable"

    resolved = resolver.resolve_class(fixture_name)
    assert resolved is not None
    assert resolved["super_name"] == mammal_name
    assert any(method["name"] == "train" for method in resolved["methods"])

    assert is_subtype(resolver, fixture_name, animal_name)
    assert is_subtype(resolver, fixture_name, pet_name)
    assert common_superclass(resolver, fixture_name, mammal_name) == mammal_name
    assert [entry.name for entry in iter_superclasses(resolver, fixture_name)][:2] == [
        mammal_name,
        animal_name,
    ]

    names = [entry.name for entry in iter_supertypes(resolver, fixture_name)]
    assert pet_name in names
    assert trainable_name in names


def test_hierarchy_factory_from_rust_models_uses_rust_resolver(tmp_path: Path) -> None:
    class_paths = compile_java_resource_classes(tmp_path, "HierarchyFixture.java")
    models = [rust.ClassModel.from_bytes(path.read_bytes()) for path in class_paths]

    resolver = HierarchyMappingClassResolver.from_models(models)

    assert isinstance(resolver, rust.MappingClassResolver)
    resolved = resolver.resolve_class("fixture/hierarchy/HierarchyFixture")
    assert resolved is not None
    assert resolved["super_name"] == "fixture/hierarchy/Mammal"


def test_public_reader_surfaces_typed_rust_attributes(tmp_path: Path) -> None:
    annotated = pytecode.ClassReader.from_file(compile_java_resource(tmp_path, "AnnotatedClass.java")).class_info
    assert any(type(attr).__name__ == "DeprecatedAttr" for attr in annotated.attributes)
    assert any(type(attr).__name__ == "DeprecatedAttr" for field in annotated.fields for attr in field.attributes)
    assert any(type(attr).__name__ == "DeprecatedAttr" for method in annotated.methods for attr in method.attributes)

    parameter_annotations = pytecode.ClassReader.from_file(
        compile_java_resource(tmp_path, "ParameterAnnotations.java")
    ).class_info
    assert any(
        type(attr).__name__ == "RuntimeVisibleParameterAnnotationsAttr"
        for method in parameter_annotations.methods
        for attr in method.attributes
    )

    type_annotations = pytecode.ClassReader.from_file(
        compile_java_resource(tmp_path, "TypeAnnotationShowcase.java")
    ).class_info
    assert any(type(attr).__name__ == "RuntimeVisibleTypeAnnotationsAttr" for attr in type_annotations.attributes)
    assert any(
        type(attr).__name__ == "RuntimeVisibleTypeAnnotationsAttr"
        for field in type_annotations.fields
        for attr in field.attributes
    )
    assert any(
        type(attr).__name__ == "RuntimeVisibleTypeAnnotationsAttr"
        for method in type_annotations.methods
        for attr in method.attributes
    )

    hello = pytecode.ClassReader.from_file(compile_java_resource(tmp_path, "HelloWorld.java")).class_info
    code_attr = next(
        attr for method in hello.methods for attr in method.attributes if type(attr).__name__ == "CodeAttr"
    )
    assert any(type(attr).__name__ == "LineNumberTableAttr" for attr in code_attr.attributes)

    lambda_showcase = pytecode.ClassReader.from_file(compile_java_resource(tmp_path, "LambdaShowcase.java")).class_info
    assert any(type(attr).__name__ == "BootstrapMethodsAttr" for attr in lambda_showcase.attributes)

    record_classes = compile_java_resource_classes(tmp_path, "RecordClass.java", release=16)
    point_class = next(path for path in record_classes if path.name == "RecordClass$Point.class")
    point = pytecode.ClassReader.from_file(point_class).class_info
    assert any(type(attr).__name__ == "RecordAttr" for attr in point.attributes)

    sealed_classes = compile_java_resource_classes(tmp_path, "SealedHierarchy.java", release=17)
    shape_class = next(path for path in sealed_classes if path.name == "SealedHierarchy$Shape.class")
    shape = pytecode.ClassReader.from_file(shape_class).class_info
    assert any(type(attr).__name__ == "PermittedSubclassesAttr" for attr in shape.attributes)


def test_public_reader_surfaces_bytecode_enums_from_classfile_api(tmp_path: Path) -> None:
    cfg = pytecode.ClassReader.from_file(compile_java_resource(tmp_path, "CfgFixture.java")).class_info
    newarray = next(
        insn
        for method in cfg.methods
        for attr in method.attributes
        if type(attr).__name__ == "CodeAttr"
        for insn in attr.code
        if insn.atype is not None
    )

    assert isinstance(newarray, classfile_api.InsnInfo)
    assert newarray.type is classfile_api.InsnInfoType.NEWARRAY
    assert newarray.atype is classfile_api.ArrayType.INT


def test_public_reader_surfaces_exception_table_entries_from_classfile_api(tmp_path: Path) -> None:
    example = pytecode.ClassReader.from_file(compile_java_resource(tmp_path, "TryCatchExample.java")).class_info
    code_attr = next(
        attr
        for method in example.methods
        for attr in method.attributes
        if type(attr).__name__ == "CodeAttr" and attr.exception_table
    )

    assert isinstance(code_attr.exception_table[0], classfile_api.ExceptionInfo)

# --- Error-case tests ---


def test_class_reader_rejects_truncated_bytes() -> None:
    with pytest.raises(Exception):
        rust.ClassReader.from_bytes(b"\xca\xfe\xba\xbe\x00")


def test_class_reader_rejects_invalid_magic() -> None:
    with pytest.raises(Exception):
        rust.ClassReader.from_bytes(b"\xde\xad\xbe\xef" + b"\x00" * 20)


def test_class_reader_rejects_empty_input() -> None:
    with pytest.raises(Exception):
        rust.ClassReader.from_bytes(b"")


def test_classmodel_rejects_invalid_bytes() -> None:
    with pytest.raises(Exception):
        pytecode.ClassModel.from_bytes(b"not a classfile")


def test_verify_classfile_rejects_wrong_type() -> None:
    with pytest.raises(TypeError):
        analysis.verify_classfile(12345)  # type: ignore[arg-type]
