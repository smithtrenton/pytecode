"""Tests for pytecode.constant_pool_builder.ConstantPoolBuilder."""

from __future__ import annotations

import pytest

from pytecode import constant_pool as cp_module
from pytecode.constant_pool_builder import ConstantPoolBuilder
from pytecode.modified_utf8 import encode_modified_utf8

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def fresh() -> ConstantPoolBuilder:
    return ConstantPoolBuilder()


# ---------------------------------------------------------------------------
# Empty builder invariants
# ---------------------------------------------------------------------------


def test_empty_builder_count():
    b = fresh()
    assert b.count == 1  # next index starts at 1 → constant_pool_count = 1


def test_empty_builder_len():
    b = fresh()
    assert len(b) == 0


def test_empty_builder_build():
    b = fresh()
    pool = b.build()
    assert pool == [None]


def test_get_index_zero_returns_none():
    b = fresh()
    assert b.get(0) is None


def test_get_out_of_range_raises():
    b = fresh()
    with pytest.raises(IndexError):
        b.get(1)
    with pytest.raises(IndexError):
        b.get(-1)


# ---------------------------------------------------------------------------
# CONSTANT_Utf8 (tag 1)
# ---------------------------------------------------------------------------


def test_add_utf8_returns_index_one():
    b = fresh()
    idx = b.add_utf8("Hello")
    assert idx == 1


def test_add_utf8_entry_content():
    b = fresh()
    idx = b.add_utf8("Hello")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.Utf8Info)
    assert entry.index == 1
    assert entry.tag == 1
    assert entry.str_bytes == b"Hello"
    assert entry.length == 5


def test_add_utf8_empty_string():
    b = fresh()
    idx = b.add_utf8("")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.Utf8Info)
    assert entry.str_bytes == b""
    assert entry.length == 0


def test_add_utf8_multibyte():
    b = fresh()
    idx = b.add_utf8("café")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.Utf8Info)
    assert entry.str_bytes == encode_modified_utf8("café")


def test_add_utf8_nul_uses_modified_encoding():
    b = fresh()
    idx = b.add_utf8("\x00")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.Utf8Info)
    assert entry.str_bytes == b"\xC0\x80"
    assert entry.length == 2


def test_add_utf8_supplementary_char_uses_surrogate_encoding():
    b = fresh()
    idx = b.add_utf8("😀")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.Utf8Info)
    assert entry.str_bytes == b"\xED\xA0\xBD\xED\xB8\x80"
    assert entry.length == 6


def test_add_utf8_dedup():
    b = fresh()
    idx1 = b.add_utf8("same")
    idx2 = b.add_utf8("same")
    assert idx1 == idx2
    assert len(b) == 1
    assert b.count == 2


def test_add_utf8_different_strings_get_different_indexes():
    b = fresh()
    idx1 = b.add_utf8("foo")
    idx2 = b.add_utf8("bar")
    assert idx1 != idx2
    assert len(b) == 2


def test_add_utf8_rejects_payloads_over_u2_limit():
    b = fresh()
    with pytest.raises(ValueError, match="65535"):
        b.add_utf8("\x00" * 32768)


# ---------------------------------------------------------------------------
# CONSTANT_Integer (tag 3) and CONSTANT_Float (tag 4)
# ---------------------------------------------------------------------------


def test_add_integer():
    b = fresh()
    idx = b.add_integer(0xDEADBEEF)
    entry = b.get(idx)
    assert isinstance(entry, cp_module.IntegerInfo)
    assert entry.tag == 3
    assert entry.value_bytes == 0xDEADBEEF
    assert entry.index == idx


def test_add_integer_dedup():
    b = fresh()
    assert b.add_integer(42) == b.add_integer(42)
    assert len(b) == 1


def test_add_float():
    b = fresh()
    raw = 0x3F800000  # 1.0f
    idx = b.add_float(raw)
    entry = b.get(idx)
    assert isinstance(entry, cp_module.FloatInfo)
    assert entry.tag == 4
    assert entry.value_bytes == raw


def test_add_float_dedup():
    b = fresh()
    assert b.add_float(0x3F800000) == b.add_float(0x3F800000)
    assert len(b) == 1


