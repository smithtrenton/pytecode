"""Tests for pytecode.edit.model — the mutable editing model."""

from __future__ import annotations

from pathlib import Path

import pytest

import pytecode.classfile.constant_pool as cp_module
from pytecode.classfile.attributes import (
    CodeAttr,
    InnerClassesAttr,
    RuntimeVisibleAnnotationsAttr,
    SignatureAttr,
    StackMapTableAttr,
    SyntheticAttr,
)
from pytecode.classfile.constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from pytecode.classfile.info import ClassFile, FieldInfo
from pytecode.classfile.instructions import InsnInfo, InsnInfoType
from pytecode.classfile.modified_utf8 import decode_modified_utf8
from pytecode.edit.constant_pool_builder import ConstantPoolBuilder
from pytecode.edit.labels import BranchInsn, ExceptionHandler, Label, LookupSwitchInsn, TableSwitchInsn
from pytecode.edit.model import ClassModel, CodeModel, FieldModel, MethodModel
from pytecode.edit.operands import MethodInsn, TypeInsn
from tests.helpers import (
    cached_java_resource_classes,
    class_entry_bytes,
    compile_java_resource,
    find_method_in_classfile,
    find_method_in_model,
    integer_entry_bytes,
    list_java_resources,
    minimal_classfile,
    utf8_entry_bytes,
)

ROUNDTRIP_JAVA_RESOURCES = list_java_resources()

# ---------------------------------------------------------------------------
# Pytest fixtures — compiled Java sources
# ---------------------------------------------------------------------------


@pytest.fixture
def hello_world_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "HelloWorld.java")


@pytest.fixture
def interface_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "SimpleInterface.java")


@pytest.fixture
def abstract_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "AbstractShape.java")


@pytest.fixture
def enum_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "Color.java")


@pytest.fixture
def multi_iface_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "MultiInterface.java")


@pytest.fixture
def field_showcase_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "FieldShowcase.java")


@pytest.fixture
def try_catch_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "TryCatchExample.java")


@pytest.fixture
def annotated_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "AnnotatedClass.java")


@pytest.fixture
def static_init_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "StaticInit.java")


@pytest.fixture
def generic_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "GenericClass.java")


@pytest.fixture
def outer_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "Outer.java")


@pytest.fixture
def control_flow_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "ControlFlowExample.java")


@pytest.fixture
def instruction_showcase_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "InstructionShowcase.java")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _read_class(path: Path) -> ClassFile:
    from pytecode.classfile.reader import ClassReader

    return ClassReader(path.read_bytes()).class_info


def _resolve_class_name(cf: ClassFile, cp_index: int) -> str:
    entry = cf.constant_pool[cp_index]
    assert isinstance(entry, cp_module.ClassInfo)
    utf8 = cf.constant_pool[entry.name_index]
    assert isinstance(utf8, cp_module.Utf8Info)
    return decode_modified_utf8(utf8.str_bytes)


def _resolve_utf8(cf: ClassFile, cp_index: int) -> str:
    entry = cf.constant_pool[cp_index]
    assert isinstance(entry, cp_module.Utf8Info)
    return decode_modified_utf8(entry.str_bytes)


def _find_field(model: ClassModel, name: str) -> FieldModel:
    for f in model.fields:
        if f.name == name:
            return f
    raise AssertionError(f"Field {name!r} not found in ClassModel")


def _find_field_info(cf: ClassFile, name: str) -> FieldInfo:
    for field in cf.fields:
        if _resolve_utf8(cf, field.name_index) == name:
            return field
    raise AssertionError(f"Field {name!r} not found in ClassFile")


def _find_method_with_stack_map(cf: ClassFile) -> tuple[str, StackMapTableAttr]:
    for method in cf.methods:
        code_attr = next((attr for attr in method.attributes if isinstance(attr, CodeAttr)), None)
        if code_attr is None:
            continue
        stack_map = next(
            (attr for attr in code_attr.attributes if isinstance(attr, StackMapTableAttr) and attr.entries),
            None,
        )
        if stack_map is not None:
            return _resolve_utf8(cf, method.name_index), stack_map
    raise AssertionError("Method with non-empty StackMapTable not found in ClassFile")


# ===========================================================================
# Unit tests — from-scratch creation
# ===========================================================================


class TestFromScratchCreation:
    """Test creating ClassModel/MethodModel/FieldModel without a ClassFile."""

    def test_create_empty_class(self) -> None:
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/Empty",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[],
            attributes=[],
        )
        assert cls.name == "com/example/Empty"
        assert cls.super_name == "java/lang/Object"
        assert cls.version == (52, 0)
        assert cls.access_flags == ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER
        assert cls.interfaces == []
        assert cls.fields == []
        assert cls.methods == []

    def test_create_class_with_fields(self) -> None:
        field = FieldModel(
            access_flags=FieldAccessFlag.PRIVATE,
            name="count",
            descriptor="I",
            attributes=[],
        )
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/Counter",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[field],
            methods=[],
            attributes=[],
        )
        assert len(cls.fields) == 1
        assert cls.fields[0].name == "count"
        assert cls.fields[0].descriptor == "I"
        assert cls.fields[0].access_flags == FieldAccessFlag.PRIVATE

    def test_create_class_with_methods(self) -> None:
        method = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
            name="doSomething",
            descriptor="()V",
            code=None,
            attributes=[],
        )
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT,
            name="com/example/Base",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[method],
            attributes=[],
        )
        assert len(cls.methods) == 1
        assert cls.methods[0].name == "doSomething"
        assert cls.methods[0].code is None

    def test_create_class_with_interfaces(self) -> None:
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/Impl",
            super_name="java/lang/Object",
            interfaces=["java/io/Serializable", "java/lang/Comparable"],
            fields=[],
            methods=[],
            attributes=[],
        )
        assert cls.interfaces == ["java/io/Serializable", "java/lang/Comparable"]

    def test_create_class_null_super(self) -> None:
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="java/lang/Object",
            super_name=None,
            interfaces=[],
            fields=[],
            methods=[],
            attributes=[],
        )
        assert cls.super_name is None

    def test_default_constant_pool_is_empty_builder(self) -> None:
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[],
            attributes=[],
        )
        assert isinstance(cls.constant_pool, ConstantPoolBuilder)

    def test_create_code_model(self) -> None:
        code = CodeModel(
            max_stack=2,
            max_locals=1,
            instructions=[],
            exception_handlers=[],
            attributes=[],
        )
        assert code.max_stack == 2
        assert code.max_locals == 1
        assert code.instructions == []
        assert code.exception_handlers == []

    def test_fields_and_methods_are_mutable(self) -> None:
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/Mutable",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[],
            attributes=[],
        )
        field = FieldModel(
            access_flags=FieldAccessFlag.PUBLIC,
            name="x",
            descriptor="I",
            attributes=[],
        )
        cls.fields.append(field)
        assert len(cls.fields) == 1

        cls.name = "com/example/Renamed"
        assert cls.name == "com/example/Renamed"


