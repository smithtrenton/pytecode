"""Tests for pytecode.verify — structural validation with structured diagnostics."""

from __future__ import annotations

from collections.abc import Sequence

import pytest

from pytecode.attributes import (
    AttributeInfo,
    BootstrapMethodsAttr,
    CodeAttr,
    ConstantValueAttr,
    ExceptionInfo,
    ExceptionsAttr,
    MethodParametersAttr,
    ModuleAttr,
    NestHostAttr,
    PermittedSubclassesAttr,
    RecordAttr,
    RuntimeVisibleTypeAnnotationsAttr,
    StackMapTableAttr,
)
from pytecode.constant_pool import (
    ClassInfo,
    ConstantPoolInfo,
    FieldrefInfo,
    FloatInfo,
    IntegerInfo,
    LongInfo,
    MethodHandleInfo,
    MethodrefInfo,
    NameAndTypeInfo,
    Utf8Info,
)
from pytecode.constants import (
    MAGIC,
    ClassAccessFlag,
    FieldAccessFlag,
    MethodAccessFlag,
)
from pytecode.debug_info import mark_class_debug_info_stale, mark_code_debug_info_stale
from pytecode.info import ClassFile, FieldInfo, MethodInfo
from pytecode.instructions import (
    Branch,
    ConstPoolIndex,
    InsnInfo,
    InsnInfoType,
    InvokeDynamic,
    InvokeInterface,
    LocalIndex,
    MultiANewArray,
)
from pytecode.labels import (
    BranchInsn,
    ExceptionHandler,
    Label,
    LineNumberEntry,
    LocalVariableEntry,
    LocalVariableTypeEntry,
    LookupSwitchInsn,
    TableSwitchInsn,
)
from pytecode.model import ClassModel, CodeModel, FieldModel, MethodModel
from pytecode.modified_utf8 import encode_modified_utf8
from pytecode.verify import (
    Category,
    Diagnostic,
    FailFastError,
    Location,
    Severity,
    verify_classfile,
    verify_classmodel,
)
from tests.helpers import cached_java_resource_classes, list_java_resources

# ── Test helpers ──────────────────────────────────────────────────────


def _utf8(index: int, text: str) -> Utf8Info:
    return Utf8Info(index=index, offset=0, tag=1, length=len(text), str_bytes=encode_modified_utf8(text))


def _class(index: int, name_index: int) -> ClassInfo:
    return ClassInfo(index=index, offset=0, tag=7, name_index=name_index)


def _nat(index: int, name_index: int, desc_index: int) -> NameAndTypeInfo:
    return NameAndTypeInfo(index=index, offset=0, tag=12, name_index=name_index, descriptor_index=desc_index)


def _methodref(index: int, class_index: int, nat_index: int) -> MethodrefInfo:
    return MethodrefInfo(index=index, offset=0, tag=10, class_index=class_index, name_and_type_index=nat_index)


def _fieldref(index: int, class_index: int, nat_index: int) -> FieldrefInfo:
    return FieldrefInfo(index=index, offset=0, tag=9, class_index=class_index, name_and_type_index=nat_index)


def _integer(index: int, value: int) -> IntegerInfo:
    return IntegerInfo(index=index, offset=0, tag=3, value_bytes=value)


def _float(index: int, value: int) -> FloatInfo:
    return FloatInfo(index=index, offset=0, tag=4, value_bytes=value)


def _long(index: int, hi: int = 0, lo: int = 0) -> LongInfo:
    return LongInfo(index=index, offset=0, tag=5, high_bytes=hi, low_bytes=lo)


def _make_cp(*entries: ConstantPoolInfo | None) -> list[ConstantPoolInfo | None]:
    """Build a CP list with None at index 0, then the given entries."""
    return [None, *entries]


def _base_cp() -> list[ConstantPoolInfo | None]:
    """Minimal valid CP: Utf8 'TestClass', Class, Utf8 'java/lang/Object', Class."""
    return _make_cp(
        _utf8(1, "TestClass"),
        _class(2, 1),
        _utf8(3, "java/lang/Object"),
        _class(4, 3),
    )


def _make_classfile(**overrides: object) -> ClassFile:
    """Build a minimal valid ClassFile with optional field overrides."""
    defaults: dict[str, object] = {
        "magic": MAGIC,
        "minor_version": 0,
        "major_version": 52,
        "constant_pool_count": 5,
        "constant_pool": _base_cp(),
        "access_flags": ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        "this_class": 2,
        "super_class": 4,
        "interfaces_count": 0,
        "interfaces": [],
        "fields_count": 0,
        "fields": [],
        "methods_count": 0,
        "methods": [],
        "attributes_count": 0,
        "attributes": [],
    }
    defaults.update(overrides)
    return ClassFile(**defaults)  # type: ignore[arg-type]


def _simple_code(
    insns: Sequence[InsnInfo] | None = None,
    code_length: int = 1,
    max_stacks: int = 1,
    max_locals: int = 1,
    exception_table: list[ExceptionInfo] | None = None,
    attributes: list[AttributeInfo] | None = None,
) -> CodeAttr:
    """Build a simple CodeAttr."""
    return CodeAttr(
        attribute_name_index=0,
        attribute_length=0,
        max_stacks=max_stacks,
        max_locals=max_locals,
        code_length=code_length,
        code=list(insns) if insns is not None else [InsnInfo(InsnInfoType.RETURN, 0)],
        exception_table_length=len(exception_table or []),
        exception_table=exception_table or [],
        attributes_count=len(attributes or []),
        attributes=attributes or [],
    )


def _errors(diags: list[Diagnostic]) -> list[Diagnostic]:
    """Filter to ERROR-severity diagnostics."""
    return [d for d in diags if d.severity is Severity.ERROR]


def _warnings(diags: list[Diagnostic]) -> list[Diagnostic]:
    return [d for d in diags if d.severity is Severity.WARNING]


def _by_cat(diags: list[Diagnostic], cat: Category) -> list[Diagnostic]:
    return [d for d in diags if d.category is cat]


# ── Diagnostic model tests ────────────────────────────────────────────