# ---------------------------------------------------------------------------
# CONSTANT_Long (tag 5) and CONSTANT_Double (tag 6) — double-slot
# ---------------------------------------------------------------------------


def test_add_long_double_slot():
    b = fresh()
    idx = b.add_long(0xDEADBEEF, 0xCAFEBABE)
    assert idx == 1
    assert b.count == 3  # occupies slots 1 and 2; count = 3


def test_add_long_content():
    b = fresh()
    idx = b.add_long(0x000000AB, 0x000000CD)
    entry = b.get(idx)
    assert isinstance(entry, cp_module.LongInfo)
    assert entry.tag == 5
    assert entry.high_bytes == 0x000000AB
    assert entry.low_bytes == 0x000000CD


def test_add_long_gap_slot_is_none():
    b = fresh()
    idx = b.add_long(0, 1)
    assert b.get(idx + 1) is None


def test_add_long_next_entry_after_gap():
    b = fresh()
    b.add_long(0, 1)  # index 1, gap at 2
    idx = b.add_utf8("after")
    assert idx == 3


def test_add_double_double_slot():
    b = fresh()
    idx = b.add_double(0x400921FB, 0x54442D18)
    assert idx == 1
    assert b.count == 3


def test_add_double_gap_slot_is_none():
    b = fresh()
    idx = b.add_double(0, 1)
    assert b.get(idx + 1) is None


def test_add_long_dedup():
    b = fresh()
    idx1 = b.add_long(1, 2)
    idx2 = b.add_long(1, 2)
    assert idx1 == idx2
    assert b.count == 3  # pool didn't grow


def test_add_double_dedup():
    b = fresh()
    idx1 = b.add_double(1, 2)
    idx2 = b.add_double(1, 2)
    assert idx1 == idx2
    assert b.count == 3


def test_len_excludes_double_slot_gaps():
    b = fresh()
    b.add_long(0, 1)   # 1 logical entry, 2 slots
    b.add_utf8("x")    # 1 logical entry, 1 slot
    assert len(b) == 2
    assert b.count == 4


# ---------------------------------------------------------------------------
# CONSTANT_Class (tag 7) — auto-creates Utf8
# ---------------------------------------------------------------------------


def test_add_class_creates_utf8():
    b = fresh()
    cls_idx = b.add_class("java/lang/Object")
    assert cls_idx == 2  # utf8 at 1, class at 2
    utf8_idx = b.find_utf8("java/lang/Object")
    assert utf8_idx == 1
    entry = b.get(cls_idx)
    assert isinstance(entry, cp_module.ClassInfo)
    assert entry.tag == 7
    assert entry.name_index == 1


def test_add_class_dedup():
    b = fresh()
    assert b.add_class("Foo") == b.add_class("Foo")
    assert len(b) == 2  # utf8 + class


def test_add_class_shares_utf8():
    b = fresh()
    utf8_idx = b.add_utf8("Foo")
    cls_idx = b.add_class("Foo")
    entry = b.get(cls_idx)
    assert isinstance(entry, cp_module.ClassInfo)
    assert entry.name_index == utf8_idx
    assert len(b) == 2  # shared utf8, no duplicates


# ---------------------------------------------------------------------------
# CONSTANT_String (tag 8)
# ---------------------------------------------------------------------------


def test_add_string_creates_utf8():
    b = fresh()
    str_idx = b.add_string("hello")
    assert str_idx == 2
    entry = b.get(str_idx)
    assert isinstance(entry, cp_module.StringInfo)
    assert entry.tag == 8
    assert entry.string_index == 1


def test_add_string_dedup():
    b = fresh()
    assert b.add_string("x") == b.add_string("x")
    assert len(b) == 2


# ---------------------------------------------------------------------------
# CONSTANT_NameAndType (tag 12)
# ---------------------------------------------------------------------------


def test_add_name_and_type():
    b = fresh()
    nat_idx = b.add_name_and_type("<init>", "()V")
    entry = b.get(nat_idx)
    assert isinstance(entry, cp_module.NameAndTypeInfo)
    assert entry.tag == 12
    # utf8("<init>") → 1, utf8("()V") → 2, nat → 3
    assert nat_idx == 3
    assert entry.name_index == 1
    assert entry.descriptor_index == 2