# ===========================================================================
# Unit tests — from_classfile() resolution
# ===========================================================================


class TestFromClassFile:
    """Test converting a parsed ClassFile to ClassModel."""

    def test_from_minimal_classfile(self) -> None:
        raw = minimal_classfile()
        model = ClassModel.from_bytes(raw)
        assert model.name == "TestClass"
        assert model.super_name == "java/lang/Object"
        assert model.version == (52, 0)
        assert model.interfaces == []
        assert model.fields == []
        assert model.methods == []

    def test_hello_world_class_name(self, hello_world_class: Path) -> None:
        cf = _read_class(hello_world_class)
        model = ClassModel.from_classfile(cf)
        assert model.name == "HelloWorld"

    def test_hello_world_super_name(self, hello_world_class: Path) -> None:
        cf = _read_class(hello_world_class)
        model = ClassModel.from_classfile(cf)
        assert model.super_name == "java/lang/Object"

    def test_hello_world_access_flags(self, hello_world_class: Path) -> None:
        cf = _read_class(hello_world_class)
        model = ClassModel.from_classfile(cf)
        assert ClassAccessFlag.PUBLIC in model.access_flags
        assert ClassAccessFlag.SUPER in model.access_flags

    def test_hello_world_methods(self, hello_world_class: Path) -> None:
        cf = _read_class(hello_world_class)
        model = ClassModel.from_classfile(cf)
        method_names = {m.name for m in model.methods}
        assert "main" in method_names
        assert "giveItToMe" in method_names
        assert "<init>" in method_names

    def test_hello_world_main_descriptor(self, hello_world_class: Path) -> None:
        cf = _read_class(hello_world_class)
        model = ClassModel.from_classfile(cf)
        main = find_method_in_model(model, "main")
        assert main.descriptor == "([Ljava/lang/String;)V"

    def test_hello_world_main_has_code(self, hello_world_class: Path) -> None:
        cf = _read_class(hello_world_class)
        model = ClassModel.from_classfile(cf)
        main = find_method_in_model(model, "main")
        assert main.code is not None
        assert len(main.code.instructions) > 0

    def test_hello_world_main_max_values(self, hello_world_class: Path) -> None:
        cf = _read_class(hello_world_class)
        model = ClassModel.from_classfile(cf)
        main = find_method_in_model(model, "main")
        assert main.code is not None
        assert main.code.max_stack >= 1
        assert main.code.max_locals >= 1

    def test_from_bytes_convenience(self, hello_world_class: Path) -> None:
        raw = hello_world_class.read_bytes()
        model = ClassModel.from_bytes(raw)
        assert model.name == "HelloWorld"

    def test_constant_pool_seeded(self, hello_world_class: Path) -> None:
        cf = _read_class(hello_world_class)
        model = ClassModel.from_classfile(cf)
        assert isinstance(model.constant_pool, ConstantPoolBuilder)
        assert len(model.constant_pool) > 0

    def test_code_attributes_separated(self, hello_world_class: Path) -> None:
        """Code attribute should be lifted into CodeModel, not in method attrs."""
        cf = _read_class(hello_world_class)
        model = ClassModel.from_classfile(cf)
        for method in model.methods:
            for attr in method.attributes:
                assert not isinstance(attr, CodeAttr), (
                    f"CodeAttr should not appear in MethodModel.attributes for {method.name}"
                )


# ===========================================================================
# Unit tests — from_classfile() with diverse fixtures
# ===========================================================================


class TestInterfaceModel:
    def test_interface_flags(self, interface_class: Path) -> None:
        model = ClassModel.from_bytes(interface_class.read_bytes())
        assert ClassAccessFlag.INTERFACE in model.access_flags
        assert ClassAccessFlag.ABSTRACT in model.access_flags

    def test_interface_methods(self, interface_class: Path) -> None:
        model = ClassModel.from_bytes(interface_class.read_bytes())
        method_names = {m.name for m in model.methods}
        assert "abstractMethod" in method_names
        assert "anotherAbstract" in method_names
        assert "defaultMethod" in method_names

    def test_abstract_methods_have_no_code(self, interface_class: Path) -> None:
        model = ClassModel.from_bytes(interface_class.read_bytes())
        abstract_m = find_method_in_model(model, "abstractMethod")
        assert abstract_m.code is None

    def test_default_method_has_code(self, interface_class: Path) -> None:
        model = ClassModel.from_bytes(interface_class.read_bytes())
        default_m = find_method_in_model(model, "defaultMethod")
        assert default_m.code is not None

    def test_interface_constant_field(self, interface_class: Path) -> None:
        model = ClassModel.from_bytes(interface_class.read_bytes())
        f = _find_field(model, "CONSTANT_VALUE")
        assert FieldAccessFlag.PUBLIC in f.access_flags
        assert FieldAccessFlag.STATIC in f.access_flags
        assert FieldAccessFlag.FINAL in f.access_flags
        assert f.descriptor == "I"


