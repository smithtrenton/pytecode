"""Constant-pool management utilities for building and editing JVM class files.

Provides :class:`ConstantPoolBuilder`, a mutable accumulator that manages a JVM
constant pool with deduplication on insertion, symbol-table-style lookups, and
deterministic ordering.  It is the constant-pool component intended for use by
the editing model (#6) and emission layer (#12).
"""

from __future__ import annotations

import copy
from typing import TYPE_CHECKING

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
from .modified_utf8 import decode_modified_utf8, encode_modified_utf8

if TYPE_CHECKING:
    pass

# ---------------------------------------------------------------------------
# JVM constant-pool tag constants (§4.4)
# ---------------------------------------------------------------------------

_TAG_UTF8: int = 1
_TAG_INTEGER: int = 3
_TAG_FLOAT: int = 4
_TAG_LONG: int = 5
_TAG_DOUBLE: int = 6
_TAG_CLASS: int = 7
_TAG_STRING: int = 8
_TAG_FIELDREF: int = 9
_TAG_METHODREF: int = 10
_TAG_INTERFACE_METHODREF: int = 11
_TAG_NAME_AND_TYPE: int = 12
_TAG_METHOD_HANDLE: int = 15
_TAG_METHOD_TYPE: int = 16
_TAG_DYNAMIC: int = 17
_TAG_INVOKE_DYNAMIC: int = 18
_TAG_MODULE: int = 19
_TAG_PACKAGE: int = 20

# The JVM spec allows constant_pool_count up to 65535 (u2). Valid entry indexes
# are 1 … constant_pool_count-1. Long/Double entries occupy two slots, so the
# second slot index must also fit within that range.
_CP_MAX_SINGLE_INDEX: int = 65534  # max index for a single-slot entry
_CP_MAX_DOUBLE_INDEX: int = 65533  # max index for a double-slot entry (needs +1 gap)
_UTF8_MAX_BYTES: int = 65535


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

# Deduplication key: a tuple whose elements identify the *content* of an entry,
# ignoring its index and file offset.  Utf8 entries include a bytes element;
# all other entries contain only int elements.
type _CPKey = tuple[int | bytes, ...]


def _entry_key(entry: ConstantPoolInfo) -> _CPKey:
    """Return the deduplication key for *entry* based solely on its content."""
    if isinstance(entry, Utf8Info):
        return (_TAG_UTF8, entry.str_bytes)
    if isinstance(entry, IntegerInfo):
        return (_TAG_INTEGER, entry.value_bytes)
    if isinstance(entry, FloatInfo):
        return (_TAG_FLOAT, entry.value_bytes)
    if isinstance(entry, LongInfo):
        return (_TAG_LONG, entry.high_bytes, entry.low_bytes)
    if isinstance(entry, DoubleInfo):
        return (_TAG_DOUBLE, entry.high_bytes, entry.low_bytes)
    if isinstance(entry, ClassInfo):
        return (_TAG_CLASS, entry.name_index)
    if isinstance(entry, StringInfo):
        return (_TAG_STRING, entry.string_index)
    if isinstance(entry, FieldrefInfo):
        return (_TAG_FIELDREF, entry.class_index, entry.name_and_type_index)
    if isinstance(entry, MethodrefInfo):
        return (_TAG_METHODREF, entry.class_index, entry.name_and_type_index)
    if isinstance(entry, InterfaceMethodrefInfo):
        return (_TAG_INTERFACE_METHODREF, entry.class_index, entry.name_and_type_index)
    if isinstance(entry, NameAndTypeInfo):
        return (_TAG_NAME_AND_TYPE, entry.name_index, entry.descriptor_index)
    if isinstance(entry, MethodHandleInfo):
        return (_TAG_METHOD_HANDLE, entry.reference_kind, entry.reference_index)
    if isinstance(entry, MethodTypeInfo):
        return (_TAG_METHOD_TYPE, entry.descriptor_index)
    if isinstance(entry, DynamicInfo):
        return (_TAG_DYNAMIC, entry.bootstrap_method_attr_index, entry.name_and_type_index)
    if isinstance(entry, InvokeDynamicInfo):
        return (_TAG_INVOKE_DYNAMIC, entry.bootstrap_method_attr_index, entry.name_and_type_index)
    if isinstance(entry, ModuleInfo):
        return (_TAG_MODULE, entry.name_index)
    if isinstance(entry, PackageInfo):
        return (_TAG_PACKAGE, entry.name_index)
    raise ValueError(f"Unknown constant pool entry type: {type(entry).__name__}")