def test_add_name_and_type_dedup():
    b = fresh()
    idx1 = b.add_name_and_type("foo", "I")
    idx2 = b.add_name_and_type("foo", "I")
    assert idx1 == idx2
    assert len(b) == 3  # utf8("foo"), utf8("I"), nat


def test_add_name_and_type_shares_utf8():
    b = fresh()
    b.add_utf8("foo")  # pre-add
    nat_idx = b.add_name_and_type("foo", "I")
    entry = b.get(nat_idx)
    assert isinstance(entry, cp_module.NameAndTypeInfo)
    assert entry.name_index == 1  # reused
    assert len(b) == 3


# ---------------------------------------------------------------------------
# CONSTANT_Fieldref (tag 9), Methodref (tag 10), InterfaceMethodref (tag 11)
# ---------------------------------------------------------------------------


def test_add_fieldref():
    b = fresh()
    idx = b.add_fieldref("com/example/Foo", "count", "I")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.FieldrefInfo)
    assert entry.tag == 9
    # class_index and nat_index should point to existing entries
    class_entry = b.get(entry.class_index)
    assert isinstance(class_entry, cp_module.ClassInfo)
    nat_entry = b.get(entry.name_and_type_index)
    assert isinstance(nat_entry, cp_module.NameAndTypeInfo)


def test_add_fieldref_dedup():
    b = fresh()
    idx1 = b.add_fieldref("Foo", "x", "I")
    idx2 = b.add_fieldref("Foo", "x", "I")
    assert idx1 == idx2


def test_add_methodref():
    b = fresh()
    idx = b.add_methodref("java/io/PrintStream", "println", "(Ljava/lang/String;)V")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.MethodrefInfo)
    assert entry.tag == 10


def test_add_methodref_dedup():
    b = fresh()
    idx1 = b.add_methodref("Foo", "bar", "()V")
    idx2 = b.add_methodref("Foo", "bar", "()V")
    assert idx1 == idx2


def test_add_interface_methodref():
    b = fresh()
    idx = b.add_interface_methodref("java/lang/Runnable", "run", "()V")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.InterfaceMethodrefInfo)
    assert entry.tag == 11


def test_add_interface_methodref_dedup():
    b = fresh()
    idx1 = b.add_interface_methodref("I", "m", "()V")
    idx2 = b.add_interface_methodref("I", "m", "()V")
    assert idx1 == idx2


def test_methodref_shares_prerequisites_with_fieldref():
    """Two entries sharing the same class name reuse the same Class and Utf8 entries."""
    b = fresh()
    b.add_fieldref("Foo", "x", "I")
    count_after_field = len(b)
    b.add_methodref("Foo", "bar", "()V")
    # "Foo" utf8 and Class already exist — only new utf8("bar"), utf8("()V"), nat, methodref added
    assert len(b) == count_after_field + 4


# ---------------------------------------------------------------------------
# CONSTANT_MethodHandle (tag 15), MethodType (tag 16)
# ---------------------------------------------------------------------------


def test_add_method_handle():
    b = fresh()
    ref_idx = b.add_methodref("Foo", "bar", "()V")
    idx = b.add_method_handle(6, ref_idx)
    entry = b.get(idx)
    assert isinstance(entry, cp_module.MethodHandleInfo)
    assert entry.tag == 15
    assert entry.reference_kind == 6
    assert entry.reference_index == ref_idx


def test_add_method_handle_dedup():
    b = fresh()
    fieldref_idx = b.add_fieldref("Foo", "value", "I")
    assert b.add_method_handle(1, fieldref_idx) == b.add_method_handle(1, fieldref_idx)


def test_add_method_handle_validates_reference_kind():
    b = fresh()
    fieldref_idx = b.add_fieldref("Foo", "value", "I")
    with pytest.raises(ValueError, match="range \\[1, 9\\]"):
        b.add_method_handle(0, fieldref_idx)
    with pytest.raises(ValueError, match="range \\[1, 9\\]"):
        b.add_method_handle(10, fieldref_idx)


def test_add_method_handle_requires_existing_target():
    b = fresh()
    with pytest.raises(ValueError, match="out of range"):
        b.add_method_handle(1, 1)


def test_add_method_handle_rejects_wrong_target_type():
    b = fresh()
    class_idx = b.add_class("Foo")
    with pytest.raises(ValueError, match="CONSTANT_Fieldref"):
        b.add_method_handle(1, class_idx)