class TestAbstractClassModel:
    def test_abstract_class_flags(self, abstract_class: Path) -> None:
        model = ClassModel.from_bytes(abstract_class.read_bytes())
        assert ClassAccessFlag.ABSTRACT in model.access_flags

    def test_abstract_methods_no_code(self, abstract_class: Path) -> None:
        model = ClassModel.from_bytes(abstract_class.read_bytes())
        compute_area = find_method_in_model(model, "computeArea")
        assert compute_area.code is None
        assert MethodAccessFlag.ABSTRACT in compute_area.access_flags

    def test_concrete_methods_have_code(self, abstract_class: Path) -> None:
        model = ClassModel.from_bytes(abstract_class.read_bytes())
        get_name = find_method_in_model(model, "getName")
        assert get_name.code is not None

    def test_fields_resolved(self, abstract_class: Path) -> None:
        model = ClassModel.from_bytes(abstract_class.read_bytes())
        name_field = _find_field(model, "name")
        assert FieldAccessFlag.PRIVATE in name_field.access_flags
        assert FieldAccessFlag.FINAL in name_field.access_flags
        assert name_field.descriptor == "Ljava/lang/String;"


class TestEnumModel:
    def test_enum_flags(self, enum_class: Path) -> None:
        model = ClassModel.from_bytes(enum_class.read_bytes())
        assert model.name == "Color"
        assert ClassAccessFlag.ENUM in model.access_flags
        assert ClassAccessFlag.FINAL in model.access_flags

    def test_enum_super(self, enum_class: Path) -> None:
        model = ClassModel.from_bytes(enum_class.read_bytes())
        assert model.super_name == "java/lang/Enum"

    def test_enum_has_values_method(self, enum_class: Path) -> None:
        model = ClassModel.from_bytes(enum_class.read_bytes())
        method_names = {m.name for m in model.methods}
        assert "values" in method_names
        assert "valueOf" in method_names

    def test_enum_user_method(self, enum_class: Path) -> None:
        model = ClassModel.from_bytes(enum_class.read_bytes())
        lower = find_method_in_model(model, "lower")
        assert lower.code is not None
        assert lower.descriptor == "()Ljava/lang/String;"

    def test_enum_constant_fields(self, enum_class: Path) -> None:
        model = ClassModel.from_bytes(enum_class.read_bytes())
        for color_name in ("RED", "GREEN", "BLUE"):
            f = _find_field(model, color_name)
            assert FieldAccessFlag.ENUM in f.access_flags
            assert FieldAccessFlag.STATIC in f.access_flags
            assert f.descriptor == "LColor;"

    def test_enum_has_clinit(self, enum_class: Path) -> None:
        model = ClassModel.from_bytes(enum_class.read_bytes())
        clinit = find_method_in_model(model, "<clinit>")
        assert clinit.code is not None


class TestMultiInterfaceModel:
    def test_multiple_interfaces(self, multi_iface_class: Path) -> None:
        model = ClassModel.from_bytes(multi_iface_class.read_bytes())
        assert len(model.interfaces) == 2
        assert "java/io/Serializable" in model.interfaces
        assert "java/lang/Comparable" in model.interfaces


class TestFieldShowcaseModel:
    def test_field_count(self, field_showcase_class: Path) -> None:
        model = ClassModel.from_bytes(field_showcase_class.read_bytes())
        assert len(model.fields) == 9

    def test_public_int(self, field_showcase_class: Path) -> None:
        model = ClassModel.from_bytes(field_showcase_class.read_bytes())
        f = _find_field(model, "publicInt")
        assert FieldAccessFlag.PUBLIC in f.access_flags
        assert f.descriptor == "I"

    def test_private_string(self, field_showcase_class: Path) -> None:
        model = ClassModel.from_bytes(field_showcase_class.read_bytes())
        f = _find_field(model, "privateName")
        assert FieldAccessFlag.PRIVATE in f.access_flags
        assert f.descriptor == "Ljava/lang/String;"

    def test_static_field(self, field_showcase_class: Path) -> None:
        model = ClassModel.from_bytes(field_showcase_class.read_bytes())
        f = _find_field(model, "staticCounter")
        assert FieldAccessFlag.STATIC in f.access_flags
        assert f.descriptor == "J"

    def test_constant_field(self, field_showcase_class: Path) -> None:
        model = ClassModel.from_bytes(field_showcase_class.read_bytes())
        f = _find_field(model, "CONSTANT")
        assert FieldAccessFlag.STATIC in f.access_flags
        assert FieldAccessFlag.FINAL in f.access_flags
        assert f.descriptor == "I"

    def test_volatile_field(self, field_showcase_class: Path) -> None:
        model = ClassModel.from_bytes(field_showcase_class.read_bytes())
        f = _find_field(model, "running")
        assert FieldAccessFlag.VOLATILE in f.access_flags
        assert f.descriptor == "Z"

    def test_transient_field(self, field_showcase_class: Path) -> None:
        model = ClassModel.from_bytes(field_showcase_class.read_bytes())
        f = _find_field(model, "temp")
        assert FieldAccessFlag.TRANSIENT in f.access_flags

    def test_array_descriptors(self, field_showcase_class: Path) -> None:
        model = ClassModel.from_bytes(field_showcase_class.read_bytes())
        int_arr = _find_field(model, "intArray")
        assert int_arr.descriptor == "[I"
        matrix = _find_field(model, "matrix")
        assert matrix.descriptor == "[[Ljava/lang/String;"


