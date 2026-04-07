"""Helpers for converting Rust-backed raw wrappers into Python raw dataclasses."""

from __future__ import annotations

import struct
from typing import Any, cast

from .._internal.bytes_utils import BytesReader
from . import attributes, constant_pool, info, instructions


def _rust_module() -> Any | None:
    try:
        from pytecode import _rust
    except ModuleNotFoundError:
        return None
    return _rust


def is_rust_classfile(value: object) -> bool:
    rust = _rust_module()
    return rust is not None and isinstance(value, rust.ClassFile)


def coerce_python_classfile(value: info.ClassFile | object) -> info.ClassFile:
    if isinstance(value, info.ClassFile):
        return value

    rust = _rust_module()
    if rust is None or not isinstance(value, rust.ClassFile):
        raise TypeError(f"Expected ClassFile or Rust-backed ClassFile, got {type(value).__name__}")

    classfile = cast(Any, value)
    constant_pool_items = [
        _convert_constant_pool_entry(entry) if entry is not None else None for entry in classfile.constant_pool
    ]
    fields = [_convert_field(field, constant_pool_items) for field in classfile.fields]
    methods = [_convert_method(method, constant_pool_items) for method in classfile.methods]
    attrs = [_convert_attribute(attr, constant_pool_items) for attr in classfile.attributes]

    return info.ClassFile(
        magic=classfile.magic,
        minor_version=classfile.minor_version,
        major_version=classfile.major_version,
        constant_pool_count=classfile.constant_pool_count,
        constant_pool=constant_pool_items,
        access_flags=classfile.access_flags,
        this_class=classfile.this_class,
        super_class=classfile.super_class,
        interfaces_count=classfile.interfaces_count,
        interfaces=list(classfile.interfaces),
        fields_count=classfile.fields_count,
        fields=fields,
        methods_count=classfile.methods_count,
        methods=methods,
        attributes_count=classfile.attributes_count,
        attributes=attrs,
    )


def _offset(value: Any) -> int:
    offset = getattr(value, "offset", None)
    return 0 if offset is None else int(offset)


def _convert_constant_pool_entry(entry: Any) -> constant_pool.ConstantPoolInfo:
    name = type(entry).__name__
    common = {"index": int(entry.index), "offset": _offset(entry), "tag": int(entry.tag)}

    if name == "Utf8Info":
        return constant_pool.Utf8Info(
            **common,
            length=int(entry.length),
            str_bytes=bytes(entry.str_bytes),
        )
    if name == "IntegerInfo":
        return constant_pool.IntegerInfo(**common, value_bytes=int(entry.value_bytes))
    if name == "FloatInfo":
        return constant_pool.FloatInfo(**common, value_bytes=int(entry.value_bytes))
    if name == "LongInfo":
        return constant_pool.LongInfo(
            **common,
            high_bytes=int(entry.high_bytes),
            low_bytes=int(entry.low_bytes),
        )
    if name == "DoubleInfo":
        return constant_pool.DoubleInfo(
            **common,
            high_bytes=int(entry.high_bytes),
            low_bytes=int(entry.low_bytes),
        )
    if name == "ClassInfo":
        return constant_pool.ClassInfo(**common, name_index=int(entry.name_index))
    if name == "StringInfo":
        return constant_pool.StringInfo(**common, string_index=int(entry.string_index))
    if name == "FieldrefInfo":
        return constant_pool.FieldrefInfo(
            **common,
            class_index=int(entry.class_index),
            name_and_type_index=int(entry.name_and_type_index),
        )
    if name == "MethodrefInfo":
        return constant_pool.MethodrefInfo(
            **common,
            class_index=int(entry.class_index),
            name_and_type_index=int(entry.name_and_type_index),
        )
    if name == "InterfaceMethodrefInfo":
        return constant_pool.InterfaceMethodrefInfo(
            **common,
            class_index=int(entry.class_index),
            name_and_type_index=int(entry.name_and_type_index),
        )
    if name == "NameAndTypeInfo":
        return constant_pool.NameAndTypeInfo(
            **common,
            name_index=int(entry.name_index),
            descriptor_index=int(entry.descriptor_index),
        )
    if name == "MethodHandleInfo":
        return constant_pool.MethodHandleInfo(
            **common,
            reference_kind=int(entry.reference_kind),
            reference_index=int(entry.reference_index),
        )
    if name == "MethodTypeInfo":
        return constant_pool.MethodTypeInfo(**common, descriptor_index=int(entry.descriptor_index))
    if name == "DynamicInfo":
        return constant_pool.DynamicInfo(
            **common,
            bootstrap_method_attr_index=int(entry.bootstrap_method_attr_index),
            name_and_type_index=int(entry.name_and_type_index),
        )
    if name == "InvokeDynamicInfo":
        return constant_pool.InvokeDynamicInfo(
            **common,
            bootstrap_method_attr_index=int(entry.bootstrap_method_attr_index),
            name_and_type_index=int(entry.name_and_type_index),
        )
    if name == "ModuleInfo":
        return constant_pool.ModuleInfo(**common, name_index=int(entry.name_index))
    if name == "PackageInfo":
        return constant_pool.PackageInfo(**common, name_index=int(entry.name_index))

    raise TypeError(f"Unsupported Rust constant-pool wrapper {name}")