def _is_double_slot(entry: ConstantPoolInfo) -> bool:
    """Return *True* if *entry* is a Long or Double (occupies two CP slots)."""
    return isinstance(entry, (LongInfo, DoubleInfo))


def _copy_entry(entry: ConstantPoolInfo | None) -> ConstantPoolInfo | None:
    return copy.copy(entry) if entry is not None else None


def _require_pool_entry(
    pool: list[ConstantPoolInfo | None],
    index: int,
    *,
    context: str,
) -> ConstantPoolInfo:
    if index <= 0 or index >= len(pool):
        raise ValueError(f"{context} index {index} out of range [1, {len(pool) - 1}]")

    entry = pool[index]
    if entry is None:
        raise ValueError(f"{context} index {index} points to an empty constant-pool slot")

    return entry


def _validate_utf8_entry(entry: Utf8Info) -> None:
    if entry.length != len(entry.str_bytes):
        raise ValueError(
            f"Utf8Info length {entry.length} does not match payload size {len(entry.str_bytes)}"
        )
    if entry.length > _UTF8_MAX_BYTES:
        raise ValueError(
            f"Utf8Info payload exceeds JVM u2 length limit of {_UTF8_MAX_BYTES} bytes"
        )
    try:
        decode_modified_utf8(entry.str_bytes)
    except UnicodeDecodeError as exc:
        raise ValueError(f"Utf8Info contains invalid modified UTF-8: {exc.reason}") from exc


def _method_handle_member_name(
    pool: list[ConstantPoolInfo | None],
    reference_entry: MethodrefInfo | InterfaceMethodrefInfo,
) -> str:
    nat_entry = _require_pool_entry(
        pool,
        reference_entry.name_and_type_index,
        context="MethodHandle name_and_type",
    )
    if not isinstance(nat_entry, NameAndTypeInfo):
        raise ValueError(
            "MethodHandle reference target must point to a Methodref/InterfaceMethodref "
            "whose name_and_type_index resolves to CONSTANT_NameAndType"
        )

    name_entry = _require_pool_entry(pool, nat_entry.name_index, context="MethodHandle member name")
    if not isinstance(name_entry, Utf8Info):
        raise ValueError("MethodHandle member name must resolve to CONSTANT_Utf8")

    try:
        return decode_modified_utf8(name_entry.str_bytes)
    except UnicodeDecodeError as exc:
        raise ValueError(f"MethodHandle member name is not valid modified UTF-8: {exc.reason}") from exc


def _validate_method_handle(
    pool: list[ConstantPoolInfo | None],
    reference_kind: int,
    reference_index: int,
) -> None:
    if not 1 <= reference_kind <= 9:
        raise ValueError(f"reference_kind must be in range [1, 9], got {reference_kind}")

    target = _require_pool_entry(pool, reference_index, context="MethodHandle reference")

    if reference_kind in (1, 2, 3, 4):
        if not isinstance(target, FieldrefInfo):
            raise ValueError(
                f"reference_kind {reference_kind} requires CONSTANT_Fieldref, got {type(target).__name__}"
            )
        return

    if reference_kind in (5, 8):
        if not isinstance(target, MethodrefInfo):
            raise ValueError(
                f"reference_kind {reference_kind} requires CONSTANT_Methodref, got {type(target).__name__}"
            )
    elif reference_kind in (6, 7):
        if not isinstance(target, (MethodrefInfo, InterfaceMethodrefInfo)):
            raise ValueError(
                "reference_kind "
                f"{reference_kind} requires CONSTANT_Methodref or CONSTANT_InterfaceMethodref, "
                f"got {type(target).__name__}"
            )
    else:
        if not isinstance(target, InterfaceMethodrefInfo):
            raise ValueError(
                f"reference_kind {reference_kind} requires CONSTANT_InterfaceMethodref, "
                f"got {type(target).__name__}"
            )

    member_name = _method_handle_member_name(pool, target)
    if reference_kind == 8:
        if member_name != "<init>":
            raise ValueError("reference_kind 8 (REF_newInvokeSpecial) must target a <init> method")
        return

    if member_name in {"<init>", "<clinit>"}:
        raise ValueError(
            f"reference_kind {reference_kind} cannot target special method {member_name!r}"
        )