class TestTryCatchModel:
    def test_exception_handlers_present(self, try_catch_class: Path) -> None:
        model = ClassModel.from_bytes(try_catch_class.read_bytes())
        safe_div = find_method_in_model(model, "safeDivide")
        assert safe_div.code is not None
        assert len(safe_div.code.exception_handlers) > 0

    def test_multi_catch_handlers(self, try_catch_class: Path) -> None:
        model = ClassModel.from_bytes(try_catch_class.read_bytes())
        multi = find_method_in_model(model, "multiCatch")
        assert multi.code is not None
        assert len(multi.code.exception_handlers) >= 2

    def test_exception_handlers_are_symbolic(self, try_catch_class: Path) -> None:
        model = ClassModel.from_bytes(try_catch_class.read_bytes())
        safe_div = find_method_in_model(model, "safeDivide")
        assert safe_div.code is not None
        for ex in safe_div.code.exception_handlers:
            assert isinstance(ex, ExceptionHandler)
            assert isinstance(ex.start, Label)
            assert isinstance(ex.end, Label)
            assert isinstance(ex.handler, Label)
            assert ex.catch_type is None or isinstance(ex.catch_type, str)


class TestControlFlowModel:
    def test_branch_instructions_are_symbolic(self, control_flow_class: Path) -> None:
        model = ClassModel.from_bytes(control_flow_class.read_bytes())
        loop_sum = find_method_in_model(model, "loopSum")
        assert loop_sum.code is not None
        assert any(isinstance(item, BranchInsn) for item in loop_sum.code.instructions)
        assert any(isinstance(item, Label) for item in loop_sum.code.instructions)

    def test_switch_instructions_are_symbolic(self, control_flow_class: Path) -> None:
        model = ClassModel.from_bytes(control_flow_class.read_bytes())
        dense_switch = find_method_in_model(model, "denseSwitch")
        sparse_switch = find_method_in_model(model, "sparseSwitch")
        assert dense_switch.code is not None
        assert sparse_switch.code is not None
        assert any(isinstance(item, TableSwitchInsn) for item in dense_switch.code.instructions)
        assert any(isinstance(item, LookupSwitchInsn) for item in sparse_switch.code.instructions)

    def test_line_numbers_are_lifted(self, control_flow_class: Path) -> None:
        model = ClassModel.from_bytes(control_flow_class.read_bytes())
        branch = find_method_in_model(model, "branch")
        assert branch.code is not None
        assert len(branch.code.line_numbers) > 0
        assert all(isinstance(entry.label, Label) for entry in branch.code.line_numbers)

    def test_raw_instruction_items_are_independent_from_source(self, control_flow_class: Path) -> None:
        cf = _read_class(control_flow_class)
        source_method = find_method_in_classfile(cf, "branch")
        source_code = next(attr for attr in source_method.attributes if isinstance(attr, CodeAttr))
        source_raw_type_names = {"InsnInfo", "ByteValue", "ShortValue"}
        source_raw: InsnInfo | None = None
        source_raw_type_name: str | None = None
        for source_item in source_code.code:
            source_item_type_name = type(source_item).__name__
            if source_item_type_name in source_raw_type_names:
                source_raw = source_item
                source_raw_type_name = source_item_type_name
                break
        assert source_raw is not None
        assert source_raw_type_name is not None

        model = ClassModel.from_classfile(cf)
        method = find_method_in_model(model, "branch")
        assert method.code is not None
        model_raw: InsnInfo | None = None
        for model_item in method.code.instructions:
            model_item_type_name = type(model_item).__name__
            if model_item_type_name == source_raw_type_name:
                assert isinstance(model_item, InsnInfo)
                model_raw = model_item
                break
        assert model_raw is not None
        assert isinstance(model_raw, InsnInfo)

        assert model_raw is not source_raw
        original_offset = source_raw.bytecode_offset
        model_raw.bytecode_offset += 1
        assert source_raw.bytecode_offset == original_offset


class TestAnnotatedClassModel:
    def test_class_has_attributes(self, annotated_class: Path) -> None:
        model = ClassModel.from_bytes(annotated_class.read_bytes())
        assert model.name == "AnnotatedClass"
        assert len(model.attributes) > 0

    def test_deprecated_field_has_attributes(self, annotated_class: Path) -> None:
        model = ClassModel.from_bytes(annotated_class.read_bytes())
        f = _find_field(model, "oldField")
        assert len(f.attributes) > 0

    def test_deprecated_method_has_attributes(self, annotated_class: Path) -> None:
        model = ClassModel.from_bytes(annotated_class.read_bytes())
        m = find_method_in_model(model, "oldMethod")
        assert len(m.attributes) > 0


class TestStaticInitModel:
    def test_has_clinit(self, static_init_class: Path) -> None:
        model = ClassModel.from_bytes(static_init_class.read_bytes())
        clinit = find_method_in_model(model, "<clinit>")
        assert clinit.code is not None
        assert MethodAccessFlag.STATIC in clinit.access_flags

    def test_has_init(self, static_init_class: Path) -> None:
        model = ClassModel.from_bytes(static_init_class.read_bytes())
        init = find_method_in_model(model, "<init>")
        assert init.code is not None


# ===========================================================================
# Round-trip tests — ClassFile → ClassModel → ClassFile → verify
# ===========================================================================