def test_add_method_handle_new_invoke_special_requires_init():
    b = fresh()
    methodref_idx = b.add_methodref("Foo", "bar", "()V")
    with pytest.raises(ValueError, match="<init>"):
        b.add_method_handle(8, methodref_idx)


def test_add_method_handle_rejects_init_for_non_new_invoke_special():
    b = fresh()
    methodref_idx = b.add_methodref("Foo", "<init>", "()V")
    with pytest.raises(ValueError, match="cannot target special method"):
        b.add_method_handle(6, methodref_idx)


def test_add_method_type():
    b = fresh()
    idx = b.add_method_type("(I)V")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.MethodTypeInfo)
    assert entry.tag == 16
    desc_entry = b.get(entry.descriptor_index)
    assert isinstance(desc_entry, cp_module.Utf8Info)
    assert desc_entry.str_bytes == b"(I)V"


def test_add_method_type_dedup():
    b = fresh()
    assert b.add_method_type("(I)V") == b.add_method_type("(I)V")
    assert len(b) == 2  # utf8 + method_type


# ---------------------------------------------------------------------------
# CONSTANT_Dynamic (tag 17), InvokeDynamic (tag 18)
# ---------------------------------------------------------------------------


def test_add_dynamic():
    b = fresh()
    idx = b.add_dynamic(0, "myField", "I")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.DynamicInfo)
    assert entry.tag == 17
    assert entry.bootstrap_method_attr_index == 0
    nat_entry = b.get(entry.name_and_type_index)
    assert isinstance(nat_entry, cp_module.NameAndTypeInfo)


def test_add_dynamic_dedup():
    b = fresh()
    assert b.add_dynamic(0, "f", "I") == b.add_dynamic(0, "f", "I")


def test_add_invoke_dynamic():
    b = fresh()
    idx = b.add_invoke_dynamic(2, "myMethod", "(I)V")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.InvokeDynamicInfo)
    assert entry.tag == 18
    assert entry.bootstrap_method_attr_index == 2


def test_add_invoke_dynamic_dedup():
    b = fresh()
    assert b.add_invoke_dynamic(1, "m", "()V") == b.add_invoke_dynamic(1, "m", "()V")


def test_dynamic_and_invoke_dynamic_different_tags():
    b = fresh()
    dyn_idx = b.add_dynamic(0, "x", "I")
    indy_idx = b.add_invoke_dynamic(0, "x", "I")
    assert dyn_idx != indy_idx
    assert isinstance(b.get(dyn_idx), cp_module.DynamicInfo)
    assert isinstance(b.get(indy_idx), cp_module.InvokeDynamicInfo)


# ---------------------------------------------------------------------------
# CONSTANT_Module (tag 19), Package (tag 20)
# ---------------------------------------------------------------------------


def test_add_module():
    b = fresh()
    idx = b.add_module("com.example")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.ModuleInfo)
    assert entry.tag == 19
    name_entry = b.get(entry.name_index)
    assert isinstance(name_entry, cp_module.Utf8Info)
    assert name_entry.str_bytes == b"com.example"


def test_add_module_dedup():
    b = fresh()
    assert b.add_module("m") == b.add_module("m")
    assert len(b) == 2


def test_add_package():
    b = fresh()
    idx = b.add_package("java/lang")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.PackageInfo)
    assert entry.tag == 20
    name_entry = b.get(entry.name_index)
    assert isinstance(name_entry, cp_module.Utf8Info)
    assert name_entry.str_bytes == b"java/lang"


def test_add_package_dedup():
    b = fresh()
    assert b.add_package("p") == b.add_package("p")
    assert len(b) == 2


# ---------------------------------------------------------------------------
# Lookups: find_utf8, find_class, find_name_and_type, resolve_utf8
# ---------------------------------------------------------------------------


def test_find_utf8_hit():
    b = fresh()
    b.add_utf8("needle")
    assert b.find_utf8("needle") is not None


def test_find_utf8_miss():
    b = fresh()
    assert b.find_utf8("missing") is None


def test_find_class_hit():
    b = fresh()
    b.add_class("Foo")
    assert b.find_class("Foo") is not None


