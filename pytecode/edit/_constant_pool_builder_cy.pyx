# cython: boundscheck=False, wraparound=False, cdivision=True
"""Constant-pool builder for JVM class files.

Provides ``ConstantPoolBuilder``, a mutable accumulator that manages a JVM
constant pool (§4.4) with deduplication on insertion, symbol-table-style
lookups, and deterministic ordering.
"""

from typing import TYPE_CHECKING

__all__ = ["ConstantPoolBuilder"]

from ..classfile.constant_pool import (
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
from ..classfile.modified_utf8 import decode_modified_utf8, encode_modified_utf8

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


def _entry_key(entry):
    """Return the deduplication key for *entry* based solely on its content."""
    entry_type = type(entry)
    if entry_type is Utf8Info:
        utf8 = entry
        return (_TAG_UTF8, utf8.str_bytes)
    if entry_type is IntegerInfo:
        integer = entry
        return (_TAG_INTEGER, integer.value_bytes)
    if entry_type is FloatInfo:
        float_info = entry
        return (_TAG_FLOAT, float_info.value_bytes)
    if entry_type is LongInfo:
        long_info = entry
        return (_TAG_LONG, long_info.high_bytes, long_info.low_bytes)
    if entry_type is DoubleInfo:
        double_info = entry
        return (_TAG_DOUBLE, double_info.high_bytes, double_info.low_bytes)
    if entry_type is ClassInfo:
        class_info = entry
        return (_TAG_CLASS, class_info.name_index)
    if entry_type is StringInfo:
        string_info = entry
        return (_TAG_STRING, string_info.string_index)
    if entry_type is FieldrefInfo:
        fieldref = entry
        return (_TAG_FIELDREF, fieldref.class_index, fieldref.name_and_type_index)
    if entry_type is MethodrefInfo:
        methodref = entry
        return (_TAG_METHODREF, methodref.class_index, methodref.name_and_type_index)
    if entry_type is InterfaceMethodrefInfo:
        interface_methodref = entry
        return (_TAG_INTERFACE_METHODREF, interface_methodref.class_index, interface_methodref.name_and_type_index)
    if entry_type is NameAndTypeInfo:
        name_and_type = entry
        return (_TAG_NAME_AND_TYPE, name_and_type.name_index, name_and_type.descriptor_index)
    if entry_type is MethodHandleInfo:
        method_handle = entry
        return (_TAG_METHOD_HANDLE, method_handle.reference_kind, method_handle.reference_index)
    if entry_type is MethodTypeInfo:
        method_type = entry
        return (_TAG_METHOD_TYPE, method_type.descriptor_index)
    if entry_type is DynamicInfo:
        dynamic = entry
        return (_TAG_DYNAMIC, dynamic.bootstrap_method_attr_index, dynamic.name_and_type_index)
    if entry_type is InvokeDynamicInfo:
        invoke_dynamic = entry
        return (_TAG_INVOKE_DYNAMIC, invoke_dynamic.bootstrap_method_attr_index, invoke_dynamic.name_and_type_index)
    if entry_type is ModuleInfo:
        module = entry
        return (_TAG_MODULE, module.name_index)
    if entry_type is PackageInfo:
        package = entry
        return (_TAG_PACKAGE, package.name_index)
    raise ValueError(f"Unknown constant pool entry type: {entry_type.__name__}")


def _is_double_slot(entry):
    """Return *True* if *entry* is a Long or Double (occupies two CP slots)."""
    return type(entry) in (LongInfo, DoubleInfo)


def _copy_pool_entry(entry):
    entry_type = type(entry)
    if entry_type is Utf8Info:
        utf8 = entry
        return Utf8Info(utf8.index, utf8.offset, utf8.tag, utf8.length, utf8.str_bytes)
    if entry_type is IntegerInfo:
        integer = entry
        return IntegerInfo(integer.index, integer.offset, integer.tag, integer.value_bytes)
    if entry_type is FloatInfo:
        float_info = entry
        return FloatInfo(float_info.index, float_info.offset, float_info.tag, float_info.value_bytes)
    if entry_type is LongInfo:
        long_info = entry
        return LongInfo(long_info.index, long_info.offset, long_info.tag, long_info.high_bytes, long_info.low_bytes)
    if entry_type is DoubleInfo:
        double_info = entry
        return DoubleInfo(
            double_info.index,
            double_info.offset,
            double_info.tag,
            double_info.high_bytes,
            double_info.low_bytes,
        )
    if entry_type is ClassInfo:
        class_info = entry
        return ClassInfo(class_info.index, class_info.offset, class_info.tag, class_info.name_index)
    if entry_type is StringInfo:
        string_info = entry
        return StringInfo(string_info.index, string_info.offset, string_info.tag, string_info.string_index)
    if entry_type is FieldrefInfo:
        fieldref = entry
        return FieldrefInfo(
            fieldref.index, fieldref.offset, fieldref.tag, fieldref.class_index, fieldref.name_and_type_index
        )
    if entry_type is MethodrefInfo:
        methodref = entry
        return MethodrefInfo(
            methodref.index,
            methodref.offset,
            methodref.tag,
            methodref.class_index,
            methodref.name_and_type_index,
        )
    if entry_type is InterfaceMethodrefInfo:
        interface_methodref = entry
        return InterfaceMethodrefInfo(
            interface_methodref.index,
            interface_methodref.offset,
            interface_methodref.tag,
            interface_methodref.class_index,
            interface_methodref.name_and_type_index,
        )
    if entry_type is NameAndTypeInfo:
        name_and_type = entry
        return NameAndTypeInfo(
            name_and_type.index,
            name_and_type.offset,
            name_and_type.tag,
            name_and_type.name_index,
            name_and_type.descriptor_index,
        )
    if entry_type is MethodHandleInfo:
        method_handle = entry
        return MethodHandleInfo(
            method_handle.index,
            method_handle.offset,
            method_handle.tag,
            method_handle.reference_kind,
            method_handle.reference_index,
        )
    if entry_type is MethodTypeInfo:
        method_type = entry
        return MethodTypeInfo(method_type.index, method_type.offset, method_type.tag, method_type.descriptor_index)
    if entry_type is DynamicInfo:
        dynamic = entry
        return DynamicInfo(
            dynamic.index,
            dynamic.offset,
            dynamic.tag,
            dynamic.bootstrap_method_attr_index,
            dynamic.name_and_type_index,
        )
    if entry_type is InvokeDynamicInfo:
        invoke_dynamic = entry
        return InvokeDynamicInfo(
            invoke_dynamic.index,
            invoke_dynamic.offset,
            invoke_dynamic.tag,
            invoke_dynamic.bootstrap_method_attr_index,
            invoke_dynamic.name_and_type_index,
        )
    if entry_type is ModuleInfo:
        module = entry
        return ModuleInfo(module.index, module.offset, module.tag, module.name_index)
    if entry_type is PackageInfo:
        package = entry
        return PackageInfo(package.index, package.offset, package.tag, package.name_index)
    raise ValueError(f"Unknown constant pool entry type: {entry_type.__name__}")


def _copy_entry(entry):
    return _copy_pool_entry(entry) if entry is not None else None


def _require_pool_entry(
    pool,
    index,
    *,
    context,
):
    if index <= 0 or index >= len(pool):
        raise ValueError(f"{context} index {index} out of range [1, {len(pool) - 1}]")

    entry = pool[index]
    if entry is None:
        raise ValueError(f"{context} index {index} points to an empty constant-pool slot")

    return entry


def _validate_utf8_entry(entry):
    if entry.length != len(entry.str_bytes):
        raise ValueError(f"Utf8Info length {entry.length} does not match payload size {len(entry.str_bytes)}")
    if entry.length > _UTF8_MAX_BYTES:
        raise ValueError(f"Utf8Info payload exceeds JVM u2 length limit of {_UTF8_MAX_BYTES} bytes")
    try:
        decode_modified_utf8(entry.str_bytes)
    except UnicodeDecodeError as exc:
        raise ValueError(f"Utf8Info contains invalid modified UTF-8: {exc.reason}") from exc


def _method_handle_member_name(
    pool,
    reference_entry,
):
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
    pool,
    reference_kind,
    reference_index,
):
    if not 1 <= reference_kind <= 9:
        raise ValueError(f"reference_kind must be in range [1, 9], got {reference_kind}")

    target = _require_pool_entry(pool, reference_index, context="MethodHandle reference")
    target_type = type(target)

    if reference_kind in (1, 2, 3, 4):
        if not isinstance(target, FieldrefInfo):
            raise ValueError(f"reference_kind {reference_kind} requires CONSTANT_Fieldref, got {target_type.__name__}")
        return

    if reference_kind in (5, 8):
        if not isinstance(target, MethodrefInfo):
            raise ValueError(f"reference_kind {reference_kind} requires CONSTANT_Methodref, got {target_type.__name__}")
    elif reference_kind in (6, 7):
        if not isinstance(target, (MethodrefInfo, InterfaceMethodrefInfo)):
            raise ValueError(
                "reference_kind "
                f"{reference_kind} requires CONSTANT_Methodref or CONSTANT_InterfaceMethodref, "
                f"got {target_type.__name__}"
            )
    else:
        if not isinstance(target, InterfaceMethodrefInfo):
            raise ValueError(
                f"reference_kind {reference_kind} requires CONSTANT_InterfaceMethodref, got {target_type.__name__}"
            )

    member_name = _method_handle_member_name(pool, target)
    if reference_kind == 8:
        if member_name != "<init>":
            raise ValueError("reference_kind 8 (REF_newInvokeSpecial) must target a <init> method")
        return

    if member_name in {"<init>", "<clinit>"}:
        raise ValueError(f"reference_kind {reference_kind} cannot target special method {member_name!r}")