class TestDiagnosticModel:
    def test_severity_values(self) -> None:
        assert Severity.ERROR.value == "error"
        assert Severity.WARNING.value == "warning"
        assert Severity.INFO.value == "info"

    def test_category_values(self) -> None:
        assert Category.MAGIC.value == "magic"
        assert Category.CONSTANT_POOL.value == "constant_pool"

    def test_location_defaults(self) -> None:
        loc = Location()
        assert loc.class_name is None
        assert loc.cp_index is None
        assert loc.bytecode_offset is None

    def test_diagnostic_str(self) -> None:
        d = Diagnostic(Severity.ERROR, Category.MAGIC, "bad magic", Location(class_name="Foo"))
        s = str(d)
        assert "[ERROR]" in s
        assert "[magic]" in s
        assert "bad magic" in s
        assert "class=Foo" in s

    def test_diagnostic_frozen(self) -> None:
        d = Diagnostic(Severity.ERROR, Category.MAGIC, "msg")
        with pytest.raises(AttributeError):
            d.message = "other"  # type: ignore[misc]

    def test_fail_fast_error(self) -> None:
        d = Diagnostic(Severity.ERROR, Category.MAGIC, "boom")
        err = FailFastError(d)
        assert err.diagnostic is d
        assert "boom" in str(err)


# ── Magic & version tests ────────────────────────────────────────────


class TestMagicVersion:
    def test_valid_classfile_no_errors(self) -> None:
        cf = _make_classfile()
        diags = verify_classfile(cf)
        assert _errors(diags) == []

    def test_invalid_magic(self) -> None:
        cf = _make_classfile(magic=0xDEADBEEF)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.MAGIC)
        assert len(errs) == 1
        assert "0xDEADBEEF" in errs[0].message

    def test_major_below_45(self) -> None:
        cf = _make_classfile(major_version=44)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.VERSION)
        assert any("below minimum 45" in e.message for e in errs)

    def test_major_56_minor_1_invalid(self) -> None:
        cf = _make_classfile(major_version=56, minor_version=1)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.VERSION)
        assert any("requires minor 0 or 65535" in e.message for e in errs)

    def test_major_56_minor_0_valid(self) -> None:
        cf = _make_classfile(major_version=56, minor_version=0)
        diags = verify_classfile(cf)
        assert not _by_cat(diags, Category.VERSION)

    def test_major_56_minor_65535_valid(self) -> None:
        cf = _make_classfile(major_version=56, minor_version=65535)
        diags = verify_classfile(cf)
        assert not _by_cat(diags, Category.VERSION)


# ── Constant pool tests ──────────────────────────────────────────────


class TestConstantPool:
    def test_valid_cp(self) -> None:
        cf = _make_classfile()
        diags = verify_classfile(cf)
        assert not _by_cat(_errors(diags), Category.CONSTANT_POOL)

    def test_cp_count_mismatch(self) -> None:
        cf = _make_classfile(constant_pool_count=99)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CONSTANT_POOL)
        assert any("constant_pool_count" in e.message for e in errs)

    def test_class_info_bad_name_index(self) -> None:
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 99),  # name_index 99 is out of range
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
        )
        cf = _make_classfile(constant_pool=cp)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CONSTANT_POOL)
        assert any("name_index" in e.message and "invalid index 99" in e.message for e in errs)

    def test_class_info_name_index_wrong_type(self) -> None:
        # name_index points to a ClassInfo instead of Utf8Info
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _class(3, 1),  # ClassInfo, not Utf8Info
            _class(4, 3),  # name_index 3 points to ClassInfo
        )
        cf = _make_classfile(constant_pool=cp)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CONSTANT_POOL)
        assert any("expected Utf8Info" in e.message for e in errs)

    def test_long_double_slot(self) -> None:
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            _long(5),
            None,  # slot 6 — must be None after Long
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=7)
        diags = verify_classfile(cf)
        assert not _by_cat(_errors(diags), Category.CONSTANT_POOL)

    def test_long_without_none_slot(self) -> None:
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            _long(5),
            _utf8(6, "oops"),  # should be None after Long
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=7)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CONSTANT_POOL)
        assert any("Long/Double" in e.message and "not empty" in e.message for e in errs)

    def test_methodhandle_invalid_kind(self) -> None:
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            MethodHandleInfo(index=5, offset=0, tag=15, reference_kind=0, reference_index=1),
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=6)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CONSTANT_POOL)
        assert any("invalid reference_kind 0" in e.message for e in errs)

    def test_methodhandle_kind_1_expects_fieldref(self) -> None:
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            _utf8(5, "field"),
            _utf8(6, "I"),
            _nat(7, 5, 6),
            _fieldref(8, 2, 7),
            MethodHandleInfo(index=9, offset=0, tag=15, reference_kind=1, reference_index=8),
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=10)
        diags = verify_classfile(cf)
        # Kind 1 (REF_getField) → FieldrefInfo — should be valid
        cp_errs = [e for e in _by_cat(diags, Category.CONSTANT_POOL) if "reference_index" in e.message]
        assert len(cp_errs) == 0

    def test_methodhandle_kind_5_expects_methodref(self) -> None:
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            # Kind 5 (REF_invokeVirtual) but reference_index points to a Utf8Info
            MethodHandleInfo(index=5, offset=0, tag=15, reference_kind=5, reference_index=1),
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=6)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CONSTANT_POOL)
        assert any("reference_index" in e.message and "expected MethodrefInfo" in e.message for e in errs)

    def test_none_slot_without_preceding_long_double(self) -> None:
        # CP#2 is None but CP#1 is Utf8, not Long/Double
        cp: list[ConstantPoolInfo | None] = [
            None,
            _utf8(1, "TestClass"),
            None,  # unexpected None
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
        ]
        cf = _make_classfile(constant_pool=cp, constant_pool_count=5, this_class=4)
        # this_class must point to a ClassInfo, but let's focus on CP validation
        diags = verify_classfile(cf)
        warns = _by_cat(_warnings(diags), Category.CONSTANT_POOL)
        assert any("CP#2 is None" in w.message for w in warns)


# ── Access flag tests ─────────────────────────────────────────────────