def test_find_class_miss_no_utf8():
    b = fresh()
    assert b.find_class("Foo") is None


def test_find_class_miss_utf8_exists_but_no_class():
    b = fresh()
    b.add_utf8("Bar")
    assert b.find_class("Bar") is None


def test_find_name_and_type_hit():
    b = fresh()
    b.add_name_and_type("foo", "I")
    assert b.find_name_and_type("foo", "I") is not None


def test_find_name_and_type_miss_wrong_descriptor():
    b = fresh()
    b.add_name_and_type("foo", "I")
    assert b.find_name_and_type("foo", "J") is None


def test_find_name_and_type_miss_no_entries():
    b = fresh()
    assert b.find_name_and_type("x", "I") is None


def test_resolve_utf8():
    b = fresh()
    idx = b.add_utf8("hello")
    assert b.resolve_utf8(idx) == "hello"


def test_resolve_utf8_multibyte():
    b = fresh()
    idx = b.add_utf8("café")
    assert b.resolve_utf8(idx) == "café"


def test_resolve_utf8_nul():
    b = fresh()
    idx = b.add_utf8("\x00")
    assert b.resolve_utf8(idx) == "\x00"


def test_resolve_utf8_supplementary_char():
    b = fresh()
    idx = b.add_utf8("😀")
    assert b.resolve_utf8(idx) == "😀"


def test_resolve_utf8_non_utf8_raises():
    b = fresh()
    idx = b.add_integer(42)
    with pytest.raises(ValueError):
        b.resolve_utf8(idx)


# ---------------------------------------------------------------------------
# Low-level add_entry
# ---------------------------------------------------------------------------


def test_add_entry_utf8():
    b = fresh()
    e = cp_module.Utf8Info(index=99, offset=0, tag=1, length=3, str_bytes=b"abc")
    idx = b.add_entry(e)
    assert idx == 1
    result = b.get(idx)
    assert isinstance(result, cp_module.Utf8Info)
    assert result.str_bytes == b"abc"


def test_add_entry_does_not_mutate_original():
    b = fresh()
    e = cp_module.IntegerInfo(index=77, offset=88, tag=3, value_bytes=9)
    b.add_entry(e)
    # Original object is unchanged
    assert e.index == 77
    assert e.offset == 88


def test_add_entry_dedup():
    b = fresh()
    e1 = cp_module.FloatInfo(index=0, offset=0, tag=4, value_bytes=0x3F800000)
    e2 = cp_module.FloatInfo(index=0, offset=0, tag=4, value_bytes=0x3F800000)
    assert b.add_entry(e1) == b.add_entry(e2)
    assert len(b) == 1


# ---------------------------------------------------------------------------
# Ordering determinism
# ---------------------------------------------------------------------------


def test_insertion_order_preserved():
    b = fresh()
    b.add_utf8("first")
    b.add_utf8("second")
    b.add_utf8("third")
    pool = b.build()
    assert pool[1].str_bytes == b"first"  # type: ignore[union-attr]
    assert pool[2].str_bytes == b"second"  # type: ignore[union-attr]
    assert pool[3].str_bytes == b"third"  # type: ignore[union-attr]


def test_dedup_does_not_change_order():
    b = fresh()
    b.add_utf8("alpha")
    b.add_utf8("beta")
    b.add_utf8("alpha")  # dedup — should not move "alpha"
    pool = b.build()
    assert pool[1].str_bytes == b"alpha"  # type: ignore[union-attr]
    assert pool[2].str_bytes == b"beta"  # type: ignore[union-attr]
    assert len(pool) == 3  # index 0 (None) + 2 entries


# ---------------------------------------------------------------------------
# build() and count
# ---------------------------------------------------------------------------


def test_build_index_zero_is_none():
    b = fresh()
    b.add_utf8("x")
    pool = b.build()
    assert pool[0] is None


def test_build_returns_copy():
    b = fresh()
    b.add_utf8("x")
    pool = b.build()
    pool.append(None)  # mutating returned list does not affect builder
    assert b.count == 2


def test_build_returns_entry_copies():
    b = fresh()
    idx = b.add_utf8("x")
    pool = b.build()
    entry = pool[idx]
    assert isinstance(entry, cp_module.Utf8Info)
    entry.str_bytes = b"changed"
    original = b.get(idx)
    assert isinstance(original, cp_module.Utf8Info)
    assert original.str_bytes == b"x"