def _validate_import_pool(pool: list[ConstantPoolInfo | None]) -> None:
    if not pool:
        raise ValueError("constant pool must include index 0")
    if pool[0] is not None:
        raise ValueError("constant pool index 0 must be None")

    index = 1
    while index < len(pool):
        entry = pool[index]
        if entry is None:
            raise ValueError(
                f"constant pool index {index} is None but not reserved as a Long/Double gap"
            )
        if entry.index != index:
            raise ValueError(
                f"constant pool entry at position {index} reports mismatched index {entry.index}"
            )

        if isinstance(entry, Utf8Info):
            _validate_utf8_entry(entry)
        elif isinstance(entry, MethodHandleInfo):
            _validate_method_handle(pool, entry.reference_kind, entry.reference_index)

        if _is_double_slot(entry):
            gap_index = index + 1
            if gap_index >= len(pool) or pool[gap_index] is not None:
                raise ValueError(
                    f"double-slot entry at index {index} must be followed by a None gap slot"
                )
            index += 2
            continue

        index += 1


# ---------------------------------------------------------------------------
# ConstantPoolBuilder
# ---------------------------------------------------------------------------


class ConstantPoolBuilder:
    """Builds a JVM constant pool with deduplication and symbol-table lookups.

    Entries are assigned indexes in insertion order starting at 1 (index 0 is
    always *None* per the JVM spec).  Inserting an already-present entry returns
    the existing index without growing the pool.

    High-level convenience methods (e.g. :meth:`add_class`) automatically create
    any prerequisite entries (e.g. the ``CONSTANT_Utf8`` name string) and
    deduplicate the entire chain.

    Use :meth:`from_pool` to seed from a parsed :attr:`~pytecode.info.ClassFile.constant_pool`
    list; new entries appended afterwards will not disturb existing indexes.

    Use :meth:`build` to export the pool as a ``list[ConstantPoolInfo | None]``
    compatible with :attr:`~pytecode.info.ClassFile.constant_pool`.
    """

    def __init__(self) -> None:
        # Index 0 is always None per the JVM spec.
        self._pool: list[ConstantPoolInfo | None] = [None]
        self._next_index: int = 1
        # Content-keyed dedup map: _entry_key(entry) → CP index.
        self._key_to_index: dict[_CPKey, int] = {}
        # Fast reverse lookup for Utf8 entries: str_bytes → CP index.
        self._utf8_to_index: dict[bytes, int] = {}

    # ------------------------------------------------------------------
    # Construction
    # ------------------------------------------------------------------

    @classmethod
    def from_pool(cls, pool: list[ConstantPoolInfo | None]) -> ConstantPoolBuilder:
        """Seed a new builder from an existing parsed constant pool.

        The original indexes are preserved so that all existing CP references in
        the classfile remain valid.  New entries appended after import receive
        fresh indexes starting immediately after the last imported entry.

        *pool* is the ``list[ConstantPoolInfo | None]`` from
        :attr:`~pytecode.info.ClassFile.constant_pool` (index 0 is *None*).
        """
        builder = cls()
        _validate_import_pool(pool)
        # Deep-copy to ensure the builder owns its data.
        builder._pool = [_copy_entry(entry) for entry in pool]
        builder._next_index = len(pool)
        # Rebuild dedup maps from the copied entries.
        for entry in builder._pool:
            if entry is None:
                continue
            key = _entry_key(entry)
            builder._key_to_index.setdefault(key, entry.index)
            if isinstance(entry, Utf8Info):
                builder._utf8_to_index.setdefault(entry.str_bytes, entry.index)
        return builder

    # ------------------------------------------------------------------
    # Internal allocation
    # ------------------------------------------------------------------

    def _allocate(self, entry: ConstantPoolInfo) -> int:
        """Assign *entry* the next available CP index, or return the existing index.

        The entry's ``index`` field is updated to the allocated index and
        ``offset`` is set to ``0`` (no source-file position for builder entries).
        Double-slot entries (Long, Double) automatically consume two slots.
        """
        key = _entry_key(entry)
        if key in self._key_to_index:
            return self._key_to_index[key]

        double = _is_double_slot(entry)
        limit = _CP_MAX_DOUBLE_INDEX if double else _CP_MAX_SINGLE_INDEX
        if self._next_index > limit:
            raise ValueError(
                f"Constant pool overflow: cannot add {'double-slot ' if double else ''}"
                f"entry at index {self._next_index} (maximum is {limit})"
            )

        index = self._next_index
        entry.index = index
        entry.offset = 0
        self._pool.append(entry)
        self._key_to_index[key] = index

        if isinstance(entry, Utf8Info):
            self._utf8_to_index[entry.str_bytes] = index

        if double:
            self._pool.append(None)  # Long/Double second slot is always None
            self._next_index += 2
        else:
            self._next_index += 1

        return index

    def _validate_entry(self, entry: ConstantPoolInfo) -> None:
        if isinstance(entry, Utf8Info):
            _validate_utf8_entry(entry)
        elif isinstance(entry, MethodHandleInfo):
            _validate_method_handle(self._pool, entry.reference_kind, entry.reference_index)

    # ------------------------------------------------------------------
    # Low-level entry insertion
    # ------------------------------------------------------------------

    def add_entry(self, entry: ConstantPoolInfo) -> int:
        """Add any :class:`~pytecode.constant_pool.ConstantPoolInfo` entry directly.

        Returns the CP index of the entry (existing index if an identical entry
        is already present, otherwise a newly allocated index).  The caller's
        object is never mutated.
        """
        entry_copy = copy.copy(entry)
        self._validate_entry(entry_copy)
        return self._allocate(entry_copy)

    # ------------------------------------------------------------------
    # Utf8 and primitive entries
    # ------------------------------------------------------------------

    def add_utf8(self, value: str) -> int:
        """Add a ``CONSTANT_Utf8`` entry for *value*.  Returns the CP index."""
        encoded = encode_modified_utf8(value)
        if len(encoded) > _UTF8_MAX_BYTES:
            raise ValueError(
                f"Modified UTF-8 payload exceeds JVM u2 length limit of {_UTF8_MAX_BYTES} bytes"
            )

        # Fast path: Utf8 lookup bypasses the general key dict.
        existing = self._utf8_to_index.get(encoded)
        if existing is not None:
            return existing
        entry = Utf8Info(index=0, offset=0, tag=_TAG_UTF8, length=len(encoded), str_bytes=encoded)
        return self._allocate(entry)

    def add_integer(self, value: int) -> int:
        """Add a ``CONSTANT_Integer`` entry.  *value* is the raw 4-byte integer bits."""
        entry = IntegerInfo(index=0, offset=0, tag=_TAG_INTEGER, value_bytes=value)
        return self._allocate(entry)

    def add_float(self, raw_bits: int) -> int:
        """Add a ``CONSTANT_Float`` entry.  *raw_bits* is the raw IEEE 754 bit pattern."""
        entry = FloatInfo(index=0, offset=0, tag=_TAG_FLOAT, value_bytes=raw_bits)
        return self._allocate(entry)

    def add_long(self, high: int, low: int) -> int:
        """Add a ``CONSTANT_Long`` entry (double-slot).  Returns the CP index."""
        entry = LongInfo(index=0, offset=0, tag=_TAG_LONG, high_bytes=high, low_bytes=low)
        return self._allocate(entry)

    def add_double(self, high: int, low: int) -> int:
        """Add a ``CONSTANT_Double`` entry (double-slot).  Returns the CP index."""
        entry = DoubleInfo(index=0, offset=0, tag=_TAG_DOUBLE, high_bytes=high, low_bytes=low)
        return self._allocate(entry)

    # ------------------------------------------------------------------
    # Compound entries (auto-create prerequisites)
    # ------------------------------------------------------------------

    def add_class(self, name: str) -> int:
        """Add a ``CONSTANT_Class`` entry for the class/interface *name*.

        *name* must be in JVM internal form (e.g. ``java/lang/Object``).
        The required ``CONSTANT_Utf8`` name entry is created automatically.
        """
        name_index = self.add_utf8(name)
        entry = ClassInfo(index=0, offset=0, tag=_TAG_CLASS, name_index=name_index)
        return self._allocate(entry)

    def add_string(self, value: str) -> int:
        """Add a ``CONSTANT_String`` entry for *value*.

        The required ``CONSTANT_Utf8`` entry is created automatically.
        """
        string_index = self.add_utf8(value)
        entry = StringInfo(index=0, offset=0, tag=_TAG_STRING, string_index=string_index)
        return self._allocate(entry)

    def add_name_and_type(self, name: str, descriptor: str) -> int:
        """Add a ``CONSTANT_NameAndType`` entry.

        Both ``CONSTANT_Utf8`` entries (name and descriptor) are created automatically.
        """
        name_index = self.add_utf8(name)
        descriptor_index = self.add_utf8(descriptor)
        entry = NameAndTypeInfo(
            index=0,
            offset=0,
            tag=_TAG_NAME_AND_TYPE,
            name_index=name_index,
            descriptor_index=descriptor_index,
        )
        return self._allocate(entry)

    def add_fieldref(self, class_name: str, field_name: str, descriptor: str) -> int:
        """Add a ``CONSTANT_Fieldref`` entry.

        Prerequisite ``CONSTANT_Class`` and ``CONSTANT_NameAndType`` entries
        (and their ``CONSTANT_Utf8`` dependencies) are created automatically.
        """
        class_index = self.add_class(class_name)
        nat_index = self.add_name_and_type(field_name, descriptor)
        entry = FieldrefInfo(
            index=0,
            offset=0,
            tag=_TAG_FIELDREF,
            class_index=class_index,
            name_and_type_index=nat_index,
        )
        return self._allocate(entry)

    def add_methodref(self, class_name: str, method_name: str, descriptor: str) -> int:
        """Add a ``CONSTANT_Methodref`` entry.

        Prerequisite entries are created automatically.
        """
        class_index = self.add_class(class_name)
        nat_index = self.add_name_and_type(method_name, descriptor)
        entry = MethodrefInfo(
            index=0,
            offset=0,
            tag=_TAG_METHODREF,
            class_index=class_index,
            name_and_type_index=nat_index,
        )
        return self._allocate(entry)

    def add_interface_methodref(self, class_name: str, method_name: str, descriptor: str) -> int:
        """Add a ``CONSTANT_InterfaceMethodref`` entry.

        Prerequisite entries are created automatically.
        """
        class_index = self.add_class(class_name)
        nat_index = self.add_name_and_type(method_name, descriptor)
        entry = InterfaceMethodrefInfo(
            index=0,
            offset=0,
            tag=_TAG_INTERFACE_METHODREF,
            class_index=class_index,
            name_and_type_index=nat_index,
        )
        return self._allocate(entry)

    # ------------------------------------------------------------------
    # Remaining entry types
    # ------------------------------------------------------------------

    def add_method_handle(self, reference_kind: int, reference_index: int) -> int:
        """Add a ``CONSTANT_MethodHandle`` entry.

        *reference_kind* must be a valid JVM reference-kind value (1–9).
        *reference_index* must already refer to a compatible existing CP entry.
        """
        _validate_method_handle(self._pool, reference_kind, reference_index)
        entry = MethodHandleInfo(
            index=0,
            offset=0,
            tag=_TAG_METHOD_HANDLE,
            reference_kind=reference_kind,
            reference_index=reference_index,
        )
        return self._allocate(entry)

    def add_method_type(self, descriptor: str) -> int:
        """Add a ``CONSTANT_MethodType`` entry.

        The required ``CONSTANT_Utf8`` descriptor entry is created automatically.
        """
        descriptor_index = self.add_utf8(descriptor)
        entry = MethodTypeInfo(index=0, offset=0, tag=_TAG_METHOD_TYPE, descriptor_index=descriptor_index)
        return self._allocate(entry)

    def add_dynamic(self, bootstrap_method_attr_index: int, name: str, descriptor: str) -> int:
        """Add a ``CONSTANT_Dynamic`` entry.

        *bootstrap_method_attr_index* references an entry in the ``BootstrapMethods``
        attribute.  The ``CONSTANT_NameAndType`` entry (and its Utf8 dependencies)
        are created automatically.
        """
        nat_index = self.add_name_and_type(name, descriptor)
        entry = DynamicInfo(
            index=0,
            offset=0,
            tag=_TAG_DYNAMIC,
            bootstrap_method_attr_index=bootstrap_method_attr_index,
            name_and_type_index=nat_index,
        )
        return self._allocate(entry)

    def add_invoke_dynamic(self, bootstrap_method_attr_index: int, name: str, descriptor: str) -> int:
        """Add a ``CONSTANT_InvokeDynamic`` entry.

        *bootstrap_method_attr_index* references an entry in the ``BootstrapMethods``
        attribute.  The ``CONSTANT_NameAndType`` entry (and its Utf8 dependencies)
        are created automatically.
        """
        nat_index = self.add_name_and_type(name, descriptor)
        entry = InvokeDynamicInfo(
            index=0,
            offset=0,
            tag=_TAG_INVOKE_DYNAMIC,
            bootstrap_method_attr_index=bootstrap_method_attr_index,
            name_and_type_index=nat_index,
        )
        return self._allocate(entry)

    def add_module(self, name: str) -> int:
        """Add a ``CONSTANT_Module`` entry for the module *name*.

        The required ``CONSTANT_Utf8`` name entry is created automatically.
        """
        name_index = self.add_utf8(name)
        entry = ModuleInfo(index=0, offset=0, tag=_TAG_MODULE, name_index=name_index)
        return self._allocate(entry)

    def add_package(self, name: str) -> int:
        """Add a ``CONSTANT_Package`` entry for the package *name* (internal form).

        The required ``CONSTANT_Utf8`` name entry is created automatically.
        """
        name_index = self.add_utf8(name)
        entry = PackageInfo(index=0, offset=0, tag=_TAG_PACKAGE, name_index=name_index)
        return self._allocate(entry)

    # ------------------------------------------------------------------
    # Lookups
    # ------------------------------------------------------------------

    def get(self, index: int) -> ConstantPoolInfo | None:
        """Return the entry at *index*, or *None* for index 0 or double-slot gaps.

        Returns a defensive copy of the stored entry.

        Raises :exc:`IndexError` if *index* is out of range.
        """
        if index < 0 or index >= len(self._pool):
            raise IndexError(f"CP index {index} out of range [0, {len(self._pool) - 1}]")
        return _copy_entry(self._pool[index])

    def find_utf8(self, value: str) -> int | None:
        """Return the CP index of a ``CONSTANT_Utf8`` entry for *value*, or *None*."""
        return self._utf8_to_index.get(encode_modified_utf8(value))

    def find_class(self, name: str) -> int | None:
        """Return the CP index of a ``CONSTANT_Class`` entry for *name*, or *None*.

        *name* is in JVM internal form (e.g. ``java/lang/Object``).
        """
        utf8_idx = self.find_utf8(name)
        if utf8_idx is None:
            return None
        key: _CPKey = (_TAG_CLASS, utf8_idx)
        return self._key_to_index.get(key)

    def find_name_and_type(self, name: str, descriptor: str) -> int | None:
        """Return the CP index of a ``CONSTANT_NameAndType`` entry, or *None*."""
        name_idx = self.find_utf8(name)
        if name_idx is None:
            return None
        desc_idx = self.find_utf8(descriptor)
        if desc_idx is None:
            return None
        key: _CPKey = (_TAG_NAME_AND_TYPE, name_idx, desc_idx)
        return self._key_to_index.get(key)

    def resolve_utf8(self, index: int) -> str:
        """Decode the ``CONSTANT_Utf8`` entry at *index* to a Python string.

        Raises :exc:`ValueError` if the entry at *index* is not a ``CONSTANT_Utf8``.
        """
        entry = self.get(index)
        if not isinstance(entry, Utf8Info):
            raise ValueError(f"CP index {index} is not a CONSTANT_Utf8 entry: {type(entry).__name__}")
        return decode_modified_utf8(entry.str_bytes)

    # ------------------------------------------------------------------
    # Output
    # ------------------------------------------------------------------

    def build(self) -> list[ConstantPoolInfo | None]:
        """Return a copy of the constant pool as a spec-format list.

        The returned list has the same structure as
        :attr:`~pytecode.info.ClassFile.constant_pool`: index 0 is *None*,
        Long/Double second slots are *None*, and all other indexes hold a
        copy of a :class:`~pytecode.constant_pool.ConstantPoolInfo` instance.
        Use :attr:`count` as the ``constant_pool_count`` field when serializing.
        """
        return [_copy_entry(entry) for entry in self._pool]

    @property
    def count(self) -> int:
        """The ``constant_pool_count`` field value (number of slots + 1)."""
        return self._next_index

    def __len__(self) -> int:
        """Number of logical entries (excludes index-0 placeholder and double-slot gaps)."""
        return sum(1 for e in self._pool if e is not None)