class TestAccessFlags:
    def test_interface_must_be_abstract(self) -> None:
        cf = _make_classfile(access_flags=ClassAccessFlag.INTERFACE)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("INTERFACE class must also be ABSTRACT" in e.message for e in errs)

    def test_interface_abstract_valid(self) -> None:
        cf = _make_classfile(access_flags=ClassAccessFlag.INTERFACE | ClassAccessFlag.ABSTRACT)
        diags = verify_classfile(cf)
        flag_errs = [e for e in _by_cat(diags, Category.ACCESS_FLAGS) if e.severity is Severity.ERROR]
        assert not flag_errs

    def test_interface_must_not_be_final(self) -> None:
        cf = _make_classfile(
            access_flags=ClassAccessFlag.INTERFACE | ClassAccessFlag.ABSTRACT | ClassAccessFlag.FINAL,
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("INTERFACE class must not be FINAL" in e.message for e in errs)

    def test_annotation_must_be_interface(self) -> None:
        cf = _make_classfile(access_flags=ClassAccessFlag.ANNOTATION | ClassAccessFlag.ABSTRACT)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("ANNOTATION class must also be INTERFACE" in e.message for e in errs)

    def test_final_and_abstract_illegal(self) -> None:
        cf = _make_classfile(access_flags=ClassAccessFlag.FINAL | ClassAccessFlag.ABSTRACT | ClassAccessFlag.SUPER)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("FINAL and ABSTRACT" in e.message for e in errs)

    def test_module_no_extra_flags(self) -> None:
        cf = _make_classfile(access_flags=ClassAccessFlag.MODULE | ClassAccessFlag.PUBLIC)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("MODULE class has unexpected flags" in e.message for e in errs)

    def test_method_multiple_visibility(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V"), _utf8(7, "Code")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=8,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.PRIVATE,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[_simple_code()],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("multiple visibility" in e.message for e in errs)

    def test_abstract_method_forbidden_flags(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.ABSTRACT | MethodAccessFlag.FINAL,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=0,
                    attributes=[],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("ABSTRACT method" in e.message and "illegal flags" in e.message for e in errs)

    def test_field_final_volatile_illegal(self) -> None:
        cp = _base_cp() + [_utf8(5, "x"), _utf8(6, "I")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            fields_count=1,
            fields=[
                FieldInfo(
                    access_flags=FieldAccessFlag.FINAL | FieldAccessFlag.VOLATILE,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=0,
                    attributes=[],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("FINAL and VOLATILE" in e.message for e in errs)

    def test_interface_field_must_be_public_static_final(self) -> None:
        cp = _base_cp() + [_utf8(5, "x"), _utf8(6, "I")]
        cf = _make_classfile(
            access_flags=ClassAccessFlag.INTERFACE | ClassAccessFlag.ABSTRACT,
            constant_pool=cp,
            constant_pool_count=7,
            fields_count=1,
            fields=[
                FieldInfo(
                    access_flags=FieldAccessFlag.PUBLIC,  # missing STATIC | FINAL
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=0,
                    attributes=[],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("PUBLIC STATIC FINAL" in e.message for e in errs)


# ── Class structure tests ─────────────────────────────────────────────


class TestClassStructure:
    def test_this_class_not_classinfo(self) -> None:
        cf = _make_classfile(this_class=1)  # index 1 is Utf8, not ClassInfo
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("this_class" in e.message and "CONSTANT_Class" in e.message for e in errs)

    def test_super_class_not_classinfo(self) -> None:
        cf = _make_classfile(super_class=1)  # index 1 is Utf8
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("super_class" in e.message for e in errs)

    def test_super_class_zero_not_object(self) -> None:
        cf = _make_classfile(super_class=0)
        diags = verify_classfile(cf)
        warns = _by_cat(_warnings(diags), Category.CLASS_STRUCTURE)
        assert any("super_class is 0" in w.message for w in warns)

    def test_duplicate_interface(self) -> None:
        # Add an interface entry to CP
        cp = _base_cp() + [_utf8(5, "java/io/Serializable"), _class(6, 5)]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            interfaces_count=2,
            interfaces=[6, 6],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Duplicate interface" in e.message for e in errs)

    def test_count_mismatch_fields(self) -> None:
        cf = _make_classfile(fields_count=5)
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("fields_count" in e.message for e in errs)

    def test_duplicate_field(self) -> None:
        cp = _base_cp() + [_utf8(5, "x"), _utf8(6, "I")]
        fi = FieldInfo(
            access_flags=FieldAccessFlag.PUBLIC,
            name_index=5,
            descriptor_index=6,
            attributes_count=0,
            attributes=[],
        )
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            fields_count=2,
            fields=[fi, fi],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Duplicate field" in e.message for e in errs)

    def test_duplicate_method(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        mi = MethodInfo(
            access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
            name_index=5,
            descriptor_index=6,
            attributes_count=0,
            attributes=[],
        )
        cf = _make_classfile(
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT | ClassAccessFlag.SUPER,
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=2,
            methods=[mi, mi],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Duplicate method" in e.message for e in errs)


# ── Field tests ───────────────────────────────────────────────────────


class TestFields:
    def test_invalid_field_name(self) -> None:
        cp = _base_cp() + [_utf8(5, "a.b"), _utf8(6, "I")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            fields_count=1,
            fields=[
                FieldInfo(
                    access_flags=FieldAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=0,
                    attributes=[],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.FIELD)
        assert any("Invalid field name" in e.message for e in errs)

    def test_invalid_field_descriptor(self) -> None:
        cp = _base_cp() + [_utf8(5, "x"), _utf8(6, "XYZ")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            fields_count=1,
            fields=[
                FieldInfo(
                    access_flags=FieldAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=0,
                    attributes=[],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.DESCRIPTOR)
        assert any("Invalid field descriptor" in e.message for e in errs)

    def test_constant_value_type_match(self) -> None:
        # int field with IntegerInfo → valid
        cp = _base_cp() + [_utf8(5, "x"), _utf8(6, "I"), _integer(7, 42)]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=8,
            fields_count=1,
            fields=[
                FieldInfo(
                    access_flags=FieldAccessFlag.PUBLIC | FieldAccessFlag.STATIC | FieldAccessFlag.FINAL,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[ConstantValueAttr(attribute_name_index=0, attribute_length=2, constantvalue_index=7)],
                ),
            ],
        )
        diags = verify_classfile(cf)
        field_errs = [e for e in _by_cat(diags, Category.FIELD) if "type mismatch" in e.message]
        assert len(field_errs) == 0

    def test_constant_value_type_mismatch(self) -> None:
        # int field with FloatInfo → error
        cp = _base_cp() + [_utf8(5, "x"), _utf8(6, "I"), _float(7, 0)]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=8,
            fields_count=1,
            fields=[
                FieldInfo(
                    access_flags=FieldAccessFlag.PUBLIC | FieldAccessFlag.STATIC | FieldAccessFlag.FINAL,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[ConstantValueAttr(attribute_name_index=0, attribute_length=2, constantvalue_index=7)],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.FIELD)
        assert any("type mismatch" in e.message for e in errs)

    def test_multiple_constant_value(self) -> None:
        cp = _base_cp() + [_utf8(5, "x"), _utf8(6, "I"), _integer(7, 1)]
        cv = ConstantValueAttr(attribute_name_index=0, attribute_length=2, constantvalue_index=7)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=8,
            fields_count=1,
            fields=[
                FieldInfo(
                    access_flags=FieldAccessFlag.PUBLIC | FieldAccessFlag.STATIC | FieldAccessFlag.FINAL,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=2,
                    attributes=[cv, cv],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.FIELD)
        assert any("multiple ConstantValue" in e.message for e in errs)

    def test_non_static_constant_value_warning(self) -> None:
        cp = _base_cp() + [_utf8(5, "x"), _utf8(6, "I"), _integer(7, 1)]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=8,
            fields_count=1,
            fields=[
                FieldInfo(
                    access_flags=FieldAccessFlag.PUBLIC,  # non-static
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[ConstantValueAttr(attribute_name_index=0, attribute_length=2, constantvalue_index=7)],
                ),
            ],
        )
        diags = verify_classfile(cf)
        warns = _by_cat(_warnings(diags), Category.FIELD)
        assert any("Non-static" in w.message for w in warns)


# ── Method tests ──────────────────────────────────────────────────────


class TestMethods:
    def test_invalid_method_name(self) -> None:
        cp = _base_cp() + [_utf8(5, "a<b"), _utf8(6, "()V")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=0,
                    attributes=[],
                ),
            ],
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT | ClassAccessFlag.SUPER,
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.METHOD)
        assert any("Invalid method name" in e.message for e in errs)

    def test_init_valid_name(self) -> None:
        cp = _base_cp() + [_utf8(5, "<init>"), _utf8(6, "()V")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[_simple_code()],
                ),
            ],
        )
        diags = verify_classfile(cf)
        method_errs = _by_cat(_errors(diags), Category.METHOD)
        assert not any("Invalid method name" in e.message for e in method_errs)

    def test_init_forbidden_flags(self) -> None:
        cp = _base_cp() + [_utf8(5, "<init>"), _utf8(6, "()V")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.STATIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[_simple_code()],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.METHOD)
        assert any("<init> has illegal flags" in e.message for e in errs)

    def test_clinit_must_be_static(self) -> None:
        cp = _base_cp() + [_utf8(5, "<clinit>"), _utf8(6, "()V")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag(0),  # not STATIC
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[_simple_code()],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.METHOD)
        assert any("<clinit> must be STATIC" in e.message for e in errs)

    def test_clinit_wrong_descriptor(self) -> None:
        cp = _base_cp() + [_utf8(5, "<clinit>"), _utf8(6, "(I)V")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.STATIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[_simple_code()],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.METHOD)
        assert any("<clinit> must have descriptor ()V" in e.message for e in errs)

    def test_abstract_with_code(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        cf = _make_classfile(
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT | ClassAccessFlag.SUPER,
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[_simple_code()],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.METHOD)
        assert any("ABSTRACT method" in e.message and "must not have a Code" in e.message for e in errs)

    def test_non_abstract_without_code(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=0,
                    attributes=[],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.METHOD)
        assert any("must have a Code attribute" in e.message for e in errs)

    def test_multiple_code_attributes(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=2,
                    attributes=[_simple_code(), _simple_code()],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.METHOD)
        assert any("Code attributes (max 1)" in e.message for e in errs)

    def test_multiple_exceptions_attributes(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        exc = ExceptionsAttr(
            attribute_name_index=0,
            attribute_length=0,
            number_of_exceptions=0,
            exception_index_table=[],
        )
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=3,
                    attributes=[_simple_code(), exc, exc],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.METHOD)
        assert any("Exceptions attributes (max 1)" in e.message for e in errs)


# ── Code attribute tests ──────────────────────────────────────────────


class TestCode:
    def test_code_length_zero(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        code = _simple_code(code_length=0)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CODE)
        assert any("code_length must be > 0" in e.message for e in errs)

    def test_code_length_exceeds_max(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        code = _simple_code(code_length=70000)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        errs = _by_cat(diags, Category.CODE)
        assert any("exceeds 65535" in e.message for e in errs)

    def test_branch_target_valid(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        insns = [
            Branch(InsnInfoType.GOTO, 0, offset=1),  # jumps to offset 3 (0+3=3? no, 0+1=1)
            InsnInfo(InsnInfoType.RETURN, 1),
        ]
        code = _simple_code(insns=insns, code_length=4)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = [
            e for e in _by_cat(diags, Category.CODE) if "Branch" in e.message and "invalid offset" in e.message
        ]
        assert len(code_errs) == 0

    def test_branch_target_invalid(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        insns = [
            Branch(InsnInfoType.GOTO, 0, offset=99),  # jumps to invalid offset 99
            InsnInfo(InsnInfoType.RETURN, 3),
        ]
        code = _simple_code(insns=insns, code_length=4)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = _by_cat(diags, Category.CODE)
        assert any("invalid offset 99" in e.message for e in code_errs)

    def test_exception_handler_start_gte_end(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        insns = [InsnInfo(InsnInfoType.RETURN, 0)]
        eh = ExceptionInfo(start_pc=5, end_pc=0, handler_pc=0, catch_type=0)
        code = _simple_code(insns=insns, code_length=1, exception_table=[eh])
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = _by_cat(diags, Category.CODE)
        assert any("start_pc" in e.message and "< end_pc" in e.message for e in code_errs)

    def test_exception_handler_bad_catch_type(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        insns = [InsnInfo(InsnInfoType.NOP, 0), InsnInfo(InsnInfoType.RETURN, 1)]
        eh = ExceptionInfo(start_pc=0, end_pc=1, handler_pc=0, catch_type=1)  # CP#1 is Utf8, not Class
        code = _simple_code(insns=insns, code_length=2, exception_table=[eh])
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = _by_cat(diags, Category.CODE)
        assert any("catch_type" in e.message and "CONSTANT_Class" in e.message for e in code_errs)

    def test_cp_ref_field_wrong_type(self) -> None:
        # GETFIELD references a ClassInfo instead of FieldrefInfo
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        insns = [ConstPoolIndex(InsnInfoType.GETFIELD, 0, index=2)]  # CP#2 is ClassInfo
        code = _simple_code(insns=insns, code_length=3)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = _by_cat(diags, Category.CODE)
        assert any("GETFIELD" in e.message and "FieldrefInfo" in e.message for e in code_errs)

    def test_ldc_valid_integer(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V"), _integer(7, 42)]
        insns = [LocalIndex(InsnInfoType.LDC, 0, index=7)]
        code = _simple_code(insns=insns, code_length=2)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=8,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = [e for e in _by_cat(diags, Category.CODE) if "LDC" in e.message]
        assert len(code_errs) == 0

    def test_ldc_non_loadable_type(self) -> None:
        # LDC points to a NameAndTypeInfo — not loadable
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V"), _nat(7, 5, 6)]
        insns = [LocalIndex(InsnInfoType.LDC, 0, index=7)]
        code = _simple_code(insns=insns, code_length=2)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=8,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = _by_cat(diags, Category.CODE)
        assert any("non-loadable type" in e.message for e in code_errs)

    def test_multianewarray_dimensions_zero(self) -> None:
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        insns = [MultiANewArray(InsnInfoType.MULTIANEWARRAY, 0, index=2, dimensions=0)]
        code = _simple_code(insns=insns, code_length=4)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = _by_cat(diags, Category.CODE)
        assert any("dimensions must be >= 1" in e.message for e in code_errs)

    def test_invokeinterface_wrong_type(self) -> None:
        # INVOKEINTERFACE references a MethodrefInfo instead of InterfaceMethodrefInfo
        cp = _base_cp() + [
            _utf8(5, "foo"),
            _utf8(6, "()V"),
            _nat(7, 5, 6),
            _methodref(8, 2, 7),
        ]
        insns = [InvokeInterface(InsnInfoType.INVOKEINTERFACE, 0, index=8, count=1, unused=b"\x00")]
        code = _simple_code(insns=insns, code_length=5)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=9,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = _by_cat(diags, Category.CODE)
        assert any("INVOKEINTERFACE" in e.message and "InterfaceMethodrefInfo" in e.message for e in code_errs)

    def test_invokedynamic_wrong_type(self) -> None:
        # INVOKEDYNAMIC references a MethodrefInfo instead of InvokeDynamicInfo
        cp = _base_cp() + [
            _utf8(5, "foo"),
            _utf8(6, "()V"),
            _nat(7, 5, 6),
            _methodref(8, 2, 7),
        ]
        insns = [InvokeDynamic(InsnInfoType.INVOKEDYNAMIC, 0, index=8, unused=b"\x00\x00")]
        code = _simple_code(insns=insns, code_length=5)
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=9,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = _by_cat(diags, Category.CODE)
        assert any("INVOKEDYNAMIC" in e.message and "InvokeDynamicInfo" in e.message for e in code_errs)


# ── Attribute versioning tests ────────────────────────────────────────


class TestAttributeVersioning:
    @pytest.mark.parametrize(
        ("attr_cls", "min_ver", "attr_name"),
        [
            (StackMapTableAttr, 50, "StackMapTable"),
            (BootstrapMethodsAttr, 51, "BootstrapMethods"),
            (MethodParametersAttr, 52, "MethodParameters"),
            (RuntimeVisibleTypeAnnotationsAttr, 52, "RuntimeVisibleTypeAnnotations"),
            (ModuleAttr, 53, "Module"),
            (NestHostAttr, 55, "NestHost"),
            (RecordAttr, 60, "Record"),
            (PermittedSubclassesAttr, 61, "PermittedSubclasses"),
        ],
    )
    def test_attribute_below_minimum_version(
        self,
        attr_cls: type[AttributeInfo],
        min_ver: int,
        attr_name: str,
    ) -> None:
        # Build a stub attribute of the right type — enough for isinstance check.
        attr: AttributeInfo
        if attr_cls is StackMapTableAttr:
            attr = StackMapTableAttr(
                attribute_name_index=0,
                attribute_length=0,
                number_of_entries=0,
                entries=[],
            )
        elif attr_cls is BootstrapMethodsAttr:
            attr = BootstrapMethodsAttr(
                attribute_name_index=0,
                attribute_length=0,
                num_bootstrap_methods=0,
                bootstrap_methods=[],
            )
        elif attr_cls is MethodParametersAttr:
            attr = MethodParametersAttr(
                attribute_name_index=0,
                attribute_length=0,
                parameters_count=0,
                parameters=[],
            )
        elif attr_cls is RuntimeVisibleTypeAnnotationsAttr:
            attr = RuntimeVisibleTypeAnnotationsAttr(
                attribute_name_index=0,
                attribute_length=0,
                num_annotations=0,
                annotations=[],
            )
        elif attr_cls is ModuleAttr:
            from pytecode.constants import ModuleAccessFlag

            attr = ModuleAttr(
                attribute_name_index=0,
                attribute_length=0,
                module_name_index=0,
                module_flags=ModuleAccessFlag(0),
                module_version_index=0,
                requires_count=0,
                requires=[],
                exports_count=0,
                exports=[],
                opens_count=0,
                opens=[],
                uses_count=0,
                uses_index=[],
                provides_count=0,
                provides=[],
            )
        elif attr_cls is NestHostAttr:
            attr = NestHostAttr(attribute_name_index=0, attribute_length=0, host_class_index=0)
        elif attr_cls is RecordAttr:
            attr = RecordAttr(attribute_name_index=0, attribute_length=0, components_count=0, components=[])
        elif attr_cls is PermittedSubclassesAttr:
            attr = PermittedSubclassesAttr(attribute_name_index=0, attribute_length=0, number_of_classes=0, classes=[])
        else:
            pytest.fail(f"Unhandled attribute class: {attr_cls}")

        cf = _make_classfile(
            major_version=min_ver - 1,
            attributes_count=1,
            attributes=[attr],
        )
        diags = verify_classfile(cf)
        attr_errs = _by_cat(diags, Category.ATTRIBUTE)
        assert any(attr_name in e.message and f">= {min_ver}" in e.message for e in attr_errs)

    def test_attribute_at_minimum_version_valid(self) -> None:
        attr = NestHostAttr(attribute_name_index=0, attribute_length=0, host_class_index=2)
        cf = _make_classfile(major_version=55, attributes_count=1, attributes=[attr])
        diags = verify_classfile(cf)
        attr_errs = _by_cat(diags, Category.ATTRIBUTE)
        assert not attr_errs


# ── Fail-fast tests ───────────────────────────────────────────────────


class TestFailFast:
    def test_fail_fast_raises_on_first_error(self) -> None:
        cf = _make_classfile(magic=0xDEADBEEF)
        with pytest.raises(FailFastError) as exc_info:
            verify_classfile(cf, fail_fast=True)
        assert exc_info.value.diagnostic.severity is Severity.ERROR

    def test_fail_fast_false_collects_all(self) -> None:
        cf = _make_classfile(magic=0xDEADBEEF, major_version=44)
        diags = verify_classfile(cf)
        errs = _errors(diags)
        assert len(errs) >= 2  # at least magic + version errors


# ── ClassModel tests ──────────────────────────────────────────────────


class TestClassModel:
    def _minimal_model(self, **overrides: object) -> ClassModel:
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        defaults: dict[str, object] = {
            "version": (52, 0),
            "access_flags": ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            "name": "com/example/Test",
            "super_name": "java/lang/Object",
            "interfaces": [],
            "fields": [],
            "methods": [],
            "attributes": [],
            "constant_pool": ConstantPoolBuilder(),
        }
        defaults.update(overrides)
        return ClassModel(**defaults)  # type: ignore[arg-type]

    def test_valid_model_no_errors(self) -> None:
        cm = self._minimal_model()
        diags = verify_classmodel(cm)
        assert _errors(diags) == []

    def test_stale_class_debug_info_warning(self) -> None:
        class_file = cached_java_resource_classes("HelloWorld.java")[0]
        cm = ClassModel.from_bytes(class_file.read_bytes())
        mark_class_debug_info_stale(cm)

        diags = verify_classmodel(cm)

        warns = _by_cat(_warnings(diags), Category.ATTRIBUTE)
        assert any("Class debug metadata is marked stale" in warning.message for warning in warns)

    def test_stale_code_debug_info_warning(self) -> None:
        class_file = cached_java_resource_classes("HelloWorld.java")[0]
        cm = ClassModel.from_bytes(class_file.read_bytes())
        method = next(method for method in cm.methods if method.code is not None and method.code.line_numbers)
        mark_code_debug_info_stale(method)

        diags = verify_classmodel(cm)

        warns = _by_cat(_warnings(diags), Category.CODE)
        assert any(
            "Code debug metadata is marked stale" in warning.message and warning.location.method_name == method.name
            for warning in warns
        )

    def test_invalid_class_name_dots(self) -> None:
        cm = self._minimal_model(name="com.example.Test")
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Invalid class name" in e.message for e in errs)

    def test_invalid_class_name_empty(self) -> None:
        cm = self._minimal_model(name="")
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Invalid class name" in e.message for e in errs)

    def test_invalid_class_name_array(self) -> None:
        cm = self._minimal_model(name="[Ljava/lang/Object;")
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Invalid class name" in e.message for e in errs)

    def test_invalid_super_name(self) -> None:
        cm = self._minimal_model(super_name="java.lang.Object")
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Invalid super class name" in e.message for e in errs)

    def test_no_super_warning(self) -> None:
        cm = self._minimal_model(super_name=None)
        diags = verify_classmodel(cm)
        warns = _by_cat(_warnings(diags), Category.CLASS_STRUCTURE)
        assert any("No superclass" in w.message for w in warns)

    def test_no_super_ok_for_object(self) -> None:
        cm = self._minimal_model(name="java/lang/Object", super_name=None)
        diags = verify_classmodel(cm)
        warns = _by_cat(_warnings(diags), Category.CLASS_STRUCTURE)
        assert not any("No superclass" in w.message for w in warns)

    def test_duplicate_interface(self) -> None:
        cm = self._minimal_model(interfaces=["java/io/Serializable", "java/io/Serializable"])
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Duplicate interface" in e.message for e in errs)

    def test_invalid_interface_name(self) -> None:
        cm = self._minimal_model(interfaces=["java.io.Serializable"])
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Invalid interface name" in e.message for e in errs)

    def test_duplicate_field(self) -> None:
        f1 = FieldModel(access_flags=FieldAccessFlag.PUBLIC, name="x", descriptor="I", attributes=[])
        f2 = FieldModel(access_flags=FieldAccessFlag.PUBLIC, name="x", descriptor="I", attributes=[])
        cm = self._minimal_model(fields=[f1, f2])
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Duplicate field" in e.message for e in errs)

    def test_duplicate_method(self) -> None:
        m1 = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
            name="foo",
            descriptor="()V",
            code=None,
            attributes=[],
        )
        m2 = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
            name="foo",
            descriptor="()V",
            code=None,
            attributes=[],
        )
        cm = self._minimal_model(
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT | ClassAccessFlag.SUPER,
            methods=[m1, m2],
        )
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.CLASS_STRUCTURE)
        assert any("Duplicate method" in e.message for e in errs)

    def test_invalid_field_name(self) -> None:
        f = FieldModel(access_flags=FieldAccessFlag.PUBLIC, name="a;b", descriptor="I", attributes=[])
        cm = self._minimal_model(fields=[f])
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.FIELD)
        assert any("Invalid field name" in e.message for e in errs)

    def test_invalid_field_descriptor(self) -> None:
        f = FieldModel(access_flags=FieldAccessFlag.PUBLIC, name="x", descriptor="INVALID", attributes=[])
        cm = self._minimal_model(fields=[f])
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.DESCRIPTOR)
        assert any("Invalid field descriptor" in e.message for e in errs)

    def test_invalid_method_name(self) -> None:
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
            name="bad<name",
            descriptor="()V",
            code=None,
            attributes=[],
        )
        cm = self._minimal_model(
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT | ClassAccessFlag.SUPER,
            methods=[m],
        )
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.METHOD)
        assert any("Invalid method name" in e.message for e in errs)

    def test_invalid_method_descriptor(self) -> None:
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
            name="foo",
            descriptor="NOT_VALID",
            code=None,
            attributes=[],
        )
        cm = self._minimal_model(
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT | ClassAccessFlag.SUPER,
            methods=[m],
        )
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.DESCRIPTOR)
        assert any("Invalid method descriptor" in e.message for e in errs)

    def test_abstract_method_with_code(self) -> None:
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
            name="foo",
            descriptor="()V",
            code=CodeModel(max_stack=0, max_locals=0),
            attributes=[],
        )
        cm = self._minimal_model(
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT | ClassAccessFlag.SUPER,
            methods=[m],
        )
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.METHOD)
        assert any("ABSTRACT method" in e.message and "must not have code" in e.message for e in errs)

    def test_method_without_code(self) -> None:
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=None,
            attributes=[],
        )
        cm = self._minimal_model(methods=[m])
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.METHOD)
        assert any("must have code" in e.message for e in errs)

    def test_clinit_wrong_descriptor(self) -> None:
        m = MethodModel(
            access_flags=MethodAccessFlag.STATIC,
            name="<clinit>",
            descriptor="(I)V",
            code=CodeModel(max_stack=0, max_locals=0, instructions=[InsnInfo(InsnInfoType.RETURN, 0)]),
            attributes=[],
        )
        cm = self._minimal_model(methods=[m])
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.METHOD)
        assert any("<clinit> must have descriptor ()V" in e.message for e in errs)

    def test_class_flags_interface_not_abstract(self) -> None:
        cm = self._minimal_model(access_flags=ClassAccessFlag.INTERFACE)
        diags = verify_classmodel(cm)
        errs = _by_cat(diags, Category.ACCESS_FLAGS)
        assert any("INTERFACE class must also be ABSTRACT" in e.message for e in errs)


# ── Code model label tests ────────────────────────────────────────────


class TestCodeModelLabels:
    def test_valid_labels(self) -> None:
        start = Label("start")
        end = Label("end")
        handler = Label("handler")
        code = CodeModel(
            max_stack=1,
            max_locals=1,
            instructions=[
                start,
                InsnInfo(InsnInfoType.NOP, 0),
                end,
                handler,
                InsnInfo(InsnInfoType.RETURN, 1),
            ],
            exception_handlers=[ExceptionHandler(start=start, end=end, handler=handler, catch_type=None)],
        )
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=code,
            attributes=[],
        )
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[m],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        diags = verify_classmodel(cm)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert len(code_errs) == 0

    def test_missing_exception_handler_label(self) -> None:
        start = Label("start")
        end = Label("end")
        handler = Label("handler")  # NOT in instruction stream
        code = CodeModel(
            max_stack=1,
            max_locals=1,
            instructions=[
                start,
                InsnInfo(InsnInfoType.NOP, 0),
                end,
                InsnInfo(InsnInfoType.RETURN, 1),
            ],
            exception_handlers=[ExceptionHandler(start=start, end=end, handler=handler, catch_type=None)],
        )
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=code,
            attributes=[],
        )
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[m],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        diags = verify_classmodel(cm)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("handler" in e.message.lower() and "label" in e.message.lower() for e in code_errs)

    def test_missing_branch_target_label(self) -> None:
        target = Label("target")  # NOT in stream
        code = CodeModel(
            max_stack=1,
            max_locals=1,
            instructions=[
                BranchInsn(InsnInfoType.GOTO, target),
                InsnInfo(InsnInfoType.RETURN, 3),
            ],
        )
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=code,
            attributes=[],
        )
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[m],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        diags = verify_classmodel(cm)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("GOTO" in e.message and "label" in e.message.lower() for e in code_errs)

    def test_missing_lookupswitch_label(self) -> None:
        default_label = Label("default")
        case_label = Label("case")  # NOT in stream
        code = CodeModel(
            max_stack=1,
            max_locals=1,
            instructions=[
                default_label,
                LookupSwitchInsn(default_target=default_label, pairs=[(1, case_label)]),
                InsnInfo(InsnInfoType.RETURN, 20),
            ],
        )
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=code,
            attributes=[],
        )
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[m],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        diags = verify_classmodel(cm)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("lookupswitch case 1" in e.message and "label" in e.message.lower() for e in code_errs)

    def test_missing_tableswitch_label(self) -> None:
        default_label = Label("default")
        case_label = Label("case")  # NOT in stream
        code = CodeModel(
            max_stack=1,
            max_locals=1,
            instructions=[
                default_label,
                TableSwitchInsn(default_target=default_label, low=0, high=0, targets=[case_label]),
                InsnInfo(InsnInfoType.RETURN, 20),
            ],
        )
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=code,
            attributes=[],
        )
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[m],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        diags = verify_classmodel(cm)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("tableswitch case" in e.message and "label" in e.message.lower() for e in code_errs)

    def test_missing_line_number_label(self) -> None:
        label = Label("missing")  # NOT in stream
        code = CodeModel(
            max_stack=1,
            max_locals=1,
            instructions=[InsnInfo(InsnInfoType.RETURN, 0)],
            line_numbers=[LineNumberEntry(label=label, line_number=1)],
        )
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=code,
            attributes=[],
        )
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[m],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        diags = verify_classmodel(cm)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("Line number" in e.message and "label" in e.message.lower() for e in code_errs)

    def test_missing_local_variable_labels(self) -> None:
        start = Label("start")  # NOT in stream
        end = Label("end")  # NOT in stream
        code = CodeModel(
            max_stack=1,
            max_locals=1,
            instructions=[InsnInfo(InsnInfoType.RETURN, 0)],
            local_variables=[LocalVariableEntry(start=start, end=end, name="x", descriptor="I", slot=0)],
        )
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=code,
            attributes=[],
        )
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[m],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        diags = verify_classmodel(cm)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert len(code_errs) >= 2  # start and end labels missing

    def test_missing_local_variable_type_labels(self) -> None:
        start = Label("start")  # NOT in stream
        end = Label("end")  # NOT in stream
        code = CodeModel(
            max_stack=1,
            max_locals=1,
            instructions=[InsnInfo(InsnInfoType.RETURN, 0)],
            local_variable_types=[LocalVariableTypeEntry(start=start, end=end, name="x", signature="I", slot=0)],
        )
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=code,
            attributes=[],
        )
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[m],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        diags = verify_classmodel(cm)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert len(code_errs) >= 2  # start and end labels missing

    def test_empty_instructions_warning(self) -> None:
        code = CodeModel(max_stack=0, max_locals=0, instructions=[])
        m = MethodModel(
            access_flags=MethodAccessFlag.PUBLIC,
            name="foo",
            descriptor="()V",
            code=code,
            attributes=[],
        )
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="Test",
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[m],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        diags = verify_classmodel(cm)
        warns = _by_cat(_warnings(diags), Category.CODE)
        assert any("empty instruction list" in w.message for w in warns)