class TestRoundTrip:
    """Verify that ClassFile → ClassModel → to_classfile() preserves structure."""

    def _assert_roundtrip(self, path: Path) -> None:
        """Parse a class, convert to model, lower back, and verify."""
        original = _read_class(path)
        model = ClassModel.from_classfile(original)
        restored = model.to_classfile()

        # Class-level metadata.
        assert restored.magic == original.magic
        assert restored.major_version == original.major_version
        assert restored.minor_version == original.minor_version

        # Resolved names should match original.
        assert _resolve_class_name(restored, restored.this_class) == _resolve_class_name(original, original.this_class)

        if original.super_class != 0:
            assert _resolve_class_name(restored, restored.super_class) == _resolve_class_name(
                original, original.super_class
            )
        else:
            assert restored.super_class == 0

        # Access flags.
        assert restored.access_flags == original.access_flags

        # Interfaces.
        assert len(restored.interfaces) == len(original.interfaces)
        orig_ifaces = [_resolve_class_name(original, i) for i in original.interfaces]
        rest_ifaces = [_resolve_class_name(restored, i) for i in restored.interfaces]
        assert rest_ifaces == orig_ifaces

        # Fields.
        assert len(restored.fields) == len(original.fields)
        for orig_f, rest_f in zip(original.fields, restored.fields, strict=True):
            assert rest_f.access_flags == orig_f.access_flags
            assert _resolve_utf8(restored, rest_f.name_index) == _resolve_utf8(original, orig_f.name_index)
            assert _resolve_utf8(restored, rest_f.descriptor_index) == _resolve_utf8(original, orig_f.descriptor_index)
            assert rest_f.attributes_count == orig_f.attributes_count
            assert rest_f.attributes == orig_f.attributes

        # Methods.
        assert len(restored.methods) == len(original.methods)
        for orig_m, rest_m in zip(original.methods, restored.methods, strict=True):
            assert rest_m.access_flags == orig_m.access_flags
            assert _resolve_utf8(restored, rest_m.name_index) == _resolve_utf8(original, orig_m.name_index)
            assert _resolve_utf8(restored, rest_m.descriptor_index) == _resolve_utf8(original, orig_m.descriptor_index)

            # Code attribute presence.
            orig_code = next((a for a in orig_m.attributes if isinstance(a, CodeAttr)), None)
            rest_code = next((a for a in rest_m.attributes if isinstance(a, CodeAttr)), None)
            if orig_code is None:
                assert rest_code is None
            else:
                assert rest_code is not None
                assert rest_code.max_stacks == orig_code.max_stacks
                assert rest_code.max_locals == orig_code.max_locals
                assert rest_code.code == orig_code.code
                assert rest_code.exception_table == orig_code.exception_table

        # Class attributes.
        assert restored.attributes == original.attributes

        # CP validity — all entries should be non-None except index 0 and double-slot gaps.
        assert restored.constant_pool[0] is None

    @pytest.mark.parametrize("resource_name", ROUNDTRIP_JAVA_RESOURCES)
    def test_roundtrip_all_java_resources(self, resource_name: str) -> None:
        """Every Java source fixture should round-trip across all generated classes."""

        for class_path in cached_java_resource_classes(resource_name):
            self._assert_roundtrip(class_path)

    def test_roundtrip_from_scratch(self) -> None:
        """A from-scratch model should round-trip through to_classfile."""
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/Test",
            super_name="java/lang/Object",
            interfaces=["java/io/Serializable"],
            fields=[
                FieldModel(
                    access_flags=FieldAccessFlag.PRIVATE,
                    name="x",
                    descriptor="I",
                    attributes=[],
                ),
            ],
            methods=[
                MethodModel(
                    access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
                    name="doIt",
                    descriptor="()V",
                    code=None,
                    attributes=[],
                ),
            ],
            attributes=[],
        )
        cf = cls.to_classfile()

        assert cf.magic == 0xCAFEBABE
        assert cf.major_version == 52
        assert cf.minor_version == 0
        assert _resolve_class_name(cf, cf.this_class) == "com/example/Test"
        assert _resolve_class_name(cf, cf.super_class) == "java/lang/Object"
        assert len(cf.interfaces) == 1
        assert _resolve_class_name(cf, cf.interfaces[0]) == "java/io/Serializable"
        assert len(cf.fields) == 1
        assert _resolve_utf8(cf, cf.fields[0].name_index) == "x"
        assert len(cf.methods) == 1
        assert _resolve_utf8(cf, cf.methods[0].name_index) == "doIt"


# ===========================================================================
# Mutation tests — verify the model is truly mutable
# ===========================================================================


class TestMutation:
    """Test that the editing model supports in-place mutations."""

    def test_add_field(self, hello_world_class: Path) -> None:
        model = ClassModel.from_bytes(hello_world_class.read_bytes())
        original_count = len(model.fields)
        model.fields.append(
            FieldModel(
                access_flags=FieldAccessFlag.PRIVATE,
                name="added",
                descriptor="Z",
                attributes=[],
            )
        )
        assert len(model.fields) == original_count + 1
        cf = model.to_classfile()
        assert cf.fields_count == original_count + 1

    def test_add_method(self, hello_world_class: Path) -> None:
        model = ClassModel.from_bytes(hello_world_class.read_bytes())
        original_count = len(model.methods)
        model.methods.append(
            MethodModel(
                access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
                name="newMethod",
                descriptor="()V",
                code=None,
                attributes=[],
            )
        )
        assert len(model.methods) == original_count + 1
        cf = model.to_classfile()
        assert cf.methods_count == original_count + 1

    def test_rename_class(self, hello_world_class: Path) -> None:
        model = ClassModel.from_bytes(hello_world_class.read_bytes())
        model.name = "RenamedClass"
        cf = model.to_classfile()
        assert _resolve_class_name(cf, cf.this_class) == "RenamedClass"

    def test_add_interface(self, hello_world_class: Path) -> None:
        model = ClassModel.from_bytes(hello_world_class.read_bytes())
        model.interfaces.append("java/io/Serializable")
        cf = model.to_classfile()
        assert len(cf.interfaces) == 1
        assert _resolve_class_name(cf, cf.interfaces[0]) == "java/io/Serializable"

    def test_change_access_flags(self, hello_world_class: Path) -> None:
        model = ClassModel.from_bytes(hello_world_class.read_bytes())
        model.access_flags = ClassAccessFlag.PUBLIC | ClassAccessFlag.FINAL | ClassAccessFlag.SUPER
        cf = model.to_classfile()
        assert ClassAccessFlag.FINAL in cf.access_flags

    def test_remove_method(self, hello_world_class: Path) -> None:
        model = ClassModel.from_bytes(hello_world_class.read_bytes())
        model.methods = [m for m in model.methods if m.name != "giveItToMe"]
        cf = model.to_classfile()
        for m in cf.methods:
            assert _resolve_utf8(cf, m.name_index) != "giveItToMe"