def test_get_returns_entry_copy():
    b = fresh()
    idx = b.add_utf8("x")
    entry = b.get(idx)
    assert isinstance(entry, cp_module.Utf8Info)
    entry.str_bytes = b"changed"
    original = b.get(idx)
    assert isinstance(original, cp_module.Utf8Info)
    assert original.str_bytes == b"x"


def test_count_equals_next_index():
    b = fresh()
    b.add_utf8("a")
    b.add_utf8("b")
    b.add_long(0, 1)  # double-slot
    # utf8×2 (indexes 1,2) + long (indexes 3,4) → next index = 5
    assert b.count == 5


def test_build_double_slot_gap():
    b = fresh()
    b.add_long(0, 1)
    pool = b.build()
    assert isinstance(pool[1], cp_module.LongInfo)
    assert pool[2] is None


# ---------------------------------------------------------------------------
# from_pool — round-trip and import
# ---------------------------------------------------------------------------


def _make_small_pool() -> list[cp_module.ConstantPoolInfo | None]:
    """Build a small but realistic pool: None, utf8, utf8, class, string."""
    pool: list[cp_module.ConstantPoolInfo | None] = [None]
    pool.append(cp_module.Utf8Info(index=1, offset=10, tag=1, length=3, str_bytes=b"Foo"))
    pool.append(cp_module.Utf8Info(index=2, offset=14, tag=1, length=5, str_bytes=b"hello"))
    pool.append(cp_module.ClassInfo(index=3, offset=20, tag=7, name_index=1))
    pool.append(cp_module.StringInfo(index=4, offset=23, tag=8, string_index=2))
    return pool


def test_from_pool_preserves_indexes():
    b = ConstantPoolBuilder.from_pool(_make_small_pool())
    assert b.count == 5
    e1 = b.get(1)
    assert isinstance(e1, cp_module.Utf8Info)
    assert e1.str_bytes == b"Foo"
    e3 = b.get(3)
    assert isinstance(e3, cp_module.ClassInfo)
    assert e3.name_index == 1


def test_from_pool_populates_utf8_lookup():
    b = ConstantPoolBuilder.from_pool(_make_small_pool())
    assert b.find_utf8("Foo") == 1
    assert b.find_utf8("hello") == 2


def test_from_pool_populates_class_lookup():
    b = ConstantPoolBuilder.from_pool(_make_small_pool())
    assert b.find_class("Foo") == 3


def test_from_pool_deduplication_active():
    b = ConstantPoolBuilder.from_pool(_make_small_pool())
    # Adding "Foo" again should return the existing index 1
    assert b.add_utf8("Foo") == 1
    assert b.count == 5  # pool did not grow


def test_from_pool_new_entries_append():
    b = ConstantPoolBuilder.from_pool(_make_small_pool())
    new_idx = b.add_utf8("new")
    assert new_idx == 5  # next after imported pool
    assert b.count == 6


def test_from_pool_does_not_alias_original():
    pool = _make_small_pool()
    b = ConstantPoolBuilder.from_pool(pool)
    # Mutating original pool entry should not affect builder
    original = pool[1]
    assert isinstance(original, cp_module.Utf8Info)
    original.str_bytes = b"CHANGED"
    e = b.get(1)
    assert isinstance(e, cp_module.Utf8Info)
    assert e.str_bytes == b"Foo"


def test_from_pool_with_double_slot():
    pool: list[cp_module.ConstantPoolInfo | None] = [None]
    pool.append(cp_module.LongInfo(index=1, offset=0, tag=5, high_bytes=0xAB, low_bytes=0xCD))
    pool.append(None)  # double-slot gap
    pool.append(cp_module.Utf8Info(index=3, offset=0, tag=1, length=3, str_bytes=b"abc"))
    b = ConstantPoolBuilder.from_pool(pool)
    assert b.count == 4
    assert b.get(2) is None
    e = b.get(3)
    assert isinstance(e, cp_module.Utf8Info)
    assert e.str_bytes == b"abc"