# ── Integration tests with compiled Java fixtures ─────────────────────

JAVA_RESOURCES = list_java_resources()


@pytest.mark.parametrize("resource", JAVA_RESOURCES, ids=JAVA_RESOURCES)
def test_compiled_fixture_zero_errors(resource: str) -> None:
    """Every compiled Java fixture should pass verification with zero ERRORs."""
    from pytecode import ClassModel, ClassReader

    for class_file in cached_java_resource_classes(resource):
        reader = ClassReader.from_file(str(class_file))
        cf = reader.class_info

        # ClassFile-level verification
        cf_diags = verify_classfile(cf)
        cf_errors = _errors(cf_diags)
        assert cf_errors == [], f"ClassFile errors in {class_file.name}: {[str(e) for e in cf_errors]}"

        # ClassModel-level verification
        cm = ClassModel.from_classfile(cf)
        cm_diags = verify_classmodel(cm)
        cm_errors = _errors(cm_diags)
        assert cm_errors == [], f"ClassModel errors in {class_file.name}: {[str(e) for e in cm_errors]}"


class TestClassModelFailFast:
    def test_fail_fast_raises(self) -> None:
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        cm = ClassModel(
            version=(52, 0),
            access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
            name="",  # invalid
            super_name="java/lang/Object",
            interfaces=[],
            fields=[],
            methods=[],
            attributes=[],
            constant_pool=ConstantPoolBuilder(),
        )
        with pytest.raises(FailFastError):
            verify_classmodel(cm, fail_fast=True)