def _validate_import_pool(pool):
    if not pool:
        raise ValueError("constant pool must include index 0")
    if pool[0] is not None:
        raise ValueError("constant pool index 0 must be None")

    cdef int index, gap_index
    index = 1
    while index < len(pool):
        entry = pool[index]
        if entry is None:
            raise ValueError(f"constant pool index {index} is None but not reserved as a Long/Double gap")
        if entry.index != index:
            raise ValueError(f"constant pool entry at position {index} reports mismatched index {entry.index}")

        if isinstance(entry, Utf8Info):
            _validate_utf8_entry(entry)
        elif isinstance(entry, MethodHandleInfo):
            _validate_method_handle(pool, entry.reference_kind, entry.reference_index)

        if _is_double_slot(entry):
            gap_index = index + 1
            if gap_index >= len(pool) or pool[gap_index] is not None:
                raise ValueError(f"double-slot entry at index {index} must be followed by a None gap slot")
            index += 2
            continue

        index += 1


# ---------------------------------------------------------------------------
# ConstantPoolBuilder
# ---------------------------------------------------------------------------


class ConstantPoolBuilder:
    """Mutable accumulator for building a JVM constant pool with deduplication.

    Entries are assigned indexes in insertion order starting at 1 (index 0 is
    always ``None`` per the JVM spec §4.1).  Inserting an already-present entry
    returns the existing index without growing the pool.

    High-level convenience methods (e.g. ``add_class``) automatically create
    any prerequisite entries (e.g. the ``CONSTANT_Utf8`` name string) and
    deduplicate the entire chain.

    Use ``from_pool`` to seed from a parsed ``ClassFile.constant_pool`` list;
    new entries appended afterwards will not disturb existing indexes.

    Use ``build`` to export the pool as a ``list[ConstantPoolInfo | None]``
    compatible with ``ClassFile.constant_pool``.
    """

    def __init__(self):
        """Initialize an empty constant pool with only the index-0 placeholder."""
        # Index 0 is always None per the JVM spec.
        self._pool = [None]
        self._next_index = 1
        # Content-keyed dedup map: _entry_key(entry) → CP index.
        self._key_to_index = {}
        # Fast reverse lookup for Utf8 entries: str_bytes → CP index.
        self._utf8_to_index = {}
        # Fast reverse lookup for Utf8 entries by decoded Python string.
        self._string_to_utf8_index = {}
        # Lazy decode cache keyed by CP index; bytes identity guards against live-entry mutation via peek().
        self._resolved_utf8_cache = {}
        # Lazy semantic reverse lookups used heavily by lowering.
        self._class_name_to_index = {}
        self._string_value_to_index = {}
        self._name_and_type_to_index = {}
        self._fieldref_to_index = {}
        self._methodref_to_index = {}
        self._interface_methodref_to_index = {}

    # ------------------------------------------------------------------
    # Construction
    # ------------------------------------------------------------------

    @classmethod
    def from_pool(
        cls,
        pool,
        *,
        skip_validation=False,
    ):
        """Seed a new builder from an existing parsed constant pool.

        The original indexes are preserved so that all existing CP references
        in the class file remain valid.  New entries appended after import
        receive fresh indexes starting immediately after the last imported
        entry.

        Args:
            pool: Constant pool list where index 0 is ``None`` and each
                subsequent element is a ``ConstantPoolInfo`` or ``None`` for
                double-slot gaps.

        Returns:
            A new builder pre-populated with copies of the imported entries.

        Raises:
            ValueError: If the pool structure is invalid (e.g. missing
                index-0 placeholder, mismatched entry indexes, or malformed
                entries).
        """
        builder = cls()
        if not skip_validation:
            _validate_import_pool(pool)
        builder._pool = [_copy_entry(entry) for entry in pool]
        builder._next_index = len(pool)
        for entry in builder._pool:
            if entry is None:
                continue
            key = _entry_key(entry)
            builder._key_to_index.setdefault(key, entry.index)
            if type(entry) is Utf8Info:
                builder._utf8_to_index.setdefault(entry.str_bytes, entry.index)
        return builder

    def clone(self):
        """Return a fast defensive copy of this builder.

        The clone preserves the current pool contents, indexes, and dedup maps
        without re-validating a pool that was already validated on import or
        incremental insertion.
        """
        clone = type(self)()
        clone._pool = [_copy_entry(entry) for entry in self._pool]
        clone._next_index = self._next_index
        clone._key_to_index = dict(self._key_to_index)
        clone._utf8_to_index = dict(self._utf8_to_index)
        clone._string_to_utf8_index = dict(self._string_to_utf8_index)
        clone._resolved_utf8_cache = dict(self._resolved_utf8_cache)
        clone._class_name_to_index = dict(self._class_name_to_index)
        clone._string_value_to_index = dict(self._string_value_to_index)
        clone._name_and_type_to_index = dict(self._name_and_type_to_index)
        clone._fieldref_to_index = dict(self._fieldref_to_index)
        clone._methodref_to_index = dict(self._methodref_to_index)
        clone._interface_methodref_to_index = dict(self._interface_methodref_to_index)
        return clone

    def checkpoint(self):
        """Capture the current allocation state for later rollback."""

        return (
            len(self._pool),
            self._next_index,
            dict(self._key_to_index),
            dict(self._utf8_to_index),
            dict(self._string_to_utf8_index),
            dict(self._resolved_utf8_cache),
            dict(self._class_name_to_index),
            dict(self._string_value_to_index),
            dict(self._name_and_type_to_index),
            dict(self._fieldref_to_index),
            dict(self._methodref_to_index),
            dict(self._interface_methodref_to_index),
        )

    def rollback(self, checkpoint):
        """Restore the builder to a previously captured checkpoint."""

        (
            pool_len,
            next_index,
            key_to_index,
            utf8_to_index,
            string_to_utf8_index,
            resolved_utf8_cache,
            class_name_to_index,
            string_value_to_index,
            name_and_type_to_index,
            fieldref_to_index,
            methodref_to_index,
            interface_methodref_to_index,
        ) = checkpoint
        del self._pool[pool_len:]
        self._next_index = next_index
        self._key_to_index = key_to_index
        self._utf8_to_index = utf8_to_index
        self._string_to_utf8_index = string_to_utf8_index
        self._resolved_utf8_cache = resolved_utf8_cache
        self._class_name_to_index = class_name_to_index
        self._string_value_to_index = string_value_to_index
        self._name_and_type_to_index = name_and_type_to_index
        self._fieldref_to_index = fieldref_to_index
        self._methodref_to_index = methodref_to_index
        self._interface_methodref_to_index = interface_methodref_to_index

    # ------------------------------------------------------------------
    # Internal allocation
    # ------------------------------------------------------------------

    def _allocate(self, entry):
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

        cdef int index
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

    def _validate_entry(self, entry):
        if isinstance(entry, Utf8Info):
            _validate_utf8_entry(entry)
        elif isinstance(entry, MethodHandleInfo):
            _validate_method_handle(self._pool, entry.reference_kind, entry.reference_index)

    # ------------------------------------------------------------------
    # Low-level entry insertion
    # ------------------------------------------------------------------

    def add_entry(self, entry):
        """Insert an arbitrary constant pool entry with deduplication.

        The caller's object is never mutated; an internal copy is stored.

        Args:
            entry: Any ``ConstantPoolInfo`` subclass instance to add.

        Returns:
            The CP index of the entry — existing if a duplicate was found,
            otherwise newly allocated.

        Raises:
            ValueError: If the entry fails validation (e.g. invalid modified
                UTF-8 or illegal ``MethodHandle`` reference kind).
        """
        entry_copy = _copy_pool_entry(entry)
        self._validate_entry(entry_copy)
        return self._allocate(entry_copy)

    # ------------------------------------------------------------------
    # Utf8 and primitive entries
    # ------------------------------------------------------------------

    def add_utf8(self, value):
        """Add a ``CONSTANT_Utf8`` entry for a Python string (§4.4.7).

        The string is encoded to JVM modified UTF-8.

        Args:
            value: The Python string to store.

        Returns:
            The CP index of the (possibly pre-existing) entry.

        Raises:
            ValueError: If the encoded form exceeds the 65 535-byte JVM limit.
        """
        existing = self._string_to_utf8_index.get(value)
        if existing is not None:
            return existing

        encoded = encode_modified_utf8(value)
        if len(encoded) > _UTF8_MAX_BYTES:
            raise ValueError(f"Modified UTF-8 payload exceeds JVM u2 length limit of {_UTF8_MAX_BYTES} bytes")

        # Fast path: Utf8 lookup bypasses the general key dict.
        existing = self._utf8_to_index.get(encoded)
        if existing is not None:
            self._string_to_utf8_index[value] = existing
            return existing
        entry = Utf8Info(index=0, offset=0, tag=_TAG_UTF8, length=len(encoded), str_bytes=encoded)
        cdef int index
        index = self._allocate(entry)
        self._string_to_utf8_index[value] = index
        self._resolved_utf8_cache[index] = (encoded, value)
        return index

    def add_integer(self, value):
        """Add a ``CONSTANT_Integer`` entry (§4.4.4).

        Args:
            value: Raw 4-byte integer value stored as the ``bytes`` field.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        entry = IntegerInfo(index=0, offset=0, tag=_TAG_INTEGER, value_bytes=value)
        return self._allocate(entry)

    def add_float(self, raw_bits):
        """Add a ``CONSTANT_Float`` entry (§4.4.4).

        Args:
            raw_bits: IEEE 754 single-precision bit pattern.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        entry = FloatInfo(index=0, offset=0, tag=_TAG_FLOAT, value_bytes=raw_bits)
        return self._allocate(entry)

    def add_long(self, high, low):
        """Add a ``CONSTANT_Long`` entry (§4.4.5, double-slot).

        Args:
            high: Upper 32 bits of the long value.
            low: Lower 32 bits of the long value.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        entry = LongInfo(index=0, offset=0, tag=_TAG_LONG, high_bytes=high, low_bytes=low)
        return self._allocate(entry)

    def add_double(self, high, low):
        """Add a ``CONSTANT_Double`` entry (§4.4.5, double-slot).

        Args:
            high: Upper 32 bits of the double value.
            low: Lower 32 bits of the double value.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        entry = DoubleInfo(index=0, offset=0, tag=_TAG_DOUBLE, high_bytes=high, low_bytes=low)
        return self._allocate(entry)

    # ------------------------------------------------------------------
    # Compound entries (auto-create prerequisites)
    # ------------------------------------------------------------------

    def add_class(self, name):
        """Add a ``CONSTANT_Class`` entry (§4.4.1).

        The required ``CONSTANT_Utf8`` name entry is created automatically.

        Args:
            name: Class or interface name in JVM internal form
                (e.g. ``java/lang/Object``).

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        existing = self._class_name_to_index.get(name)
        if existing is not None:
            return existing
        cdef int name_index, index
        name_index = self.add_utf8(name)
        entry = ClassInfo(index=0, offset=0, tag=_TAG_CLASS, name_index=name_index)
        index = self._allocate(entry)
        self._class_name_to_index[name] = index
        return index

    def add_string(self, value):
        """Add a ``CONSTANT_String`` entry (§4.4.3).

        The required ``CONSTANT_Utf8`` entry is created automatically.

        Args:
            value: The string literal value.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        existing = self._string_value_to_index.get(value)
        if existing is not None:
            return existing
        cdef int string_index, index
        string_index = self.add_utf8(value)
        entry = StringInfo(index=0, offset=0, tag=_TAG_STRING, string_index=string_index)
        index = self._allocate(entry)
        self._string_value_to_index[value] = index
        return index

    def add_name_and_type(self, name, descriptor):
        """Add a ``CONSTANT_NameAndType`` entry (§4.4.6).

        Both ``CONSTANT_Utf8`` entries are created automatically.

        Args:
            name: Unqualified field or method name.
            descriptor: Field or method descriptor string.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        key = (name, descriptor)
        existing = self._name_and_type_to_index.get(key)
        if existing is not None:
            return existing
        cdef int name_index, descriptor_index, index
        name_index = self.add_utf8(name)
        descriptor_index = self.add_utf8(descriptor)
        entry = NameAndTypeInfo(
            index=0,
            offset=0,
            tag=_TAG_NAME_AND_TYPE,
            name_index=name_index,
            descriptor_index=descriptor_index,
        )
        index = self._allocate(entry)
        self._name_and_type_to_index[key] = index
        return index

    def add_fieldref(self, class_name, field_name, descriptor):
        """Add a ``CONSTANT_Fieldref`` entry (§4.4.2).

        Prerequisite ``CONSTANT_Class`` and ``CONSTANT_NameAndType`` entries
        (and their ``CONSTANT_Utf8`` dependencies) are created automatically.

        Args:
            class_name: Owning class in JVM internal form.
            field_name: Unqualified field name.
            descriptor: Field descriptor string.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        key = (class_name, field_name, descriptor)
        existing = self._fieldref_to_index.get(key)
        if existing is not None:
            return existing
        cdef int class_index, nat_index, index
        class_index = self.add_class(class_name)
        nat_index = self.add_name_and_type(field_name, descriptor)
        entry = FieldrefInfo(
            index=0,
            offset=0,
            tag=_TAG_FIELDREF,
            class_index=class_index,
            name_and_type_index=nat_index,
        )
        index = self._allocate(entry)
        self._fieldref_to_index[key] = index
        return index

    def add_methodref(self, class_name, method_name, descriptor):
        """Add a ``CONSTANT_Methodref`` entry (§4.4.2).

        Prerequisite entries are created automatically.

        Args:
            class_name: Owning class in JVM internal form.
            method_name: Unqualified method name.
            descriptor: Method descriptor string.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        key = (class_name, method_name, descriptor)
        existing = self._methodref_to_index.get(key)
        if existing is not None:
            return existing
        cdef int class_index, nat_index, index
        class_index = self.add_class(class_name)
        nat_index = self.add_name_and_type(method_name, descriptor)
        entry = MethodrefInfo(
            index=0,
            offset=0,
            tag=_TAG_METHODREF,
            class_index=class_index,
            name_and_type_index=nat_index,
        )
        index = self._allocate(entry)
        self._methodref_to_index[key] = index
        return index

    def add_interface_methodref(self, class_name, method_name, descriptor):
        """Add a ``CONSTANT_InterfaceMethodref`` entry (§4.4.2).

        Prerequisite entries are created automatically.

        Args:
            class_name: Owning interface in JVM internal form.
            method_name: Unqualified method name.
            descriptor: Method descriptor string.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        key = (class_name, method_name, descriptor)
        existing = self._interface_methodref_to_index.get(key)
        if existing is not None:
            return existing
        cdef int class_index, nat_index, index
        class_index = self.add_class(class_name)
        nat_index = self.add_name_and_type(method_name, descriptor)
        entry = InterfaceMethodrefInfo(
            index=0,
            offset=0,
            tag=_TAG_INTERFACE_METHODREF,
            class_index=class_index,
            name_and_type_index=nat_index,
        )
        index = self._allocate(entry)
        self._interface_methodref_to_index[key] = index
        return index

    # ------------------------------------------------------------------
    # Remaining entry types
    # ------------------------------------------------------------------

    def add_method_handle(self, reference_kind, reference_index):
        """Add a ``CONSTANT_MethodHandle`` entry (§4.4.8).

        Args:
            reference_kind: JVM reference-kind value (1–9), denoting the
                bytecode behavior of the handle.
            reference_index: CP index of the target ``Fieldref``,
                ``Methodref``, or ``InterfaceMethodref`` entry.

        Returns:
            The CP index of the (possibly pre-existing) entry.

        Raises:
            ValueError: If *reference_kind* is out of range or the target
                entry type is incompatible with the specified kind.
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

    def add_method_type(self, descriptor):
        """Add a ``CONSTANT_MethodType`` entry (§4.4.9).

        The required ``CONSTANT_Utf8`` descriptor entry is created
        automatically.

        Args:
            descriptor: Method descriptor string.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        cdef int descriptor_index
        descriptor_index = self.add_utf8(descriptor)
        entry = MethodTypeInfo(index=0, offset=0, tag=_TAG_METHOD_TYPE, descriptor_index=descriptor_index)
        return self._allocate(entry)

    def add_dynamic(self, bootstrap_method_attr_index, name, descriptor):
        """Add a ``CONSTANT_Dynamic`` entry (§4.4.10).

        The ``CONSTANT_NameAndType`` entry and its ``CONSTANT_Utf8``
        dependencies are created automatically.

        Args:
            bootstrap_method_attr_index: Index into the
                ``BootstrapMethods`` attribute table.
            name: Unqualified name of the dynamically-computed constant.
            descriptor: Field descriptor for the constant's type.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        cdef int nat_index
        nat_index = self.add_name_and_type(name, descriptor)
        entry = DynamicInfo(
            index=0,
            offset=0,
            tag=_TAG_DYNAMIC,
            bootstrap_method_attr_index=bootstrap_method_attr_index,
            name_and_type_index=nat_index,
        )
        return self._allocate(entry)

    def add_invoke_dynamic(self, bootstrap_method_attr_index, name, descriptor):
        """Add a ``CONSTANT_InvokeDynamic`` entry (§4.4.10).

        The ``CONSTANT_NameAndType`` entry and its ``CONSTANT_Utf8``
        dependencies are created automatically.

        Args:
            bootstrap_method_attr_index: Index into the
                ``BootstrapMethods`` attribute table.
            name: Unqualified method name for the call site.
            descriptor: Method descriptor for the call site.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        cdef int nat_index
        nat_index = self.add_name_and_type(name, descriptor)
        entry = InvokeDynamicInfo(
            index=0,
            offset=0,
            tag=_TAG_INVOKE_DYNAMIC,
            bootstrap_method_attr_index=bootstrap_method_attr_index,
            name_and_type_index=nat_index,
        )
        return self._allocate(entry)

    def add_module(self, name):
        """Add a ``CONSTANT_Module`` entry (§4.4.11).

        The required ``CONSTANT_Utf8`` name entry is created automatically.

        Args:
            name: Module name.

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        cdef int name_index
        name_index = self.add_utf8(name)
        entry = ModuleInfo(index=0, offset=0, tag=_TAG_MODULE, name_index=name_index)
        return self._allocate(entry)

    def add_package(self, name):
        """Add a ``CONSTANT_Package`` entry (§4.4.12).

        The required ``CONSTANT_Utf8`` name entry is created automatically.

        Args:
            name: Package name in JVM internal form (e.g. ``java/lang``).

        Returns:
            The CP index of the (possibly pre-existing) entry.
        """
        cdef int name_index
        name_index = self.add_utf8(name)
        entry = PackageInfo(index=0, offset=0, tag=_TAG_PACKAGE, name_index=name_index)
        return self._allocate(entry)

    # ------------------------------------------------------------------
    # Lookups
    # ------------------------------------------------------------------

    def get(self, index):
        """Return the entry at a given CP index.

        Returns a defensive copy; ``None`` is returned for index 0 and
        double-slot gap positions.

        Args:
            index: Constant pool index to retrieve.

        Returns:
            A copy of the entry, or ``None`` for placeholder/gap slots.

        Raises:
            IndexError: If *index* is out of the pool's range.
        """
        if index < 0 or index >= len(self._pool):
            raise IndexError(f"CP index {index} out of range [0, {len(self._pool) - 1}]")
        return _copy_entry(self._pool[index])

    def peek(self, index):
        """Return the entry at a CP index without allocating a defensive copy."""
        if index < 0 or index >= len(self._pool):
            raise IndexError(f"CP index {index} out of range [0, {len(self._pool) - 1}]")
        return self._pool[index]

    def find_utf8(self, value):
        """Look up the CP index of a ``CONSTANT_Utf8`` entry by string value.

        Args:
            value: The Python string to search for.

        Returns:
            The CP index if found, otherwise ``None``.
        """
        existing = self._string_to_utf8_index.get(value)
        if existing is not None:
            return existing

        encoded = encode_modified_utf8(value)
        existing = self._utf8_to_index.get(encoded)
        if existing is not None:
            self._string_to_utf8_index[value] = existing
        return existing

    def _find_key(self, key):
        return self._key_to_index.get(key)

    def find_integer(self, value):
        """Look up the CP index of a ``CONSTANT_Integer`` entry."""

        return self._find_key((_TAG_INTEGER, value))

    def find_float(self, raw_bits):
        """Look up the CP index of a ``CONSTANT_Float`` entry."""

        return self._find_key((_TAG_FLOAT, raw_bits))

    def find_long(self, high, low):
        """Look up the CP index of a ``CONSTANT_Long`` entry."""

        return self._find_key((_TAG_LONG, high, low))

    def find_double(self, high, low):
        """Look up the CP index of a ``CONSTANT_Double`` entry."""

        return self._find_key((_TAG_DOUBLE, high, low))

    def find_class(self, name):
        """Look up the CP index of a ``CONSTANT_Class`` entry by class name.

        Args:
            name: Class name in JVM internal form (e.g. ``java/lang/Object``).

        Returns:
            The CP index if found, otherwise ``None``.
        """
        existing = self._class_name_to_index.get(name)
        if existing is not None:
            return existing
        utf8_idx = self.find_utf8(name)
        if utf8_idx is None:
            return None
        existing = self._find_key((_TAG_CLASS, utf8_idx))
        if existing is not None:
            self._class_name_to_index[name] = existing
        return existing

    def find_string(self, value):
        """Look up the CP index of a ``CONSTANT_String`` entry by value."""

        existing = self._string_value_to_index.get(value)
        if existing is not None:
            return existing
        string_index = self.find_utf8(value)
        if string_index is None:
            return None
        existing = self._find_key((_TAG_STRING, string_index))
        if existing is not None:
            self._string_value_to_index[value] = existing
        return existing

    def find_method_type(self, descriptor):
        """Look up the CP index of a ``CONSTANT_MethodType`` entry."""

        descriptor_index = self.find_utf8(descriptor)
        if descriptor_index is None:
            return None
        return self._find_key((_TAG_METHOD_TYPE, descriptor_index))

    def find_name_and_type(self, name, descriptor):
        """Look up the CP index of a ``CONSTANT_NameAndType`` entry.

        Args:
            name: Unqualified field or method name.
            descriptor: Field or method descriptor string.

        Returns:
            The CP index if found, otherwise ``None``.
        """
        key = (name, descriptor)
        existing = self._name_and_type_to_index.get(key)
        if existing is not None:
            return existing
        name_idx = self.find_utf8(name)
        if name_idx is None:
            return None
        desc_idx = self.find_utf8(descriptor)
        if desc_idx is None:
            return None
        existing = self._find_key((_TAG_NAME_AND_TYPE, name_idx, desc_idx))
        if existing is not None:
            self._name_and_type_to_index[key] = existing
        return existing

    def find_fieldref(self, class_name, field_name, descriptor):
        """Look up the CP index of a ``CONSTANT_Fieldref`` entry."""

        key = (class_name, field_name, descriptor)
        existing = self._fieldref_to_index.get(key)
        if existing is not None:
            return existing
        class_index = self.find_class(class_name)
        if class_index is None:
            return None
        nat_index = self.find_name_and_type(field_name, descriptor)
        if nat_index is None:
            return None
        existing = self._find_key((_TAG_FIELDREF, class_index, nat_index))
        if existing is not None:
            self._fieldref_to_index[key] = existing
        return existing

    def find_methodref(self, class_name, method_name, descriptor):
        """Look up the CP index of a ``CONSTANT_Methodref`` entry."""

        key = (class_name, method_name, descriptor)
        existing = self._methodref_to_index.get(key)
        if existing is not None:
            return existing
        class_index = self.find_class(class_name)
        if class_index is None:
            return None
        nat_index = self.find_name_and_type(method_name, descriptor)
        if nat_index is None:
            return None
        existing = self._find_key((_TAG_METHODREF, class_index, nat_index))
        if existing is not None:
            self._methodref_to_index[key] = existing
        return existing

    def find_interface_methodref(self, class_name, method_name, descriptor):
        """Look up the CP index of a ``CONSTANT_InterfaceMethodref`` entry."""

        key = (class_name, method_name, descriptor)
        existing = self._interface_methodref_to_index.get(key)
        if existing is not None:
            return existing
        class_index = self.find_class(class_name)
        if class_index is None:
            return None
        nat_index = self.find_name_and_type(method_name, descriptor)
        if nat_index is None:
            return None
        existing = self._find_key((_TAG_INTERFACE_METHODREF, class_index, nat_index))
        if existing is not None:
            self._interface_methodref_to_index[key] = existing
        return existing

    def find_method_handle(self, reference_kind, reference_index):
        """Look up the CP index of a ``CONSTANT_MethodHandle`` entry."""

        return self._find_key((_TAG_METHOD_HANDLE, reference_kind, reference_index))

    def find_dynamic(self, bootstrap_method_attr_index, name, descriptor):
        """Look up the CP index of a ``CONSTANT_Dynamic`` entry."""

        nat_index = self.find_name_and_type(name, descriptor)
        if nat_index is None:
            return None
        return self._find_key((_TAG_DYNAMIC, bootstrap_method_attr_index, nat_index))

    def resolve_utf8(self, index):
        """Decode the ``CONSTANT_Utf8`` entry at a CP index to a Python string.

        Args:
            index: Constant pool index of the ``CONSTANT_Utf8`` entry.

        Returns:
            The decoded Python string.

        Raises:
            ValueError: If the entry at *index* is not a ``CONSTANT_Utf8``.
        """
        entry = self.peek(index)
        if not isinstance(entry, Utf8Info):
            raise ValueError(f"CP index {index} is not a CONSTANT_Utf8 entry: {type(entry).__name__}")
        cached = self._resolved_utf8_cache.get(index)
        if cached is not None and cached[0] is entry.str_bytes:
            return cached[1]

        value = decode_modified_utf8(entry.str_bytes)
        self._resolved_utf8_cache[index] = (entry.str_bytes, value)
        self._string_to_utf8_index.setdefault(value, self._utf8_to_index.get(entry.str_bytes, index))
        return value

    # ------------------------------------------------------------------
    # Output
    # ------------------------------------------------------------------

    def build(self):
        """Export the constant pool as a spec-format list.

        The returned list has the same structure as
        ``ClassFile.constant_pool``: index 0 is ``None``, Long/Double
        second slots are ``None``, and all other positions hold a copy of a
        ``ConstantPoolInfo`` instance.  Use ``count`` as the
        ``constant_pool_count`` field when serializing.

        Returns:
            A new list of entry copies suitable for class file emission.
        """
        return [_copy_entry(entry) for entry in self._pool]

    @property
    def count(self):
        """The ``constant_pool_count`` value (§4.1) for class file serialization."""
        return self._next_index

    def __len__(self):
        """Return the number of logical entries, excluding placeholder and gap slots."""
        return sum(1 for e in self._pool if e is not None)