def test_from_pool_build_round_trip():
    """from_pool(pool).build() reproduces entries at the same indexes."""
    pool = _make_small_pool()
    b = ConstantPoolBuilder.from_pool(pool)
    rebuilt = b.build()
    assert len(rebuilt) == len(pool)
    for orig, copy_ in zip(pool, rebuilt):
        if orig is None:
            assert copy_ is None
        else:
            assert isinstance(copy_, cp_module.ConstantPoolInfo)
            assert type(orig) is type(copy_)
            assert orig.index == copy_.index


def test_from_pool_rejects_missing_index_zero_placeholder():
    pool: list[cp_module.ConstantPoolInfo | None] = [
        cp_module.Utf8Info(index=1, offset=0, tag=1, length=1, str_bytes=b"x")
    ]
    with pytest.raises(ValueError, match="index 0 must be None"):
        ConstantPoolBuilder.from_pool(pool)


def test_from_pool_rejects_missing_double_slot_gap():
    pool: list[cp_module.ConstantPoolInfo | None] = [None]
    pool.append(cp_module.LongInfo(index=1, offset=0, tag=5, high_bytes=0, low_bytes=1))
    pool.append(cp_module.Utf8Info(index=2, offset=0, tag=1, length=1, str_bytes=b"x"))
    with pytest.raises(ValueError, match="gap slot"):
        ConstantPoolBuilder.from_pool(pool)


def test_from_pool_rejects_invalid_method_handle_reference():
    pool: list[cp_module.ConstantPoolInfo | None] = [None]
    pool.append(cp_module.Utf8Info(index=1, offset=0, tag=1, length=3, str_bytes=b"Foo"))
    pool.append(cp_module.ClassInfo(index=2, offset=0, tag=7, name_index=1))
    pool.append(cp_module.MethodHandleInfo(index=3, offset=0, tag=15, reference_kind=1, reference_index=2))
    with pytest.raises(ValueError, match="CONSTANT_Fieldref"):
        ConstantPoolBuilder.from_pool(pool)


# ---------------------------------------------------------------------------
# Pool overflow guard
# ---------------------------------------------------------------------------


def test_overflow_single_slot():
    """Builder must raise ValueError when the pool is full."""
    b = fresh()
    # Fill pool to max single-slot index (65534).
    # Add one real entry, then use add_long to jump ahead (faster than 65534 add_utf8 calls).
    # We'll manipulate _next_index directly for speed.
    b._next_index = 65534
    b._pool.extend([None] * (65534 - 1))  # pad pool list to match
    # Now adding one more single-slot entry is fine (index 65534 → count 65535)
    b._next_index = 65535  # simulate full pool
    b._pool.append(None)  # keep list length in sync
    with pytest.raises(ValueError, match="overflow"):
        b.add_utf8("overflow")


def test_overflow_double_slot():
    b = fresh()
    b._next_index = 65534  # only room for one single-slot entry, not a double
    b._pool.extend([None] * (65534 - 1))
    with pytest.raises(ValueError, match="overflow"):
        b.add_long(1, 2)


# ---------------------------------------------------------------------------
# All 17 entry types produce the right tag
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("method_name, args, tag", [
    ("add_utf8", ("x",), 1),
    ("add_integer", (0,), 3),
    ("add_float", (0,), 4),
    ("add_long", (0, 0), 5),
    ("add_double", (0, 0), 6),
    ("add_class", ("A",), 7),
    ("add_string", ("s",), 8),
    ("add_name_and_type", ("n", "I"), 12),
    ("add_fieldref", ("A", "f", "I"), 9),
    ("add_methodref", ("A", "m", "()V"), 10),
    ("add_interface_methodref", ("A", "m", "()V"), 11),
    ("add_method_handle", (1, "fieldref"), 15),
    ("add_method_type", ("()V",), 16),
    ("add_dynamic", (0, "n", "I"), 17),
    ("add_invoke_dynamic", (0, "n", "I"), 18),
    ("add_module", ("m",), 19),
    ("add_package", ("p",), 20),
])
def test_entry_tag(method_name: str, args: tuple[object, ...], tag: int):
    b = fresh()
    if method_name == "add_method_handle":
        args = (args[0], b.add_fieldref("A", "f", "I"))
    idx = getattr(b, method_name)(*args)
    entry = b.get(idx)
    assert entry is not None
    assert entry.tag == tag