class TestMalformedConstantPool:
    """Edge cases for constant pool validation."""

    def test_cp_ref_to_gap_slot(self) -> None:
        """ClassInfo.name_index pointing to a Long/Double gap slot."""
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            _long(5),
            None,
            _class(7, 6),
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=len(cp))
        diags = verify_classfile(cf)
        cp_errs = _by_cat(_errors(diags), Category.CONSTANT_POOL)
        assert any("references invalid index 6" in e.message for e in cp_errs)

    def test_cp_ref_out_of_bounds(self) -> None:
        """ClassInfo.name_index pointing beyond constant pool."""
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            _class(5, 999),
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=len(cp))
        diags = verify_classfile(cf)
        cp_errs = _by_cat(_errors(diags), Category.CONSTANT_POOL)
        assert any("references invalid index 999" in e.message for e in cp_errs)

    def test_cp_ref_wrong_type(self) -> None:
        """ClassInfo.name_index pointing to Integer instead of Utf8."""
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            _integer(5, 42),
            _class(6, 5),
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=len(cp))
        diags = verify_classfile(cf)
        cp_errs = _by_cat(_errors(diags), Category.CONSTANT_POOL)
        assert any("expected Utf8Info" in e.message for e in cp_errs)

    def test_long_followed_by_non_none(self) -> None:
        """Long entry must be followed by a gap (None) slot."""
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            _long(5),
            _utf8(6, "not_a_gap"),
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=len(cp))
        diags = verify_classfile(cf)
        cp_errs = _by_cat(_errors(diags), Category.CONSTANT_POOL)
        assert any("Long/Double" in e.message and "not empty" in e.message for e in cp_errs)

    def test_none_without_preceding_long_double(self) -> None:
        """None entry not preceded by Long/Double should warn."""
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            None,
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=len(cp))
        diags = verify_classfile(cf)
        cp_warns = _by_cat(_warnings(diags), Category.CONSTANT_POOL)
        assert any("None" in w.message and "not Long/Double" in w.message for w in cp_warns)

    def test_method_handle_invalid_reference_kind(self) -> None:
        """MethodHandle with reference_kind=0 (invalid, must be 1-9)."""
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            MethodHandleInfo(index=5, offset=0, tag=15, reference_kind=0, reference_index=1),
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=len(cp))
        diags = verify_classfile(cf)
        cp_errs = _by_cat(_errors(diags), Category.CONSTANT_POOL)
        assert any("invalid reference_kind" in e.message for e in cp_errs)

    def test_method_handle_reference_kind_10(self) -> None:
        """MethodHandle with reference_kind=10 (above valid range 1-9)."""
        cp = _make_cp(
            _utf8(1, "TestClass"),
            _class(2, 1),
            _utf8(3, "java/lang/Object"),
            _class(4, 3),
            MethodHandleInfo(index=5, offset=0, tag=15, reference_kind=10, reference_index=1),
        )
        cf = _make_classfile(constant_pool=cp, constant_pool_count=len(cp))
        diags = verify_classfile(cf)
        cp_errs = _by_cat(_errors(diags), Category.CONSTANT_POOL)
        assert any("invalid reference_kind" in e.message for e in cp_errs)