# ===========================================================================
# Ownership tests — model must not share mutable state with ClassFile objects
# ===========================================================================


class TestOwnership:
    """Verify the model does not alias mutable raw structures with parsed or lowered ClassFile objects."""

    def test_model_class_attrs_independent_from_source(self, hello_world_class: Path) -> None:
        """Appending to model.attributes must not mutate the original ClassFile."""
        cf = _read_class(hello_world_class)
        original_attr_count = len(cf.attributes)
        model = ClassModel.from_classfile(cf)
        model.attributes.append(SyntheticAttr(attribute_name_index=1, attribute_length=0))
        assert len(cf.attributes) == original_attr_count

    def test_lifted_const_pool_instructions_do_not_alias_cached_items(self) -> None:
        code = CodeModel(
            max_stack=2,
            max_locals=0,
            instructions=[
                TypeInsn(InsnInfoType.NEW, "java/lang/StringBuilder"),
                TypeInsn(InsnInfoType.NEW, "java/lang/StringBuilder"),
                MethodInsn(
                    InsnInfoType.INVOKESPECIAL,
                    "java/lang/StringBuilder",
                    "<init>",
                    "()V",
                ),
                MethodInsn(
                    InsnInfoType.INVOKESPECIAL,
                    "java/lang/StringBuilder",
                    "<init>",
                    "()V",
                ),
                InsnInfo(InsnInfoType.RETURN, -1),
            ],
        )
        model = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/CacheLift",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[
                MethodModel(
                    access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.STATIC,
                    name="demo",
                    descriptor="()V",
                    code=code,
                    attributes=[],
                )
            ],
            attributes=[],
        )

        reparsed = ClassModel.from_classfile(model.to_classfile())
        lifted = reparsed.methods[0].code
        assert lifted is not None

        first_new = lifted.instructions[0]
        second_new = lifted.instructions[1]
        first_init = lifted.instructions[2]
        second_init = lifted.instructions[3]

        assert first_new == second_new
        assert first_new is not second_new
        assert first_init == second_init
        assert first_init is not second_init

    def test_classfile_class_attrs_independent_from_model(self, hello_world_class: Path) -> None:
        """Appending to a lowered ClassFile's attributes must not mutate the model."""
        model = ClassModel.from_bytes(hello_world_class.read_bytes())
        original_attr_count = len(model.attributes)
        cf = model.to_classfile()
        cf.attributes.append(SyntheticAttr(attribute_name_index=1, attribute_length=0))
        assert len(model.attributes) == original_attr_count

    def test_model_field_attrs_independent_from_source(self, annotated_class: Path) -> None:
        """Appending to FieldModel.attributes must not mutate the original FieldInfo."""
        cf = _read_class(annotated_class)
        original_counts = [len(fi.attributes) for fi in cf.fields]
        model = ClassModel.from_classfile(cf)
        for fm in model.fields:
            fm.attributes.append(SyntheticAttr(attribute_name_index=1, attribute_length=0))
        assert [len(fi.attributes) for fi in cf.fields] == original_counts

    def test_classfile_field_attrs_independent_from_model(self, annotated_class: Path) -> None:
        """Appending to a lowered FieldInfo's attributes must not mutate the FieldModel."""
        model = ClassModel.from_bytes(annotated_class.read_bytes())
        original_counts = [len(fm.attributes) for fm in model.fields]
        cf = model.to_classfile()
        for fi in cf.fields:
            fi.attributes.append(SyntheticAttr(attribute_name_index=1, attribute_length=0))
        assert [len(fm.attributes) for fm in model.fields] == original_counts

    def test_model_code_instructions_independent_from_source(self, hello_world_class: Path) -> None:
        """Clearing CodeModel.instructions must not mutate the original CodeAttr."""
        cf = _read_class(hello_world_class)
        main_mi = next(mi for mi in cf.methods if _resolve_utf8(cf, mi.name_index) == "main")
        orig_code_attr = next(a for a in main_mi.attributes if isinstance(a, CodeAttr))
        original_insn_count = len(orig_code_attr.code)
        model = ClassModel.from_classfile(cf)
        main_mm = find_method_in_model(model, "main")
        assert main_mm.code is not None
        main_mm.code.instructions.clear()
        assert len(orig_code_attr.code) == original_insn_count

    def test_lowered_code_instructions_independent_from_model(self, hello_world_class: Path) -> None:
        """Clearing instructions in a lowered CodeAttr must not affect the CodeModel."""
        model = ClassModel.from_bytes(hello_world_class.read_bytes())
        main_mm = find_method_in_model(model, "main")
        assert main_mm.code is not None
        original_insn_count = len(main_mm.code.instructions)
        cf = model.to_classfile()
        main_mi = next(m for m in cf.methods if _resolve_utf8(cf, m.name_index) == "main")
        rest_code_attr = next(a for a in main_mi.attributes if isinstance(a, CodeAttr))
        rest_code_attr.code.clear()
        assert len(main_mm.code.instructions) == original_insn_count

    def test_model_nested_class_attrs_independent_from_source(self, outer_class: Path) -> None:
        """Mutating InnerClasses entries on the model must not affect the source ClassFile."""
        cf = _read_class(outer_class)
        original_inner = next(a for a in cf.attributes if isinstance(a, InnerClassesAttr))
        original_name_index = original_inner.classes[0].inner_name_index
        model = ClassModel.from_classfile(cf)
        model_inner = next(a for a in model.attributes if isinstance(a, InnerClassesAttr))
        model_inner.classes[0].inner_name_index += 1
        assert original_inner.classes[0].inner_name_index == original_name_index

    def test_classfile_nested_class_attrs_independent_from_model(self, outer_class: Path) -> None:
        """Mutating lowered InnerClasses entries must not affect the model."""
        model = ClassModel.from_bytes(outer_class.read_bytes())
        model_inner = next(a for a in model.attributes if isinstance(a, InnerClassesAttr))
        original_name_index = model_inner.classes[0].inner_name_index
        cf = model.to_classfile()
        lowered_inner = next(a for a in cf.attributes if isinstance(a, InnerClassesAttr))
        lowered_inner.classes[0].inner_name_index += 1
        assert model_inner.classes[0].inner_name_index == original_name_index

    def test_model_nested_field_attrs_independent_from_source(self, annotated_class: Path) -> None:
        """Mutating nested field annotation entries on the model must not affect the source FieldInfo."""
        cf = _read_class(annotated_class)
        source_field = _find_field_info(cf, "oldField")
        source_attr = next(a for a in source_field.attributes if isinstance(a, RuntimeVisibleAnnotationsAttr))
        original_type_index = source_attr.annotations[0].type_index
        model = ClassModel.from_classfile(cf)
        field = _find_field(model, "oldField")
        model_attr = next(a for a in field.attributes if isinstance(a, RuntimeVisibleAnnotationsAttr))
        model_attr.annotations[0].type_index += 1
        assert source_attr.annotations[0].type_index == original_type_index

    def test_classfile_nested_field_attrs_independent_from_model(self, annotated_class: Path) -> None:
        """Mutating nested field annotation entries on lowered output must not affect the model."""
        model = ClassModel.from_bytes(annotated_class.read_bytes())
        field = _find_field(model, "oldField")
        model_attr = next(a for a in field.attributes if isinstance(a, RuntimeVisibleAnnotationsAttr))
        original_type_index = model_attr.annotations[0].type_index
        cf = model.to_classfile()
        lowered_field = _find_field_info(cf, "oldField")
        lowered_attr = next(a for a in lowered_field.attributes if isinstance(a, RuntimeVisibleAnnotationsAttr))
        lowered_attr.annotations[0].type_index += 1
        assert model_attr.annotations[0].type_index == original_type_index

    def test_model_nested_method_attrs_independent_from_source(self, annotated_class: Path) -> None:
        """Mutating nested method annotation entries on the model must not affect the source MethodInfo."""
        cf = _read_class(annotated_class)
        source_method = find_method_in_classfile(cf, "oldMethod")
        source_attr = next(a for a in source_method.attributes if isinstance(a, RuntimeVisibleAnnotationsAttr))
        original_type_index = source_attr.annotations[0].type_index
        model = ClassModel.from_classfile(cf)
        method = find_method_in_model(model, "oldMethod")
        model_attr = next(a for a in method.attributes if isinstance(a, RuntimeVisibleAnnotationsAttr))
        model_attr.annotations[0].type_index += 1
        assert source_attr.annotations[0].type_index == original_type_index

    def test_classfile_nested_method_attrs_independent_from_model(self, annotated_class: Path) -> None:
        """Mutating nested method annotation entries on lowered output must not affect the model."""
        model = ClassModel.from_bytes(annotated_class.read_bytes())
        method = find_method_in_model(model, "oldMethod")
        model_attr = next(a for a in method.attributes if isinstance(a, RuntimeVisibleAnnotationsAttr))
        original_type_index = model_attr.annotations[0].type_index
        cf = model.to_classfile()
        lowered_method = find_method_in_classfile(cf, "oldMethod")
        lowered_attr = next(a for a in lowered_method.attributes if isinstance(a, RuntimeVisibleAnnotationsAttr))
        lowered_attr.annotations[0].type_index += 1
        assert model_attr.annotations[0].type_index == original_type_index

    def test_model_nested_code_attrs_independent_from_source(self, control_flow_class: Path) -> None:
        """Mutating nested StackMapTable frames on the model must not affect the source CodeAttr."""
        cf = _read_class(control_flow_class)
        method_name, source_stack_map = _find_method_with_stack_map(cf)
        original_frame_type = source_stack_map.entries[0].frame_type
        model = ClassModel.from_classfile(cf)
        method = find_method_in_model(model, method_name)
        assert method.code is not None
        model_stack_map = next(a for a in method.code.attributes if isinstance(a, StackMapTableAttr))
        model_stack_map.entries[0].frame_type += 1
        assert source_stack_map.entries[0].frame_type == original_frame_type

    def test_classfile_nested_code_attrs_independent_from_model(self, control_flow_class: Path) -> None:
        """Mutating nested StackMapTable frames on lowered output must not affect the model."""
        model = ClassModel.from_bytes(control_flow_class.read_bytes())
        cf = model.to_classfile()
        method_name, lowered_stack_map = _find_method_with_stack_map(cf)
        method = find_method_in_model(model, method_name)
        assert method.code is not None
        model_stack_map = next(a for a in method.code.attributes if isinstance(a, StackMapTableAttr))
        original_frame_type = model_stack_map.entries[0].frame_type
        lowered_stack_map.entries[0].frame_type += 1
        assert model_stack_map.entries[0].frame_type == original_frame_type