def _convert_field(
    field: Any,
    constant_pool_items: list[constant_pool.ConstantPoolInfo | None],
) -> info.FieldInfo:
    attrs = [_convert_attribute(attr, constant_pool_items) for attr in field.attributes]
    return info.FieldInfo(
        access_flags=field.access_flags,
        name_index=int(field.name_index),
        descriptor_index=int(field.descriptor_index),
        attributes_count=int(field.attributes_count),
        attributes=attrs,
    )


def _convert_method(
    method: Any,
    constant_pool_items: list[constant_pool.ConstantPoolInfo | None],
) -> info.MethodInfo:
    attrs = [_convert_attribute(attr, constant_pool_items) for attr in method.attributes]
    return info.MethodInfo(
        access_flags=method.access_flags,
        name_index=int(method.name_index),
        descriptor_index=int(method.descriptor_index),
        attributes_count=int(method.attributes_count),
        attributes=attrs,
    )


def _convert_attribute(
    attr: Any,
    constant_pool_items: list[constant_pool.ConstantPoolInfo | None],
) -> attributes.AttributeInfo:
    name = type(attr).__name__

    if name == "ConstantValueAttr":
        return attributes.ConstantValueAttr(
            int(attr.attribute_name_index),
            int(attr.attribute_length),
            int(attr.constantvalue_index),
        )
    if name == "SignatureAttr":
        return attributes.SignatureAttr(
            int(attr.attribute_name_index),
            int(attr.attribute_length),
            int(attr.signature_index),
        )
    if name == "SourceFileAttr":
        return attributes.SourceFileAttr(
            int(attr.attribute_name_index),
            int(attr.attribute_length),
            int(attr.sourcefile_index),
        )
    if name == "SourceDebugExtensionAttr":
        return attributes.SourceDebugExtensionAttr(
            int(attr.attribute_name_index),
            int(attr.attribute_length),
            bytes(attr.debug_extension).decode("utf-8"),
        )
    if name == "ExceptionsAttr":
        table = [int(index) for index in attr.exception_index_table]
        return attributes.ExceptionsAttr(
            int(attr.attribute_name_index),
            int(attr.attribute_length),
            int(attr.number_of_exceptions),
            table,
        )
    if name == "CodeAttr":
        code = [_convert_instruction(insn) for insn in attr.code]
        exception_table = [_convert_exception(entry) for entry in attr.exception_table]
        nested_attrs = [_convert_attribute(item, constant_pool_items) for item in attr.attributes]
        return attributes.CodeAttr(
            int(attr.attribute_name_index),
            int(attr.attribute_length),
            int(attr.max_stacks),
            int(attr.max_locals),
            int(attr.code_length),
            code,
            int(attr.exception_table_length),
            exception_table,
            int(attr.attributes_count),
            nested_attrs,
        )
    if name == "UnimplementedAttr":
        return _parse_unknown_attribute(attr, constant_pool_items)

    raise TypeError(f"Unsupported Rust attribute wrapper {name}")


