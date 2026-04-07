from __future__ import annotations

from pathlib import Path

import pytest

from pytecode.analysis.hierarchy import ResolvedClass
from pytecode.analysis.verify import verify_classfile
from pytecode.classfile.attributes import (
    BootstrapMethodsAttr,
    CodeAttr,
    DeprecatedAttr,
    LineNumberTableAttr,
    PermittedSubclassesAttr,
    RecordAttr,
    RuntimeVisibleParameterAnnotationsAttr,
    RuntimeVisibleTypeAnnotationsAttr,
)
from pytecode.classfile.constants import ClassAccessFlag
from pytecode.classfile.reader import ClassReader
from pytecode.classfile.writer import ClassWriter
from pytecode.edit.model import ClassModel
from tests.helpers import compile_java_resource, compile_java_resource_classes

rust = pytest.importorskip("pytecode._rust")


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


def test_class_reader_exposes_constant_pool_and_attributes(tmp_path: Path) -> None:
    hello_world_class = compile_java_resource(tmp_path, "HelloWorld.java")
    class_info = rust.ClassReader.from_file(hello_world_class).class_info

    assert class_info.constant_pool_count == len(class_info.constant_pool)
    assert class_info.constant_pool[0] is None
    assert class_info.access_flags & ClassAccessFlag.SUPER
    assert class_info.this_class > 0
    assert class_info.super_class > 0

    hello_name = next(
        entry
        for entry in class_info.constant_pool
        if isinstance(entry, rust.Utf8Info) and entry.str_bytes == b"HelloWorld"
    )
    assert hello_name.index > 0
    assert hello_name.tag == 1

    source_attr = next(attr for attr in class_info.attributes if isinstance(attr, rust.SourceFileAttr))
    assert source_attr.sourcefile_index > 0

    code_attr = next(
        attr for method in class_info.methods for attr in method.attributes if isinstance(attr, rust.CodeAttr)
    )
    assert code_attr.code_length > 0
    assert code_attr.max_stacks > 0
    assert code_attr.exception_table_length == len(code_attr.exception_table)
    assert code_attr.attributes_count == len(code_attr.attributes)
    assert code_attr.code[0].bytecode_offset == 0
    assert any(insn.opcode == 0xB1 for insn in code_attr.code)


def test_class_model_accepts_rust_classfile(tmp_path: Path) -> None:
    hello_world_class = compile_java_resource(tmp_path, "HelloWorld.java")
    class_bytes = hello_world_class.read_bytes()

    rust_classfile = rust.ClassReader.from_bytes(class_bytes).class_info
    model = ClassModel.from_classfile(rust_classfile)

    assert model.name == "HelloWorld"
    assert any(method.name == "main" for method in model.methods)
    assert model.to_bytes() == class_bytes


def test_analysis_entrypoints_accept_rust_classfile(tmp_path: Path) -> None:
    hello_world_class = compile_java_resource(tmp_path, "HelloWorld.java")
    rust_classfile = rust.ClassReader.from_file(hello_world_class).class_info

    resolved = ResolvedClass.from_classfile(rust_classfile)
    diagnostics = verify_classfile(rust_classfile)

    assert resolved.name == "HelloWorld"
    assert any(method.name == "main" for method in resolved.methods)
    assert not [diag for diag in diagnostics if diag.severity.value == "error"]


def test_public_class_writer_accepts_rust_classfile(tmp_path: Path) -> None:
    hello_world_class = compile_java_resource(tmp_path, "HelloWorld.java")
    class_bytes = hello_world_class.read_bytes()
    rust_classfile = rust.ClassReader.from_bytes(class_bytes).class_info

    assert ClassWriter.write(rust_classfile) == class_bytes


def test_public_reader_surfaces_typed_rust_attributes(tmp_path: Path) -> None:
    annotated = ClassReader.from_file(compile_java_resource(tmp_path, "AnnotatedClass.java")).class_info
    assert any(isinstance(attr, DeprecatedAttr) for attr in annotated.attributes)
    assert any(isinstance(attr, DeprecatedAttr) for field in annotated.fields for attr in field.attributes)
    assert any(isinstance(attr, DeprecatedAttr) for method in annotated.methods for attr in method.attributes)

    parameter_annotations = ClassReader.from_file(
        compile_java_resource(tmp_path, "ParameterAnnotations.java")
    ).class_info
    assert any(
        isinstance(attr, RuntimeVisibleParameterAnnotationsAttr)
        for method in parameter_annotations.methods
        for attr in method.attributes
    )

    type_annotations = ClassReader.from_file(compile_java_resource(tmp_path, "TypeAnnotationShowcase.java")).class_info
    assert any(isinstance(attr, RuntimeVisibleTypeAnnotationsAttr) for attr in type_annotations.attributes)
    assert any(
        isinstance(attr, RuntimeVisibleTypeAnnotationsAttr)
        for field in type_annotations.fields
        for attr in field.attributes
    )
    assert any(
        isinstance(attr, RuntimeVisibleTypeAnnotationsAttr)
        for method in type_annotations.methods
        for attr in method.attributes
    )

    hello = ClassReader.from_file(compile_java_resource(tmp_path, "HelloWorld.java")).class_info
    code_attr = next(attr for method in hello.methods for attr in method.attributes if isinstance(attr, CodeAttr))
    assert any(isinstance(attr, LineNumberTableAttr) for attr in code_attr.attributes)

    lambda_showcase = ClassReader.from_file(compile_java_resource(tmp_path, "LambdaShowcase.java")).class_info
    assert any(isinstance(attr, BootstrapMethodsAttr) for attr in lambda_showcase.attributes)

    record_classes = compile_java_resource_classes(tmp_path, "RecordClass.java", release=16)
    point_class = next(path for path in record_classes if path.name == "RecordClass$Point.class")
    point = ClassReader.from_file(point_class).class_info
    assert any(isinstance(attr, RecordAttr) for attr in point.attributes)

    sealed_classes = compile_java_resource_classes(tmp_path, "SealedHierarchy.java", release=17)
    shape_class = next(path for path in sealed_classes if path.name == "SealedHierarchy$Shape.class")
    shape = ClassReader.from_file(shape_class).class_info
    assert any(isinstance(attr, PermittedSubclassesAttr) for attr in shape.attributes)
