from __future__ import annotations

import pytest

import pytecode.classfile.constant_pool as cp_module
import pytecode.edit.constant_pool_builder as builder_module


@pytest.mark.skipif(
    not builder_module._RUST_CONSTANT_POOL_BUILDER_AVAILABLE,
    reason="Rust constant-pool builder backend is not installed in this environment",
)
def test_constant_pool_builder_module_exports_rust_backend() -> None:
    assert builder_module.ConstantPoolBuilder is not builder_module._PythonConstantPoolBuilder


@pytest.mark.skipif(
    not builder_module._RUST_CONSTANT_POOL_BUILDER_AVAILABLE,
    reason="Rust constant-pool builder backend is not installed in this environment",
)
def test_rust_builder_matches_python_builder_for_compound_entries() -> None:
    rust_builder = builder_module.ConstantPoolBuilder()
    python_builder = builder_module._PythonConstantPoolBuilder()

    for builder in (rust_builder, python_builder):
        fieldref_idx = builder.add_fieldref("Owner", "value", "I")
        methodref_idx = builder.add_methodref("Owner", "call", "()V")
        interface_methodref_idx = builder.add_interface_methodref("Iface", "run", "()V")
        builder.add_method_handle(1, fieldref_idx)
        builder.add_method_handle(5, methodref_idx)
        builder.add_dynamic(7, "dyn", "I")
        builder.add_invoke_dynamic(8, "indy", "()V")
        builder.add_module("mod.name")
        builder.add_package("pkg/name")
        assert builder.find_fieldref("Owner", "value", "I") == fieldref_idx
        assert builder.find_methodref("Owner", "call", "()V") == methodref_idx
        assert builder.find_interface_methodref("Iface", "run", "()V") == interface_methodref_idx

    assert rust_builder.build() == python_builder.build()


@pytest.mark.skipif(
    not builder_module._RUST_CONSTANT_POOL_BUILDER_AVAILABLE,
    reason="Rust constant-pool builder backend is not installed in this environment",
)
def test_rust_builder_from_pool_clone_checkpoint_and_peek_behave_like_python() -> None:
    seed = builder_module._PythonConstantPoolBuilder()
    utf8_idx = seed.add_utf8("Hello")
    class_idx = seed.add_class("Owner")
    fieldref_idx = seed.add_fieldref("Owner", "value", "I")
    imported_pool = seed.build()

    rust_builder = builder_module.ConstantPoolBuilder.from_pool(imported_pool)
    python_builder = builder_module._PythonConstantPoolBuilder.from_pool(imported_pool)

    assert rust_builder.build() == python_builder.build()
    assert rust_builder.find_utf8("Hello") == utf8_idx
    assert rust_builder.find_class("Owner") == class_idx
    assert rust_builder.find_fieldref("Owner", "value", "I") == fieldref_idx

    clone = rust_builder.clone()
    assert clone.build() == rust_builder.build()

    checkpoint = rust_builder.checkpoint()
    rust_builder.add_utf8("Later")
    rust_builder.add_class("LaterOwner")
    rust_builder.rollback(checkpoint)
    assert rust_builder.build() == python_builder.build()

    live_entry = rust_builder.peek(utf8_idx)
    assert isinstance(live_entry, cp_module.Utf8Info)
    live_entry.str_bytes = b"Mutated"
    live_entry.length = len(live_entry.str_bytes)
    assert rust_builder.resolve_utf8(utf8_idx) == "Mutated"
