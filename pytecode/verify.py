"""Structural validation for JVM classfiles with structured diagnostics.

Provides two entry points:

- ``verify_classfile(cf)`` — spec-level checks on the parsed ``ClassFile`` model
- ``verify_classmodel(cm)`` — symbolic-level checks on the mutable ``ClassModel``

Both return a list of :class:`Diagnostic` objects carrying severity, category,
message, and location context.  By default all diagnostics are collected; pass
``fail_fast=True`` to raise :class:`FailFastError` on the first ERROR.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum

from .attributes import (
    AnnotationDefaultAttr,
    AttributeInfo,
    BootstrapMethodsAttr,
    CodeAttr,
    ConstantValueAttr,
    ExceptionsAttr,
    LocalVariableTypeTableAttr,
    MethodParametersAttr,
    ModuleAttr,
    ModuleMainClassAttr,
    ModulePackagesAttr,
    NestHostAttr,
    NestMembersAttr,
    PermittedSubclassesAttr,
    RecordAttr,
    RuntimeInvisibleAnnotationsAttr,
    RuntimeInvisibleParameterAnnotationsAttr,
    RuntimeInvisibleTypeAnnotationsAttr,
    RuntimeVisibleAnnotationsAttr,
    RuntimeVisibleParameterAnnotationsAttr,
    RuntimeVisibleTypeAnnotationsAttr,
    SignatureAttr,
    SourceDebugExtensionAttr,
    StackMapTableAttr,
)
from .constant_pool import (
    ClassInfo,
    ConstantPoolInfo,
    DoubleInfo,
    DynamicInfo,
    FieldrefInfo,
    FloatInfo,
    IntegerInfo,
    InterfaceMethodrefInfo,
    InvokeDynamicInfo,
    LongInfo,
    MethodHandleInfo,
    MethodrefInfo,
    MethodTypeInfo,
    ModuleInfo,
    NameAndTypeInfo,
    PackageInfo,
    StringInfo,
    Utf8Info,
)
from .constants import MAGIC, ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from .debug_info import is_class_debug_info_stale, is_code_debug_info_stale
from .descriptors import is_valid_field_descriptor, is_valid_method_descriptor
from .info import ClassFile, FieldInfo, MethodInfo
from .instructions import (
    Branch,
    BranchW,
    ConstPoolIndex,
    InsnInfoType,
    InvokeDynamic,
    InvokeInterface,
    LocalIndex,
    LookupSwitch,
    MultiANewArray,
    TableSwitch,
)
from .labels import (
    BranchInsn,
    Label,
    LookupSwitchInsn,
    TableSwitchInsn,
)
from .model import ClassModel, CodeModel, FieldModel, MethodModel
from .modified_utf8 import decode_modified_utf8

# ── Diagnostic model ──────────────────────────────────────────────────


class Severity(Enum):
    """Diagnostic severity level."""

    ERROR = "error"
    WARNING = "warning"
    INFO = "info"


class Category(Enum):
    """Diagnostic category grouping."""

    MAGIC = "magic"
    VERSION = "version"
    CONSTANT_POOL = "constant_pool"
    ACCESS_FLAGS = "access_flags"
    CLASS_STRUCTURE = "class_structure"
    FIELD = "field"
    METHOD = "method"
    CODE = "code"
    ATTRIBUTE = "attribute"
    DESCRIPTOR = "descriptor"


@dataclass(frozen=True)
class Location:
    """Context for where a diagnostic was raised."""

    class_name: str | None = None
    field_name: str | None = None
    method_name: str | None = None
    method_descriptor: str | None = None
    attribute_name: str | None = None
    cp_index: int | None = None
    bytecode_offset: int | None = None


@dataclass(frozen=True)
class Diagnostic:
    """A single validation finding."""

    severity: Severity
    category: Category
    message: str
    location: Location = field(default_factory=Location)

    def __str__(self) -> str:
        parts = [f"[{self.severity.value.upper()}]", f"[{self.category.value}]", self.message]
        loc_parts: list[str] = []
        if self.location.class_name:
            loc_parts.append(f"class={self.location.class_name}")
        if self.location.method_name:
            loc_parts.append(f"method={self.location.method_name}")
        if self.location.method_descriptor:
            loc_parts.append(f"desc={self.location.method_descriptor}")
        if self.location.field_name:
            loc_parts.append(f"field={self.location.field_name}")
        if self.location.cp_index is not None:
            loc_parts.append(f"cp={self.location.cp_index}")
        if self.location.bytecode_offset is not None:
            loc_parts.append(f"offset={self.location.bytecode_offset}")
        if loc_parts:
            parts.append(f"({', '.join(loc_parts)})")
        return " ".join(parts)


class FailFastError(Exception):
    """Raised when ``fail_fast=True`` and an ERROR-severity diagnostic is found."""

    def __init__(self, diagnostic: Diagnostic) -> None:
        self.diagnostic = diagnostic
        super().__init__(str(diagnostic))


# ── Internal helpers ──────────────────────────────────────────────────


class _Collector:
    """Accumulates diagnostics; optionally raises on first ERROR."""

    __slots__ = ("diagnostics", "_fail_fast")

    def __init__(self, fail_fast: bool) -> None:
        self.diagnostics: list[Diagnostic] = []
        self._fail_fast = fail_fast

    def add(
        self,
        severity: Severity,
        category: Category,
        message: str,
        location: Location | None = None,
    ) -> None:
        diag = Diagnostic(severity, category, message, location or Location())
        self.diagnostics.append(diag)
        if self._fail_fast and severity is Severity.ERROR:
            raise FailFastError(diag)


def _resolve_cp_utf8(cp: list[ConstantPoolInfo | None], index: int) -> str | None:
    """Resolve a CP index to a decoded UTF-8 string, or ``None`` if invalid."""
    if 1 <= index < len(cp):
        entry = cp[index]
        if isinstance(entry, Utf8Info):
            try:
                return decode_modified_utf8(entry.str_bytes)
            except Exception:
                return None
    return None


def _cp_entry(cp: list[ConstantPoolInfo | None], index: int) -> ConstantPoolInfo | None:
    """Return the CP entry at *index*, or ``None`` if out of range."""
    if 1 <= index < len(cp):
        return cp[index]
    return None


def _is_valid_internal_name(name: str) -> bool:
    """Check if *name* is a valid JVM internal-form class name.

    Internal names use ``/`` as the package separator (not ``.``), must not
    contain ``;`` or ``[``, and must have non-empty segments.
    """
    if not name or name.startswith("/") or name.endswith("/") or "//" in name:
        return False
    if any(c in ".;[" for c in name):
        return False
    return True


def _is_valid_unqualified_name(name: str) -> bool:
    """Check if *name* is a valid JVM unqualified name (field or method)."""
    if not name:
        return False
    return not any(c in ".;[/" for c in name)


def _is_valid_method_name(name: str) -> bool:
    """Check if *name* is a valid JVM method name."""
    if name in ("<init>", "<clinit>"):
        return True
    if not name:
        return False
    return not any(c in ".;[/<>" for c in name)


def _resolve_class_name(cf: ClassFile) -> str | None:
    """Best-effort class name resolution for diagnostic locations."""
    entry = _cp_entry(cf.constant_pool, cf.this_class)
    if isinstance(entry, ClassInfo):
        return _resolve_cp_utf8(cf.constant_pool, entry.name_index)
    return None


# ── Attribute version requirements ────────────────────────────────────

_ATTR_MIN_VERSION: dict[type[AttributeInfo], tuple[int, str]] = {
    StackMapTableAttr: (50, "StackMapTable"),
    SourceDebugExtensionAttr: (49, "SourceDebugExtension"),
    LocalVariableTypeTableAttr: (49, "LocalVariableTypeTable"),
    SignatureAttr: (49, "Signature"),
    RuntimeVisibleAnnotationsAttr: (49, "RuntimeVisibleAnnotations"),
    RuntimeInvisibleAnnotationsAttr: (49, "RuntimeInvisibleAnnotations"),
    RuntimeVisibleParameterAnnotationsAttr: (49, "RuntimeVisibleParameterAnnotations"),
    RuntimeInvisibleParameterAnnotationsAttr: (49, "RuntimeInvisibleParameterAnnotations"),
    AnnotationDefaultAttr: (49, "AnnotationDefault"),
    BootstrapMethodsAttr: (51, "BootstrapMethods"),
    MethodParametersAttr: (52, "MethodParameters"),
    RuntimeVisibleTypeAnnotationsAttr: (52, "RuntimeVisibleTypeAnnotations"),
    RuntimeInvisibleTypeAnnotationsAttr: (52, "RuntimeInvisibleTypeAnnotations"),
    ModuleAttr: (53, "Module"),
    ModulePackagesAttr: (53, "ModulePackages"),
    ModuleMainClassAttr: (53, "ModuleMainClass"),
    NestHostAttr: (55, "NestHost"),
    NestMembersAttr: (55, "NestMembers"),
    RecordAttr: (60, "Record"),
    PermittedSubclassesAttr: (61, "PermittedSubclasses"),
}


# ── Shared flag validation ────────────────────────────────────────────


def _check_class_flags(flags: ClassAccessFlag, loc: Location, dc: _Collector) -> None:
    if ClassAccessFlag.INTERFACE in flags:
        if ClassAccessFlag.ABSTRACT not in flags:
            dc.add(Severity.ERROR, Category.ACCESS_FLAGS, "INTERFACE class must also be ABSTRACT", loc)
        if ClassAccessFlag.FINAL in flags:
            dc.add(Severity.ERROR, Category.ACCESS_FLAGS, "INTERFACE class must not be FINAL", loc)
        if ClassAccessFlag.SUPER in flags:
            dc.add(Severity.WARNING, Category.ACCESS_FLAGS, "INTERFACE class should not have SUPER flag", loc)
        if ClassAccessFlag.ENUM in flags:
            dc.add(Severity.ERROR, Category.ACCESS_FLAGS, "INTERFACE class must not be ENUM", loc)

    if ClassAccessFlag.ANNOTATION in flags and ClassAccessFlag.INTERFACE not in flags:
        dc.add(Severity.ERROR, Category.ACCESS_FLAGS, "ANNOTATION class must also be INTERFACE", loc)

    if ClassAccessFlag.MODULE in flags:
        non_module = int(flags) & ~(int(ClassAccessFlag.MODULE) | int(ClassAccessFlag.SYNTHETIC))
        if non_module:
            dc.add(
                Severity.ERROR,
                Category.ACCESS_FLAGS,
                f"MODULE class has unexpected flags: 0x{non_module:04X}",
                loc,
            )

    if ClassAccessFlag.FINAL in flags and ClassAccessFlag.ABSTRACT in flags and ClassAccessFlag.INTERFACE not in flags:
        dc.add(Severity.ERROR, Category.ACCESS_FLAGS, "Class cannot be both FINAL and ABSTRACT", loc)


def _check_method_flags(
    flags: MethodAccessFlag,
    name: str | None,
    is_interface: bool,
    major: int,
    loc: Location,
    dc: _Collector,
) -> None:
    vis_count = sum(
        1 for f in (MethodAccessFlag.PUBLIC, MethodAccessFlag.PRIVATE, MethodAccessFlag.PROTECTED) if f in flags
    )
    if vis_count > 1:
        dc.add(Severity.ERROR, Category.ACCESS_FLAGS, f"Method {name!r} has multiple visibility modifiers", loc)

    if MethodAccessFlag.ABSTRACT in flags:
        forbidden = (
            MethodAccessFlag.PRIVATE
            | MethodAccessFlag.STATIC
            | MethodAccessFlag.FINAL
            | MethodAccessFlag.SYNCHRONIZED
            | MethodAccessFlag.NATIVE
            | MethodAccessFlag.STRICT
        )
        bad = flags & forbidden
        if bad:
            dc.add(Severity.ERROR, Category.ACCESS_FLAGS, f"ABSTRACT method {name!r} has illegal flags: {bad!r}", loc)

    if is_interface and name not in ("<clinit>",):
        if major < 52:
            if MethodAccessFlag.PUBLIC not in flags or MethodAccessFlag.ABSTRACT not in flags:
                if name != "<init>":
                    dc.add(
                        Severity.ERROR,
                        Category.ACCESS_FLAGS,
                        f"Interface method {name!r} must be PUBLIC ABSTRACT (pre-Java 8)",
                        loc,
                    )
        elif MethodAccessFlag.PUBLIC not in flags:
            if major < 53 or MethodAccessFlag.PRIVATE not in flags:
                if name != "<init>":
                    dc.add(
                        Severity.ERROR,
                        Category.ACCESS_FLAGS,
                        f"Interface method {name!r} must be PUBLIC (or PRIVATE for Java 9+)",
                        loc,
                    )

    if name == "<init>":
        forbidden_init = (
            MethodAccessFlag.STATIC
            | MethodAccessFlag.FINAL
            | MethodAccessFlag.SYNCHRONIZED
            | MethodAccessFlag.NATIVE
            | MethodAccessFlag.ABSTRACT
            | MethodAccessFlag.BRIDGE
        )
        bad = flags & forbidden_init
        if bad:
            dc.add(Severity.ERROR, Category.METHOD, f"<init> has illegal flags: {bad!r}", loc)

    if name == "<clinit>":
        if MethodAccessFlag.STATIC not in flags:
            dc.add(Severity.ERROR, Category.METHOD, "<clinit> must be STATIC", loc)


def _check_field_flags(
    flags: FieldAccessFlag,
    name: str | None,
    is_interface: bool,
    loc: Location,
    dc: _Collector,
) -> None:
    vis_count = sum(
        1 for f in (FieldAccessFlag.PUBLIC, FieldAccessFlag.PRIVATE, FieldAccessFlag.PROTECTED) if f in flags
    )
    if vis_count > 1:
        dc.add(Severity.ERROR, Category.ACCESS_FLAGS, f"Field {name!r} has multiple visibility modifiers", loc)

    if FieldAccessFlag.FINAL in flags and FieldAccessFlag.VOLATILE in flags:
        dc.add(Severity.ERROR, Category.ACCESS_FLAGS, f"Field {name!r} cannot be both FINAL and VOLATILE", loc)

    if is_interface:
        required = FieldAccessFlag.PUBLIC | FieldAccessFlag.STATIC | FieldAccessFlag.FINAL
        if (flags & required) != required:
            dc.add(Severity.ERROR, Category.ACCESS_FLAGS, f"Interface field {name!r} must be PUBLIC STATIC FINAL", loc)


# ── Shared attribute version checking ─────────────────────────────────


def _verify_attr_versions(attrs: list[AttributeInfo], major: int, loc: Location, dc: _Collector) -> None:
    """Check that attributes satisfy their minimum version requirements."""
    for attr in attrs:
        attr_type = type(attr)
        if attr_type in _ATTR_MIN_VERSION:
            min_ver, attr_name = _ATTR_MIN_VERSION[attr_type]
            if major < min_ver:
                dc.add(
                    Severity.ERROR,
                    Category.ATTRIBUTE,
                    f"{attr_name} attribute requires classfile version >= {min_ver}, got {major}",
                    Location(
                        class_name=loc.class_name,
                        field_name=loc.field_name,
                        method_name=loc.method_name,
                        method_descriptor=loc.method_descriptor,
                        attribute_name=attr_name,
                    ),
                )
        if isinstance(attr, CodeAttr):
            _verify_attr_versions(attr.attributes, major, loc, dc)


# ── ClassFile verification ────────────────────────────────────────────


def _verify_magic_version(cf: ClassFile, dc: _Collector, loc: Location) -> None:
    if cf.magic != MAGIC:
        dc.add(
            Severity.ERROR,
            Category.MAGIC,
            f"Invalid magic number: 0x{cf.magic:08X} (expected 0xCAFEBABE)",
            loc,
        )
    if cf.major_version < 45:
        dc.add(Severity.ERROR, Category.VERSION, f"Major version {cf.major_version} is below minimum 45", loc)
    if cf.major_version >= 56 and cf.minor_version not in (0, 65535):
        dc.add(
            Severity.ERROR,
            Category.VERSION,
            f"Major version {cf.major_version} (>=56) requires minor 0 or 65535, got {cf.minor_version}",
            loc,
        )


def _verify_constant_pool(cf: ClassFile, dc: _Collector, class_name: str | None) -> None:
    cp = cf.constant_pool

    if cf.constant_pool_count != len(cp):
        dc.add(
            Severity.ERROR,
            Category.CONSTANT_POOL,
            f"constant_pool_count ({cf.constant_pool_count}) != len(constant_pool) ({len(cp)})",
            Location(class_name=class_name),
        )

    def _check_ref(index: int, expected: type | tuple[type, ...], entry_idx: int, ref_field: str) -> None:
        loc = Location(class_name=class_name, cp_index=entry_idx)
        target = _cp_entry(cp, index)
        if target is None:
            dc.add(
                Severity.ERROR,
                Category.CONSTANT_POOL,
                f"CP#{entry_idx}.{ref_field} references invalid index {index}",
                loc,
            )
        elif not isinstance(target, expected):
            if isinstance(expected, type):
                exp_name = expected.__name__
            else:
                exp_name = " | ".join(t.__name__ for t in expected)
            dc.add(
                Severity.ERROR,
                Category.CONSTANT_POOL,
                f"CP#{entry_idx}.{ref_field} (index {index}) expected {exp_name}, got {type(target).__name__}",
                loc,
            )

    i = 1
    while i < len(cp):
        entry = cp[i]
        if entry is None:
            if i > 1:
                prev = cp[i - 1]
                if not isinstance(prev, (LongInfo, DoubleInfo)):
                    dc.add(
                        Severity.WARNING,
                        Category.CONSTANT_POOL,
                        f"CP#{i} is None but previous entry is not Long/Double",
                        Location(class_name=class_name, cp_index=i),
                    )
            i += 1
            continue

        if isinstance(entry, ClassInfo):
            _check_ref(entry.name_index, Utf8Info, i, "name_index")
        elif isinstance(entry, StringInfo):
            _check_ref(entry.string_index, Utf8Info, i, "string_index")
        elif isinstance(entry, (FieldrefInfo, MethodrefInfo, InterfaceMethodrefInfo)):
            _check_ref(entry.class_index, ClassInfo, i, "class_index")
            _check_ref(entry.name_and_type_index, NameAndTypeInfo, i, "name_and_type_index")
        elif isinstance(entry, NameAndTypeInfo):
            _check_ref(entry.name_index, Utf8Info, i, "name_index")
            _check_ref(entry.descriptor_index, Utf8Info, i, "descriptor_index")
        elif isinstance(entry, MethodHandleInfo):
            loc = Location(class_name=class_name, cp_index=i)
            if not (1 <= entry.reference_kind <= 9):
                dc.add(
                    Severity.ERROR,
                    Category.CONSTANT_POOL,
                    f"CP#{i} MethodHandle has invalid reference_kind {entry.reference_kind}",
                    loc,
                )
            else:
                kind = entry.reference_kind
                if kind <= 4:
                    _check_ref(entry.reference_index, FieldrefInfo, i, "reference_index")
                elif kind in (5, 8):
                    _check_ref(entry.reference_index, MethodrefInfo, i, "reference_index")
                elif kind in (6, 7):
                    _check_ref(
                        entry.reference_index,
                        (MethodrefInfo, InterfaceMethodrefInfo),
                        i,
                        "reference_index",
                    )
                elif kind == 9:
                    _check_ref(entry.reference_index, InterfaceMethodrefInfo, i, "reference_index")
        elif isinstance(entry, MethodTypeInfo):
            _check_ref(entry.descriptor_index, Utf8Info, i, "descriptor_index")
        elif isinstance(entry, (DynamicInfo, InvokeDynamicInfo)):
            _check_ref(entry.name_and_type_index, NameAndTypeInfo, i, "name_and_type_index")
        elif isinstance(entry, (ModuleInfo, PackageInfo)):
            _check_ref(entry.name_index, Utf8Info, i, "name_index")
        elif isinstance(entry, (LongInfo, DoubleInfo)):
            if i + 1 < len(cp) and cp[i + 1] is not None:
                dc.add(
                    Severity.ERROR,
                    Category.CONSTANT_POOL,
                    f"CP#{i} is Long/Double but CP#{i + 1} is not empty",
                    Location(class_name=class_name, cp_index=i),
                )
            i += 2
            continue

        i += 1


def _verify_class_structure(cf: ClassFile, dc: _Collector, class_name: str | None) -> None:
    cp = cf.constant_pool
    loc = Location(class_name=class_name)

    if not isinstance(_cp_entry(cp, cf.this_class), ClassInfo):
        dc.add(
            Severity.ERROR,
            Category.CLASS_STRUCTURE,
            f"this_class (index {cf.this_class}) does not point to CONSTANT_Class",
            loc,
        )

    if cf.super_class != 0:
        if not isinstance(_cp_entry(cp, cf.super_class), ClassInfo):
            dc.add(
                Severity.ERROR,
                Category.CLASS_STRUCTURE,
                f"super_class (index {cf.super_class}) does not point to CONSTANT_Class",
                loc,
            )
    elif class_name != "java/lang/Object":
        dc.add(
            Severity.WARNING,
            Category.CLASS_STRUCTURE,
            "super_class is 0 but class is not java/lang/Object",
            loc,
        )

    seen_ifaces: set[int] = set()
    for idx in cf.interfaces:
        if not isinstance(_cp_entry(cp, idx), ClassInfo):
            dc.add(
                Severity.ERROR,
                Category.CLASS_STRUCTURE,
                f"Interface index {idx} does not point to CONSTANT_Class",
                loc,
            )
        if idx in seen_ifaces:
            dc.add(Severity.ERROR, Category.CLASS_STRUCTURE, f"Duplicate interface index {idx}", loc)
        seen_ifaces.add(idx)

    if cf.interfaces_count != len(cf.interfaces):
        dc.add(
            Severity.ERROR,
            Category.CLASS_STRUCTURE,
            f"interfaces_count ({cf.interfaces_count}) != len(interfaces) ({len(cf.interfaces)})",
            loc,
        )
    if cf.fields_count != len(cf.fields):
        dc.add(
            Severity.ERROR,
            Category.CLASS_STRUCTURE,
            f"fields_count ({cf.fields_count}) != len(fields) ({len(cf.fields)})",
            loc,
        )
    if cf.methods_count != len(cf.methods):
        dc.add(
            Severity.ERROR,
            Category.CLASS_STRUCTURE,
            f"methods_count ({cf.methods_count}) != len(methods) ({len(cf.methods)})",
            loc,
        )
    if cf.attributes_count != len(cf.attributes):
        dc.add(
            Severity.ERROR,
            Category.CLASS_STRUCTURE,
            f"attributes_count ({cf.attributes_count}) != len(attributes) ({len(cf.attributes)})",
            loc,
        )

    # Duplicate fields (resolved name + descriptor).
    field_sigs: set[tuple[str | None, str | None]] = set()
    for fi in cf.fields:
        name = _resolve_cp_utf8(cp, fi.name_index)
        desc = _resolve_cp_utf8(cp, fi.descriptor_index)
        key = (name, desc)
        if name is not None and key in field_sigs:
            dc.add(Severity.ERROR, Category.CLASS_STRUCTURE, f"Duplicate field: {name} {desc}", loc)
        field_sigs.add(key)

    # Duplicate methods (resolved name + descriptor).
    method_sigs: set[tuple[str | None, str | None]] = set()
    for mi in cf.methods:
        name = _resolve_cp_utf8(cp, mi.name_index)
        desc = _resolve_cp_utf8(cp, mi.descriptor_index)
        key = (name, desc)
        if name is not None and key in method_sigs:
            dc.add(Severity.ERROR, Category.CLASS_STRUCTURE, f"Duplicate method: {name}{desc}", loc)
        method_sigs.add(key)


def _verify_field(
    fi: FieldInfo,
    cf: ClassFile,
    dc: _Collector,
    class_name: str | None,
    is_interface: bool,
) -> None:
    cp = cf.constant_pool
    name = _resolve_cp_utf8(cp, fi.name_index)
    desc = _resolve_cp_utf8(cp, fi.descriptor_index)
    loc = Location(class_name=class_name, field_name=name)

    if not isinstance(_cp_entry(cp, fi.name_index), Utf8Info):
        dc.add(Severity.ERROR, Category.FIELD, f"Field name_index {fi.name_index} is not Utf8Info", loc)
    elif name is not None and not _is_valid_unqualified_name(name):
        dc.add(Severity.ERROR, Category.FIELD, f"Invalid field name: {name!r}", loc)

    if not isinstance(_cp_entry(cp, fi.descriptor_index), Utf8Info):
        dc.add(
            Severity.ERROR,
            Category.FIELD,
            f"Field descriptor_index {fi.descriptor_index} is not Utf8Info",
            loc,
        )
    elif desc is not None and not is_valid_field_descriptor(desc):
        dc.add(Severity.ERROR, Category.DESCRIPTOR, f"Invalid field descriptor: {desc!r}", loc)

    _check_field_flags(fi.access_flags, name, is_interface, loc, dc)

    # ConstantValue checks.
    cv_count = 0
    for attr in fi.attributes:
        if isinstance(attr, ConstantValueAttr):
            cv_count += 1
            if cv_count > 1:
                dc.add(Severity.ERROR, Category.FIELD, f"Field {name!r} has multiple ConstantValue attributes", loc)
            cv_entry = _cp_entry(cp, attr.constantvalue_index)
            if cv_entry is None:
                dc.add(
                    Severity.ERROR,
                    Category.FIELD,
                    f"Field {name!r} ConstantValue index {attr.constantvalue_index} is invalid",
                    loc,
                )
            elif desc is not None and is_valid_field_descriptor(desc):
                _verify_cv_type(desc, cv_entry, name, loc, dc)
            if FieldAccessFlag.STATIC not in fi.access_flags:
                dc.add(
                    Severity.WARNING,
                    Category.FIELD,
                    f"Non-static field {name!r} has ConstantValue (ignored by JVM)",
                    loc,
                )

    _verify_attr_versions(fi.attributes, cf.major_version, loc, dc)


def _verify_cv_type(
    desc: str,
    entry: ConstantPoolInfo,
    field_name: str | None,
    loc: Location,
    dc: _Collector,
) -> None:
    """Check that a ConstantValue entry type matches the field descriptor."""
    char = desc[0] if desc else ""
    expected: type | tuple[type, ...] | None = None
    if char in "ISCBZ":
        expected = IntegerInfo
    elif char == "J":
        expected = LongInfo
    elif char == "F":
        expected = FloatInfo
    elif char == "D":
        expected = DoubleInfo
    elif desc == "Ljava/lang/String;":
        expected = StringInfo
    if expected is not None and not isinstance(entry, expected):
        exp_name = expected.__name__
        msg = (
            f"Field {field_name!r} ConstantValue type mismatch: "
            f"descriptor {desc!r} expects {exp_name}, got {type(entry).__name__}"
        )
        dc.add(Severity.ERROR, Category.FIELD, msg, loc)


def _verify_method(
    mi: MethodInfo,
    cf: ClassFile,
    dc: _Collector,
    class_name: str | None,
    is_interface: bool,
) -> None:
    cp = cf.constant_pool
    name = _resolve_cp_utf8(cp, mi.name_index)
    desc = _resolve_cp_utf8(cp, mi.descriptor_index)
    loc = Location(class_name=class_name, method_name=name, method_descriptor=desc)
    major = cf.major_version

    if not isinstance(_cp_entry(cp, mi.name_index), Utf8Info):
        dc.add(Severity.ERROR, Category.METHOD, f"Method name_index {mi.name_index} is not Utf8Info", loc)
    elif name is not None and not _is_valid_method_name(name):
        dc.add(Severity.ERROR, Category.METHOD, f"Invalid method name: {name!r}", loc)

    if not isinstance(_cp_entry(cp, mi.descriptor_index), Utf8Info):
        dc.add(
            Severity.ERROR,
            Category.METHOD,
            f"Method descriptor_index {mi.descriptor_index} is not Utf8Info",
            loc,
        )
    elif desc is not None and not is_valid_method_descriptor(desc):
        dc.add(Severity.ERROR, Category.DESCRIPTOR, f"Invalid method descriptor: {desc!r}", loc)

    _check_method_flags(mi.access_flags, name, is_interface, major, loc, dc)

    if name == "<clinit>" and desc is not None and desc != "()V":
        dc.add(Severity.ERROR, Category.METHOD, f"<clinit> must have descriptor ()V, got {desc!r}", loc)

    has_code = any(isinstance(a, CodeAttr) for a in mi.attributes)
    code_count = sum(1 for a in mi.attributes if isinstance(a, CodeAttr))
    is_abstract = MethodAccessFlag.ABSTRACT in mi.access_flags
    is_native = MethodAccessFlag.NATIVE in mi.access_flags

    if is_abstract or is_native:
        if has_code:
            label = "ABSTRACT" if is_abstract else "NATIVE"
            dc.add(Severity.ERROR, Category.METHOD, f"{label} method {name!r} must not have a Code attribute", loc)
    else:
        if not has_code:
            dc.add(Severity.ERROR, Category.METHOD, f"Method {name!r} must have a Code attribute", loc)
        if code_count > 1:
            dc.add(
                Severity.ERROR,
                Category.METHOD,
                f"Method {name!r} has {code_count} Code attributes (max 1)",
                loc,
            )

    exc_count = sum(1 for a in mi.attributes if isinstance(a, ExceptionsAttr))
    if exc_count > 1:
        dc.add(
            Severity.ERROR,
            Category.METHOD,
            f"Method {name!r} has {exc_count} Exceptions attributes (max 1)",
            loc,
        )

    for attr in mi.attributes:
        if isinstance(attr, CodeAttr):
            _verify_code(attr, cf, dc, class_name, name, desc)

    _verify_attr_versions(mi.attributes, major, loc, dc)


# ── Code attribute verification ───────────────────────────────────────

_FIELD_OPS = frozenset({InsnInfoType.GETFIELD, InsnInfoType.PUTFIELD, InsnInfoType.GETSTATIC, InsnInfoType.PUTSTATIC})
_METHOD_OPS = frozenset({InsnInfoType.INVOKEVIRTUAL, InsnInfoType.INVOKESPECIAL, InsnInfoType.INVOKESTATIC})
_CLASS_OPS = frozenset({InsnInfoType.NEW, InsnInfoType.CHECKCAST, InsnInfoType.INSTANCEOF, InsnInfoType.ANEWARRAY})


def _verify_code(
    code: CodeAttr,
    cf: ClassFile,
    dc: _Collector,
    class_name: str | None,
    method_name: str | None,
    method_desc: str | None,
) -> None:
    cp = cf.constant_pool
    major = cf.major_version
    loc = Location(class_name=class_name, method_name=method_name, method_descriptor=method_desc)

    if code.code_length <= 0:
        dc.add(Severity.ERROR, Category.CODE, "code_length must be > 0", loc)
    if code.code_length > 65535:
        dc.add(Severity.ERROR, Category.CODE, f"code_length {code.code_length} exceeds 65535", loc)
    if code.max_stacks < 0 or code.max_stacks > 65535:
        dc.add(Severity.ERROR, Category.CODE, f"max_stack {code.max_stacks} out of range [0, 65535]", loc)
    if code.max_locals < 0 or code.max_locals > 65535:
        dc.add(Severity.ERROR, Category.CODE, f"max_locals {code.max_locals} out of range [0, 65535]", loc)

    if not code.code:
        return

    valid_offsets: set[int] = {insn.bytecode_offset for insn in code.code}

    for insn in code.code:
        insn_loc = Location(
            class_name=class_name,
            method_name=method_name,
            method_descriptor=method_desc,
            bytecode_offset=insn.bytecode_offset,
        )

        # Branch target validation.
        if isinstance(insn, Branch):
            target = insn.bytecode_offset + insn.offset
            if target not in valid_offsets:
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"Branch at offset {insn.bytecode_offset} targets invalid offset {target}",
                    insn_loc,
                )
        elif isinstance(insn, BranchW):
            target = insn.bytecode_offset + insn.offset
            if target not in valid_offsets:
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"Wide branch at offset {insn.bytecode_offset} targets invalid offset {target}",
                    insn_loc,
                )
        elif isinstance(insn, LookupSwitch):
            default_target = insn.bytecode_offset + insn.default
            if default_target not in valid_offsets:
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"lookupswitch default targets invalid offset {default_target}",
                    insn_loc,
                )
            for pair in insn.pairs:
                t = insn.bytecode_offset + pair.offset
                if t not in valid_offsets:
                    dc.add(
                        Severity.ERROR,
                        Category.CODE,
                        f"lookupswitch case {pair.match} targets invalid offset {t}",
                        insn_loc,
                    )
        elif isinstance(insn, TableSwitch):
            default_target = insn.bytecode_offset + insn.default
            if default_target not in valid_offsets:
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"tableswitch default targets invalid offset {default_target}",
                    insn_loc,
                )
            for j, off in enumerate(insn.offsets):
                t = insn.bytecode_offset + off
                if t not in valid_offsets:
                    dc.add(
                        Severity.ERROR,
                        Category.CODE,
                        f"tableswitch case {insn.low + j} targets invalid offset {t}",
                        insn_loc,
                    )

    # Exception handlers.
    for eh in code.exception_table:
        eh_loc = Location(class_name=class_name, method_name=method_name, method_descriptor=method_desc)
        if eh.start_pc not in valid_offsets:
            dc.add(
                Severity.ERROR,
                Category.CODE,
                f"Exception handler start_pc {eh.start_pc} is not a valid instruction offset",
                eh_loc,
            )
        if eh.end_pc not in valid_offsets and eh.end_pc != code.code_length:
            dc.add(
                Severity.ERROR,
                Category.CODE,
                f"Exception handler end_pc {eh.end_pc} is not a valid offset or code_length",
                eh_loc,
            )
        if eh.start_pc >= eh.end_pc:
            dc.add(
                Severity.ERROR,
                Category.CODE,
                f"Exception handler start_pc ({eh.start_pc}) must be < end_pc ({eh.end_pc})",
                eh_loc,
            )
        if eh.handler_pc not in valid_offsets:
            dc.add(
                Severity.ERROR,
                Category.CODE,
                f"Exception handler handler_pc {eh.handler_pc} is not a valid instruction offset",
                eh_loc,
            )
        if eh.catch_type != 0 and not isinstance(_cp_entry(cp, eh.catch_type), ClassInfo):
            dc.add(
                Severity.ERROR,
                Category.CODE,
                f"Exception handler catch_type {eh.catch_type} does not point to CONSTANT_Class",
                eh_loc,
            )

    # CP references in instructions.
    _verify_code_cp_refs(code, cp, major, dc, class_name, method_name, method_desc)

    # Nested attribute versioning.
    _verify_attr_versions(code.attributes, major, loc, dc)


def _verify_code_cp_refs(
    code: CodeAttr,
    cp: list[ConstantPoolInfo | None],
    major: int,
    dc: _Collector,
    class_name: str | None,
    method_name: str | None,
    method_desc: str | None,
) -> None:
    """Validate CP references in bytecode instructions."""
    for insn in code.code:
        loc = Location(
            class_name=class_name,
            method_name=method_name,
            method_descriptor=method_desc,
            bytecode_offset=insn.bytecode_offset,
        )

        if isinstance(insn, ConstPoolIndex):
            entry = _cp_entry(cp, insn.index)
            if entry is None:
                dc.add(Severity.ERROR, Category.CODE, f"{insn.type.name} references invalid CP index {insn.index}", loc)
                continue

            if insn.type in _FIELD_OPS:
                if not isinstance(entry, FieldrefInfo):
                    dc.add(
                        Severity.ERROR,
                        Category.CODE,
                        f"{insn.type.name} CP#{insn.index} expected FieldrefInfo, got {type(entry).__name__}",
                        loc,
                    )
            elif insn.type in _METHOD_OPS:
                if major >= 52:
                    if not isinstance(entry, (MethodrefInfo, InterfaceMethodrefInfo)):
                        msg = (
                            f"{insn.type.name} CP#{insn.index} expected "
                            f"Methodref/InterfaceMethodref, got {type(entry).__name__}"
                        )
                        dc.add(Severity.ERROR, Category.CODE, msg, loc)
                elif not isinstance(entry, MethodrefInfo):
                    dc.add(
                        Severity.ERROR,
                        Category.CODE,
                        f"{insn.type.name} CP#{insn.index} expected MethodrefInfo, got {type(entry).__name__}",
                        loc,
                    )
            elif insn.type in _CLASS_OPS:
                if not isinstance(entry, ClassInfo):
                    dc.add(
                        Severity.ERROR,
                        Category.CODE,
                        f"{insn.type.name} CP#{insn.index} expected ClassInfo, got {type(entry).__name__}",
                        loc,
                    )
            elif insn.type == InsnInfoType.LDC_W:
                _verify_ldc_entry(entry, insn.index, major, loc, dc)
            elif insn.type == InsnInfoType.LDC2_W:
                if not isinstance(entry, (LongInfo, DoubleInfo)):
                    dc.add(
                        Severity.ERROR,
                        Category.CODE,
                        f"LDC2_W CP#{insn.index} expected Long/Double, got {type(entry).__name__}",
                        loc,
                    )

        elif isinstance(insn, LocalIndex) and insn.type == InsnInfoType.LDC:
            entry = _cp_entry(cp, insn.index)
            if entry is None:
                dc.add(Severity.ERROR, Category.CODE, f"LDC references invalid CP index {insn.index}", loc)
            else:
                _verify_ldc_entry(entry, insn.index, major, loc, dc)

        elif isinstance(insn, InvokeInterface):
            entry = _cp_entry(cp, insn.index)
            if entry is None:
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"INVOKEINTERFACE references invalid CP index {insn.index}",
                    loc,
                )
            elif not isinstance(entry, InterfaceMethodrefInfo):
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"INVOKEINTERFACE CP#{insn.index} expected InterfaceMethodrefInfo, got {type(entry).__name__}",
                    loc,
                )

        elif isinstance(insn, InvokeDynamic):
            entry = _cp_entry(cp, insn.index)
            if entry is None:
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"INVOKEDYNAMIC references invalid CP index {insn.index}",
                    loc,
                )
            elif not isinstance(entry, InvokeDynamicInfo):
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"INVOKEDYNAMIC CP#{insn.index} expected InvokeDynamicInfo, got {type(entry).__name__}",
                    loc,
                )

        elif isinstance(insn, MultiANewArray):
            entry = _cp_entry(cp, insn.index)
            if entry is None:
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"MULTIANEWARRAY references invalid CP index {insn.index}",
                    loc,
                )
            elif not isinstance(entry, ClassInfo):
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"MULTIANEWARRAY CP#{insn.index} expected ClassInfo, got {type(entry).__name__}",
                    loc,
                )
            if insn.dimensions < 1:
                dc.add(
                    Severity.ERROR,
                    Category.CODE,
                    f"MULTIANEWARRAY dimensions must be >= 1, got {insn.dimensions}",
                    loc,
                )


def _verify_ldc_entry(entry: ConstantPoolInfo, idx: int, major: int, loc: Location, dc: _Collector) -> None:
    """Validate that an LDC/LDC_W entry is a valid loadable type."""
    if isinstance(entry, (IntegerInfo, FloatInfo, StringInfo)):
        return
    if isinstance(entry, ClassInfo):
        if major < 49:
            dc.add(
                Severity.ERROR,
                Category.CODE,
                f"LDC CP#{idx} ClassInfo requires version >= 49, got {major}",
                loc,
            )
        return
    if isinstance(entry, (MethodHandleInfo, MethodTypeInfo)):
        if major < 51:
            dc.add(
                Severity.ERROR,
                Category.CODE,
                f"LDC CP#{idx} {type(entry).__name__} requires version >= 51, got {major}",
                loc,
            )
        return
    if isinstance(entry, DynamicInfo):
        if major < 55:
            dc.add(
                Severity.ERROR,
                Category.CODE,
                f"LDC CP#{idx} DynamicInfo requires version >= 55, got {major}",
                loc,
            )
        return
    dc.add(
        Severity.ERROR,
        Category.CODE,
        f"LDC CP#{idx} has non-loadable type {type(entry).__name__}",
        loc,
    )


# ── Main entry: verify_classfile ──────────────────────────────────────


def verify_classfile(cf: ClassFile, *, fail_fast: bool = False) -> list[Diagnostic]:
    """Validate a parsed ``ClassFile`` against JVM spec structural rules.

    Returns a list of :class:`Diagnostic` objects.  By default all issues
    are collected; set *fail_fast* to ``True`` to raise
    :class:`FailFastError` on the first ERROR-severity diagnostic.
    """
    dc = _Collector(fail_fast)
    class_name = _resolve_class_name(cf)
    loc = Location(class_name=class_name)

    _verify_magic_version(cf, dc, loc)
    _verify_constant_pool(cf, dc, class_name)
    _check_class_flags(cf.access_flags, loc, dc)
    _verify_class_structure(cf, dc, class_name)

    is_interface = ClassAccessFlag.INTERFACE in cf.access_flags

    for fi in cf.fields:
        _verify_field(fi, cf, dc, class_name, is_interface)

    for mi in cf.methods:
        _verify_method(mi, cf, dc, class_name, is_interface)

    _verify_attr_versions(cf.attributes, cf.major_version, loc, dc)

    return dc.diagnostics


# ── ClassModel verification ───────────────────────────────────────────


def _verify_model_names(cm: ClassModel, dc: _Collector) -> None:
    loc = Location(class_name=cm.name)

    if not _is_valid_internal_name(cm.name):
        dc.add(Severity.ERROR, Category.CLASS_STRUCTURE, f"Invalid class name: {cm.name!r}", loc)

    if cm.super_name is not None:
        if not _is_valid_internal_name(cm.super_name):
            dc.add(Severity.ERROR, Category.CLASS_STRUCTURE, f"Invalid super class name: {cm.super_name!r}", loc)
    elif cm.name != "java/lang/Object":
        dc.add(Severity.WARNING, Category.CLASS_STRUCTURE, "No superclass (only valid for java/lang/Object)", loc)

    seen: set[str] = set()
    for iface in cm.interfaces:
        if not _is_valid_internal_name(iface):
            dc.add(Severity.ERROR, Category.CLASS_STRUCTURE, f"Invalid interface name: {iface!r}", loc)
        if iface in seen:
            dc.add(Severity.ERROR, Category.CLASS_STRUCTURE, f"Duplicate interface: {iface!r}", loc)
        seen.add(iface)


def _verify_model_duplicates(cm: ClassModel, dc: _Collector) -> None:
    loc = Location(class_name=cm.name)

    field_sigs: set[tuple[str, str]] = set()
    for fm in cm.fields:
        key = (fm.name, fm.descriptor)
        if key in field_sigs:
            dc.add(Severity.ERROR, Category.CLASS_STRUCTURE, f"Duplicate field: {fm.name} {fm.descriptor}", loc)
        field_sigs.add(key)

    method_sigs: set[tuple[str, str]] = set()
    for mm in cm.methods:
        key = (mm.name, mm.descriptor)
        if key in method_sigs:
            dc.add(Severity.ERROR, Category.CLASS_STRUCTURE, f"Duplicate method: {mm.name}{mm.descriptor}", loc)
        method_sigs.add(key)


def _verify_model_field(fm: FieldModel, cm: ClassModel, dc: _Collector) -> None:
    loc = Location(class_name=cm.name, field_name=fm.name)
    is_interface = ClassAccessFlag.INTERFACE in cm.access_flags

    if not _is_valid_unqualified_name(fm.name):
        dc.add(Severity.ERROR, Category.FIELD, f"Invalid field name: {fm.name!r}", loc)

    if not is_valid_field_descriptor(fm.descriptor):
        dc.add(Severity.ERROR, Category.DESCRIPTOR, f"Invalid field descriptor: {fm.descriptor!r}", loc)

    _check_field_flags(fm.access_flags, fm.name, is_interface, loc, dc)
    _verify_attr_versions(fm.attributes, cm.version[0], loc, dc)


def _verify_model_method(mm: MethodModel, cm: ClassModel, dc: _Collector) -> None:
    loc = Location(class_name=cm.name, method_name=mm.name, method_descriptor=mm.descriptor)
    is_interface = ClassAccessFlag.INTERFACE in cm.access_flags
    major = cm.version[0]

    if not _is_valid_method_name(mm.name):
        dc.add(Severity.ERROR, Category.METHOD, f"Invalid method name: {mm.name!r}", loc)

    if not is_valid_method_descriptor(mm.descriptor):
        dc.add(Severity.ERROR, Category.DESCRIPTOR, f"Invalid method descriptor: {mm.descriptor!r}", loc)

    _check_method_flags(mm.access_flags, mm.name, is_interface, major, loc, dc)

    if mm.name == "<clinit>" and mm.descriptor != "()V":
        dc.add(Severity.ERROR, Category.METHOD, f"<clinit> must have descriptor ()V, got {mm.descriptor!r}", loc)

    is_abstract = MethodAccessFlag.ABSTRACT in mm.access_flags
    is_native = MethodAccessFlag.NATIVE in mm.access_flags

    if is_abstract or is_native:
        if mm.code is not None:
            label = "ABSTRACT" if is_abstract else "NATIVE"
            dc.add(Severity.ERROR, Category.METHOD, f"{label} method {mm.name!r} must not have code", loc)
    else:
        if mm.code is None:
            dc.add(Severity.ERROR, Category.METHOD, f"Method {mm.name!r} must have code", loc)

    if mm.code is not None:
        _verify_model_code(mm.code, cm.name, mm.name, mm.descriptor, dc)

    _verify_attr_versions(mm.attributes, major, loc, dc)


def _verify_model_code(
    code: CodeModel,
    class_name: str,
    method_name: str,
    method_desc: str,
    dc: _Collector,
) -> None:
    loc = Location(class_name=class_name, method_name=method_name, method_descriptor=method_desc)

    if is_code_debug_info_stale(code):
        dc.add(
            Severity.WARNING,
            Category.CODE,
            "Code debug metadata is marked stale and will be stripped during lowering",
            loc,
        )

    if not code.instructions:
        dc.add(Severity.WARNING, Category.CODE, "Code has empty instruction list", loc)
        return

    # Collect label identities present in the instruction stream.
    labels_in_stream: set[int] = set()
    for item in code.instructions:
        if isinstance(item, Label):
            labels_in_stream.add(id(item))

    def _check_label(label: Label, context: str) -> None:
        if id(label) not in labels_in_stream:
            dc.add(Severity.ERROR, Category.CODE, f"{context} references label not in instruction stream", loc)

    for eh in code.exception_handlers:
        _check_label(eh.start, "Exception handler start")
        _check_label(eh.end, "Exception handler end")
        _check_label(eh.handler, "Exception handler handler")

    for ln in code.line_numbers:
        _check_label(ln.label, "Line number entry")

    for lv in code.local_variables:
        _check_label(lv.start, f"Local variable '{lv.name}' start")
        _check_label(lv.end, f"Local variable '{lv.name}' end")

    for lvt in code.local_variable_types:
        _check_label(lvt.start, f"Local variable type '{lvt.name}' start")
        _check_label(lvt.end, f"Local variable type '{lvt.name}' end")

    for item in code.instructions:
        if isinstance(item, BranchInsn):
            _check_label(item.target, f"{item.type.name} target")
        elif isinstance(item, LookupSwitchInsn):
            _check_label(item.default_target, "lookupswitch default")
            for match_val, label in item.pairs:
                _check_label(label, f"lookupswitch case {match_val}")
        elif isinstance(item, TableSwitchInsn):
            _check_label(item.default_target, "tableswitch default")
            for label in item.targets:
                _check_label(label, "tableswitch case")


def verify_classmodel(cm: ClassModel, *, fail_fast: bool = False) -> list[Diagnostic]:
    """Validate a ``ClassModel`` against structural and naming rules.

    Checks symbolic names, descriptors, access flags, code model structure
    (label validity, branch targets), and version-aware attribute rules.

    Returns a list of :class:`Diagnostic` objects.  By default all issues
    are collected; set *fail_fast* to ``True`` to raise
    :class:`FailFastError` on the first ERROR-severity diagnostic.
    """
    dc = _Collector(fail_fast)
    class_loc = Location(class_name=cm.name)

    _verify_model_names(cm, dc)
    _check_class_flags(cm.access_flags, class_loc, dc)
    if is_class_debug_info_stale(cm):
        dc.add(
            Severity.WARNING,
            Category.ATTRIBUTE,
            "Class debug metadata is marked stale and will be stripped during lowering",
            class_loc,
        )
    _verify_model_duplicates(cm, dc)

    for fm in cm.fields:
        _verify_model_field(fm, cm, dc)

    for mm in cm.methods:
        _verify_model_method(mm, cm, dc)

    _verify_attr_versions(cm.attributes, cm.version[0], class_loc, dc)

    return dc.diagnostics