class TestExceptionHandlerBoundaries:
    """Edge cases for exception handler validation in Code attributes."""

    def _make_method_with_handler(self, start_pc: int, end_pc: int, handler_pc: int, catch_type: int) -> ClassFile:
        """Build a ClassFile with one method containing an exception handler."""
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V")]
        insns = [
            InsnInfo(InsnInfoType.ACONST_NULL, 0),
            InsnInfo(InsnInfoType.ATHROW, 1),
            InsnInfo(InsnInfoType.RETURN, 2),
        ]
        handler = ExceptionInfo(start_pc=start_pc, end_pc=end_pc, handler_pc=handler_pc, catch_type=catch_type)
        code = _simple_code(insns=insns, code_length=3, exception_table=[handler])
        return _make_classfile(
            constant_pool=cp,
            constant_pool_count=7,
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )

    def test_start_pc_equals_end_pc(self) -> None:
        """start_pc must be strictly less than end_pc."""
        cf = self._make_method_with_handler(start_pc=0, end_pc=0, handler_pc=2, catch_type=0)
        diags = verify_classfile(cf)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("start_pc" in e.message and "end_pc" in e.message for e in code_errs)

    def test_start_pc_greater_than_end_pc(self) -> None:
        """start_pc > end_pc should also be flagged."""
        cf = self._make_method_with_handler(start_pc=2, end_pc=1, handler_pc=0, catch_type=0)
        diags = verify_classfile(cf)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("start_pc" in e.message and "end_pc" in e.message for e in code_errs)

    def test_handler_pc_outside_code(self) -> None:
        """handler_pc pointing beyond the code range."""
        cf = self._make_method_with_handler(start_pc=0, end_pc=1, handler_pc=99, catch_type=0)
        diags = verify_classfile(cf)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("handler_pc" in e.message for e in code_errs)

    def test_start_pc_invalid_offset(self) -> None:
        """start_pc not aligned to a valid instruction offset."""
        cf = self._make_method_with_handler(start_pc=99, end_pc=100, handler_pc=0, catch_type=0)
        diags = verify_classfile(cf)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("start_pc" in e.message for e in code_errs)

    def test_catch_type_not_class_info(self) -> None:
        """catch_type pointing to a non-Class CP entry (an Integer)."""
        cp = _base_cp() + [_utf8(5, "foo"), _utf8(6, "()V"), _integer(7, 42)]
        insns = [
            InsnInfo(InsnInfoType.ACONST_NULL, 0),
            InsnInfo(InsnInfoType.ATHROW, 1),
            InsnInfo(InsnInfoType.RETURN, 2),
        ]
        handler = ExceptionInfo(start_pc=0, end_pc=1, handler_pc=2, catch_type=7)
        code = _simple_code(insns=insns, code_length=3, exception_table=[handler])
        cf = _make_classfile(
            constant_pool=cp,
            constant_pool_count=len(cp),
            methods_count=1,
            methods=[
                MethodInfo(
                    access_flags=MethodAccessFlag.PUBLIC,
                    name_index=5,
                    descriptor_index=6,
                    attributes_count=1,
                    attributes=[code],
                ),
            ],
        )
        diags = verify_classfile(cf)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert any("catch_type" in e.message for e in code_errs)

    def test_catch_type_zero_is_valid(self) -> None:
        """catch_type=0 means catch-all (finally), which is valid."""
        cf = self._make_method_with_handler(start_pc=0, end_pc=1, handler_pc=2, catch_type=0)
        diags = verify_classfile(cf)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert not any("catch_type" in e.message for e in code_errs)

    def test_valid_exception_handler(self) -> None:
        """A well-formed handler should produce no code-level errors."""
        cf = self._make_method_with_handler(start_pc=0, end_pc=1, handler_pc=2, catch_type=0)
        diags = verify_classfile(cf)
        code_errs = _by_cat(_errors(diags), Category.CODE)
        assert not code_errs