# ===========================================================================
# Negative tests — from_classfile() error paths for malformed constant pools
# ===========================================================================


class TestFromClassFileErrors:
    """Test that from_classfile() raises ValueError for malformed constant-pool references."""

    def test_this_class_not_classinfo(self) -> None:
        # this_class=1 points to Utf8 "TestClass" instead of a ClassInfo
        raw = minimal_classfile(this_class=1)
        with pytest.raises(ValueError, match="this_class"):
            ClassModel.from_bytes(raw)

    def test_super_class_not_classinfo(self) -> None:
        # super_class=3 points to Utf8 "java/lang/Object" instead of a ClassInfo
        raw = minimal_classfile(super_class=3)
        with pytest.raises(ValueError, match="super_class"):
            ClassModel.from_bytes(raw)

    def test_interface_entry_not_classinfo(self) -> None:
        # index 5 is a fresh Utf8 entry; using it as an interface entry must fail
        raw = minimal_classfile(
            extra_cp_bytes=utf8_entry_bytes("BadIface"),
            extra_cp_count=1,
            interfaces=[5],
        )
        with pytest.raises(ValueError, match="interface"):
            ClassModel.from_bytes(raw)

    def test_classinfo_name_index_not_utf8(self) -> None:
        # index 5 = Integer(42), index 6 = Class(name_index=5)
        # this_class=6 → resolve_utf8(5) must raise because 5 is not Utf8
        raw = minimal_classfile(
            extra_cp_bytes=integer_entry_bytes(42) + class_entry_bytes(5),
            extra_cp_count=2,
            this_class=6,
        )
        with pytest.raises(ValueError):
            ClassModel.from_bytes(raw)