def _parse_unknown_attribute(
    attr: Any,
    constant_pool_items: list[constant_pool.ConstantPoolInfo | None],
) -> attributes.AttributeInfo:
    from . import reader as reader_module

    payload = (
        struct.pack(">H", int(attr.attribute_name_index))
        + struct.pack(">I", int(attr.attribute_length))
        + bytes(attr.info)
    )
    scratch = object.__new__(reader_module.ClassReader)
    BytesReader.__init__(scratch, payload)
    scratch.constant_pool = constant_pool_items
    scratch._rust_reader = None
    return reader_module.ClassReader.read_attribute(scratch)


def _convert_exception(entry: Any) -> attributes.ExceptionInfo:
    return attributes.ExceptionInfo(
        int(entry.start_pc),
        int(entry.end_pc),
        int(entry.handler_pc),
        int(entry.catch_type),
    )


def _convert_instruction(insn: Any) -> instructions.InsnInfo:
    insn_type = insn.type
    instinfo = insn_type.instinfo
    bytecode_offset = int(insn.bytecode_offset)

    if instinfo is instructions.InsnInfo:
        return instructions.InsnInfo(insn_type, bytecode_offset)
    if instinfo in (instructions.LocalIndex, instructions.LocalIndexW, instructions.ConstPoolIndex):
        return cast(
            instructions.InsnInfo,
            instinfo(insn_type, bytecode_offset, int(insn.index)),
        )
    if instinfo in (instructions.ByteValue, instructions.ShortValue):
        return cast(
            instructions.InsnInfo,
            instinfo(insn_type, bytecode_offset, int(insn.value)),
        )
    if instinfo in (instructions.Branch, instructions.BranchW):
        return cast(
            instructions.InsnInfo,
            instinfo(insn_type, bytecode_offset, int(insn.branch_offset)),
        )
    if instinfo in (instructions.IInc, instructions.IIncW):
        return cast(
            instructions.InsnInfo,
            instinfo(insn_type, bytecode_offset, int(insn.index), int(insn.value)),
        )
    if instinfo is instructions.InvokeDynamic:
        return instructions.InvokeDynamic(
            insn_type,
            bytecode_offset,
            int(insn.index),
            bytes(insn.reserved),
        )
    if instinfo is instructions.InvokeInterface:
        return instructions.InvokeInterface(
            insn_type,
            bytecode_offset,
            int(insn.index),
            int(insn.count),
            bytes(insn.reserved),
        )
    if instinfo is instructions.NewArray:
        return instructions.NewArray(insn_type, bytecode_offset, insn.atype)
    if instinfo is instructions.MultiANewArray:
        return instructions.MultiANewArray(
            insn_type,
            bytecode_offset,
            int(insn.index),
            int(insn.dimensions),
        )
    if instinfo is instructions.LookupSwitch:
        pairs_raw: list[Any] = list(insn.pairs or [])
        pairs = [instructions.MatchOffsetPair(int(pair.match), int(pair.offset)) for pair in pairs_raw]
        return instructions.LookupSwitch(
            insn_type,
            bytecode_offset,
            int(insn.default),
            int(insn.npairs),
            pairs,
        )
    if instinfo is instructions.TableSwitch:
        offsets_raw: list[Any] = list(insn.offsets or [])
        return instructions.TableSwitch(
            insn_type,
            bytecode_offset,
            int(insn.default),
            int(insn.low),
            int(insn.high),
            [int(offset) for offset in offsets_raw],
        )

    raise TypeError(f"Unsupported Rust instruction wrapper {type(insn).__name__}")