# ===========================================================================
# Generic-class fixture tests — Signature attribute passthrough
# ===========================================================================


class TestGenericClassModel:
    def test_generic_class_has_signature_attribute(self, generic_class: Path) -> None:
        model = ClassModel.from_bytes(generic_class.read_bytes())
        assert any(isinstance(a, SignatureAttr) for a in model.attributes)

    def test_generic_field_has_signature_attribute(self, generic_class: Path) -> None:
        model = ClassModel.from_bytes(generic_class.read_bytes())
        f = _find_field(model, "value")
        assert any(isinstance(a, SignatureAttr) for a in f.attributes)

    def test_generic_method_has_signature_attribute(self, generic_class: Path) -> None:
        model = ClassModel.from_bytes(generic_class.read_bytes())
        m = find_method_in_model(model, "getValue")
        assert any(isinstance(a, SignatureAttr) for a in m.attributes)


# ===========================================================================
# Outer/Inner-class fixture tests — InnerClasses attribute passthrough
# ===========================================================================


class TestOuterClassModel:
    def test_outer_has_inner_classes_attribute(self, outer_class: Path) -> None:
        model = ClassModel.from_bytes(outer_class.read_bytes())
        assert any(isinstance(a, InnerClassesAttr) for a in model.attributes)

    def test_inner_class_count(self, outer_class: Path) -> None:
        model = ClassModel.from_bytes(outer_class.read_bytes())
        inner_attr = next(a for a in model.attributes if isinstance(a, InnerClassesAttr))
        assert inner_attr.number_of_classes >= 1


# ===========================================================================
# Empty collections roundtrip — verify model handles edge-case class shapes
# ===========================================================================


class TestEmptyCollectionsRoundtrip:
    """Roundtrip tests for classes with zero methods, fields, interfaces, etc."""

    def test_zero_methods_roundtrip(self) -> None:
        """A class with no methods should roundtrip through to_classfile."""
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/NoMethods",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[
                FieldModel(
                    access_flags=FieldAccessFlag.PUBLIC,
                    name="x",
                    descriptor="I",
                    attributes=[],
                ),
            ],
            methods=[],
            attributes=[],
        )
        cf = cls.to_classfile()
        assert cf.methods_count == 0
        assert cf.methods == []
        assert cf.fields_count == 1

    def test_zero_fields_roundtrip(self) -> None:
        """A class with no fields should roundtrip through to_classfile."""
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT,
            name="com/example/NoFields",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[
                MethodModel(
                    access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
                    name="doIt",
                    descriptor="()V",
                    code=None,
                    attributes=[],
                ),
            ],
            attributes=[],
        )
        cf = cls.to_classfile()
        assert cf.fields_count == 0
        assert cf.fields == []
        assert cf.methods_count == 1

    def test_zero_interfaces_roundtrip(self) -> None:
        """A class with no interfaces should roundtrip through to_classfile."""
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/NoIfaces",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[],
            attributes=[],
        )
        cf = cls.to_classfile()
        assert cf.interfaces_count == 0
        assert cf.interfaces == []

    def test_zero_everything_roundtrip(self) -> None:
        """A class with zero fields, methods, interfaces, and attributes."""
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/Bare",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[],
            attributes=[],
        )
        cf = cls.to_classfile()
        assert cf.magic == 0xCAFEBABE
        assert cf.fields_count == 0
        assert cf.methods_count == 0
        assert cf.interfaces_count == 0
        assert cf.attributes_count == 0

    def test_zero_everything_byte_roundtrip(self) -> None:
        """Bare class should survive to_bytes → ClassReader → to_classfile."""
        from pytecode.classfile.reader import ClassReader

        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="com/example/Bare",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[],
            attributes=[],
        )
        raw = cls.to_bytes()
        reparsed = ClassReader(raw).class_info
        assert reparsed.magic == 0xCAFEBABE
        assert reparsed.fields_count == 0
        assert reparsed.methods_count == 0
        assert reparsed.interfaces_count == 0

    def test_abstract_method_no_code_roundtrip(self) -> None:
        """An abstract method (no Code attribute) should roundtrip cleanly."""
        cls = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT,
            name="com/example/Abstract",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[
                MethodModel(
                    access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
                    name="run",
                    descriptor="()V",
                    code=None,
                    attributes=[],
                ),
            ],
            attributes=[],
        )
        cf = cls.to_classfile()
        assert cf.methods_count == 1
        # Abstract method should have no Code attribute
        method = cf.methods[0]
        assert not any(isinstance(a, CodeAttr) for a in method.attributes)
