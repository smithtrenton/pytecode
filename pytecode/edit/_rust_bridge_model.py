"""Fast Rust-backed model conversion.

Converts a ``RustClassModel`` (from the PyO3 bridge) into a Python
``ClassModel`` dataclass, bypassing the slow coerce-bridge and Python
constant-pool builder entirely. The Rust bridge now exposes live
sequence views for collections such as ``fields``, ``methods``,
``attributes``, and ``instructions``; calling ``list(...)`` on those
views is the explicit snapshot/materialization boundary used here.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any, cast

from ..classfile.attributes import (
    AttributeInfoType,
    ConstantValueAttr,
    ExceptionsAttr,
    SignatureAttr,
    SourceDebugExtensionAttr,
    SourceFileAttr,
    UnimplementedAttr,
)
from ..classfile.constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from ..classfile.instructions import (
    ArrayType,
    ByteValue,
    InsnInfo,
    InsnInfoType,
    NewArray,
    ShortValue,
)
from .constant_pool_builder import ConstantPoolBuilder
from .debug_info import DebugInfoState
from .labels import (
    BranchInsn,
    ExceptionHandler,
    Label,
    LineNumberEntry,
    LocalVariableEntry,
    LocalVariableTypeEntry,
    LookupSwitchInsn,
    TableSwitchInsn,
)
from .operands import (
    FieldInsn,
    IIncInsn,
    InterfaceMethodInsn,
    InvokeDynamicInsn,
    LdcClass,
    LdcDouble,
    LdcDynamic,
    LdcFloat,
    LdcInsn,
    LdcInt,
    LdcLong,
    LdcMethodHandle,
    LdcMethodType,
    LdcString,
    MethodInsn,
    MultiANewArrayInsn,
    TypeInsn,
    VarInsn,
)

type CodeItem = InsnInfo | Label
type RustAttrConverter = Callable[[Any], Any]


def _rust_module() -> Any:
    """Import the Rust extension, returning ``None`` if unavailable."""
    try:
        from .. import _rust  # type: ignore[attr-defined]

        return _rust
    except ImportError:
        return None


# Rust PyO3 attribute classes that need conversion to Python dataclasses.
_rust_attr_converters: dict[type[object], RustAttrConverter] | None = None


def _get_rust_attr_converters() -> dict[type[object], RustAttrConverter]:
    global _rust_attr_converters  # noqa: PLW0603
    if _rust_attr_converters is not None:
        return _rust_attr_converters
    rust = _rust_module()
    if rust is None:
        _rust_attr_converters = {}
        return _rust_attr_converters
    _rust_attr_converters = {}
    for rust_cls_name, convert_fn in [
        ("ConstantValueAttr", _convert_constant_value_attr),
        ("SignatureAttr", _convert_signature_attr),
        ("SourceFileAttr", _convert_source_file_attr),
        ("SourceDebugExtensionAttr", _convert_source_debug_extension_attr),
        ("ExceptionsAttr", _convert_exceptions_attr),
        ("UnimplementedAttr", _convert_unimplemented_attr),
    ]:
        cls = getattr(rust, rust_cls_name, None)
        if cls is not None:
            _rust_attr_converters[cls] = convert_fn
    return _rust_attr_converters


def _convert_constant_value_attr(attr: Any) -> Any:
    return ConstantValueAttr(
        attribute_name_index=attr.attribute_name_index,
        attribute_length=attr.attribute_length,
        constantvalue_index=attr.constantvalue_index,
    )


def _convert_signature_attr(attr: Any) -> Any:
    return SignatureAttr(
        attribute_name_index=attr.attribute_name_index,
        attribute_length=attr.attribute_length,
        signature_index=attr.signature_index,
    )


def _convert_source_file_attr(attr: Any) -> Any:
    return SourceFileAttr(
        attribute_name_index=attr.attribute_name_index,
        attribute_length=attr.attribute_length,
        sourcefile_index=attr.sourcefile_index,
    )


def _convert_source_debug_extension_attr(attr: Any) -> Any:
    return SourceDebugExtensionAttr(
        attribute_name_index=attr.attribute_name_index,
        attribute_length=attr.attribute_length,
        debug_extension=attr.debug_extension,
    )


def _convert_exceptions_attr(attr: Any) -> Any:
    return ExceptionsAttr(
        attribute_name_index=attr.attribute_name_index,
        attribute_length=attr.attribute_length,
        number_of_exceptions=attr.number_of_exceptions,
        exception_index_table=list(attr.exception_index_table),
    )


def _convert_unimplemented_attr(attr: Any) -> Any:
    return UnimplementedAttr(
        attribute_name_index=attr.attribute_name_index,
        attribute_length=attr.attribute_length,
        info=attr.info,
        attr_type=AttributeInfoType.UNIMPLEMENTED,
    )


def _convert_attribute(attr: Any) -> Any:
    """Convert a Rust-backed attribute to a Python dataclass equivalent."""
    converters = _get_rust_attr_converters()
    converter = converters.get(cast(type[object], type(attr)))
    if converter is not None:
        return converter(attr)
    # call_attr_class! types are already Python dataclasses
    return attr


def _convert_rust_label(
    rust_label: Any,
    label_map: dict[Any, Label],
) -> Label:
    """Map a RustLabel to a Python Label, preserving identity."""
    existing = label_map.get(rust_label)
    if existing is not None:
        return existing
    py_label = Label(name=rust_label.name)
    label_map[rust_label] = py_label
    return py_label


def _convert_ldc_value(item: dict[str, Any]) -> Any:
    """Convert an ldc code-item dict to a typed LdcValue."""
    vtype = item["value_type"]
    if vtype == "int":
        return LdcInt(item["value"])
    if vtype == "float":
        return LdcFloat(item["value"])
    if vtype == "long":
        return LdcLong(item["value"])
    if vtype == "double":
        raw = item["value"]
        return LdcDouble(high_bytes=(raw >> 32) & 0xFFFFFFFF, low_bytes=raw & 0xFFFFFFFF)
    if vtype == "string":
        return LdcString(item["value"])
    if vtype == "class":
        return LdcClass(item["value"])
    if vtype == "method_type":
        return LdcMethodType(item["value"])
    if vtype == "method_handle":
        return LdcMethodHandle(
            reference_kind=item["reference_kind"],
            owner=item["owner"],
            name=item["name"],
            descriptor=item["descriptor"],
            is_interface=item["is_interface"],
        )
    if vtype == "dynamic":
        return LdcDynamic(
            bootstrap_method_attr_index=item["bootstrap_method_attr_index"],
            name=item["name"],
            descriptor=item["descriptor"],
        )
    raise ValueError(f"unknown ldc value_type: {vtype!r}")


def _convert_code_item(
    item: dict[str, Any],
    label_map: dict[Any, Label],
) -> CodeItem:
    """Convert a Rust code-item dict to a typed Python CodeItem."""
    kind = item["type"]

    if kind == "label":
        return _convert_rust_label(item["label"], label_map)

    if kind == "raw":
        opcode: int = item["opcode"]
        return InsnInfo(type=InsnInfoType(opcode), bytecode_offset=-1)

    if kind == "byte":
        return ByteValue(
            type=InsnInfoType(item["opcode"]),
            bytecode_offset=-1,
            value=item["value"],
        )

    if kind == "short":
        return ShortValue(
            type=InsnInfoType(item["opcode"]),
            bytecode_offset=-1,
            value=item["value"],
        )

    if kind == "newarray":
        return NewArray(
            type=InsnInfoType.NEWARRAY,
            bytecode_offset=-1,
            atype=ArrayType(item["atype"]),
        )

    if kind == "field":
        return FieldInsn(
            InsnInfoType(item["opcode"]),
            owner=item["owner"],
            name=item["name"],
            descriptor=item["descriptor"],
        )

    if kind == "method":
        return MethodInsn(
            InsnInfoType(item["opcode"]),
            owner=item["owner"],
            name=item["name"],
            descriptor=item["descriptor"],
            is_interface=item.get("is_interface", False),
        )

    if kind == "interface_method":
        return InterfaceMethodInsn(
            owner=item["owner"],
            name=item["name"],
            descriptor=item["descriptor"],
        )

    if kind == "type":
        return TypeInsn(
            InsnInfoType(item["opcode"]),
            class_name=item["descriptor"],
        )

    if kind == "var":
        return VarInsn(
            InsnInfoType(item["opcode"]),
            slot=item["slot"],
        )

    if kind == "iinc":
        return IIncInsn(slot=item["slot"], increment=item["value"])

    if kind == "ldc":
        return LdcInsn(_convert_ldc_value(item))

    if kind == "invokedynamic":
        return InvokeDynamicInsn(
            bootstrap_method_attr_index=item["bootstrap_method_attr_index"],
            name=item["name"],
            descriptor=item["descriptor"],
        )

    if kind == "multianewarray":
        return MultiANewArrayInsn(
            class_name=item["descriptor"],
            dimensions=item["dimensions"],
        )

    if kind == "branch":
        return BranchInsn(
            InsnInfoType(item["opcode"]),
            target=_convert_rust_label(item["target"], label_map),
        )

    if kind == "lookupswitch":
        return LookupSwitchInsn(
            default_target=_convert_rust_label(item["default_target"], label_map),
            pairs=[(pair[0], _convert_rust_label(pair[1], label_map)) for pair in item["pairs"]],
        )

    if kind == "tableswitch":
        return TableSwitchInsn(
            default_target=_convert_rust_label(item["default_target"], label_map),
            low=item["low"],
            high=item["high"],
            targets=[_convert_rust_label(t, label_map) for t in item["targets"]],
        )

    raise ValueError(f"unknown code item type: {kind!r}")


def _convert_code_model(rust_code: Any, label_map: dict[Any, Label]) -> Any:
    """Convert a RustCodeModel to a Python CodeModel dict of args."""
    from .model import CodeModel

    instructions = [_convert_code_item(item, label_map) for item in rust_code.instructions]
    exception_handlers = [
        ExceptionHandler(
            start=_convert_rust_label(eh.start, label_map),
            end=_convert_rust_label(eh.end, label_map),
            handler=_convert_rust_label(eh.handler, label_map),
            catch_type=eh.catch_type,
        )
        for eh in rust_code.exception_handlers
    ]
    line_numbers = [
        LineNumberEntry(
            label=_convert_rust_label(ln["label"], label_map),
            line_number=ln["line_number"],
        )
        for ln in rust_code.line_numbers
    ]
    local_variables = [
        LocalVariableEntry(
            start=_convert_rust_label(lv["start"], label_map),
            end=_convert_rust_label(lv["end"], label_map),
            name=lv["name"],
            descriptor=lv["descriptor"],
            slot=lv["index"],
        )
        for lv in rust_code.local_variables
    ]
    local_variable_types = [
        LocalVariableTypeEntry(
            start=_convert_rust_label(lvt["start"], label_map),
            end=_convert_rust_label(lvt["end"], label_map),
            name=lvt["name"],
            signature=lvt["signature"],
            slot=lvt["index"],
        )
        for lvt in rust_code.local_variable_types
    ]
    debug_state = DebugInfoState.FRESH if rust_code.debug_info_state == "fresh" else DebugInfoState.STALE
    # Rust reports "stack_map_table" in layout; Python lowerer treats it as "other"
    _LAYOUT_MAP: dict[str, str] = {
        "line_numbers": "line_numbers",
        "local_variables": "local_variables",
        "local_variable_types": "local_variable_types",
        "stack_map_table": "other",
        "other": "other",
    }
    layout: tuple[str, ...] = tuple(
        _LAYOUT_MAP[s] if s in _LAYOUT_MAP else s for s in rust_code.nested_attribute_layout
    )
    return CodeModel(
        max_stack=rust_code.max_stack,
        max_locals=rust_code.max_locals,
        instructions=instructions,
        exception_handlers=exception_handlers,
        line_numbers=line_numbers,
        local_variables=local_variables,
        local_variable_types=local_variable_types,
        attributes=[_convert_attribute(a) for a in rust_code.attributes],
        _nested_attribute_layout=layout,
        debug_info_state=debug_state,
    )


def from_rust_model(
    rust_model: Any,
    *,
    skip_debug: bool = False,
) -> Any:
    """Convert a RustClassModel to a Python ClassModel.

    This is the fast path for ``ClassModel.from_bytes()`` that bypasses
    the coerce bridge and Python constant-pool builder entirely.
    """
    from .debug_info import skip_debug_method_attributes, strip_class_debug_attributes
    from .model import ClassModel, FieldModel, MethodModel

    label_map: dict[Any, Label] = {}

    fields = [
        FieldModel(
            access_flags=FieldAccessFlag(f.access_flags),
            name=f.name,
            descriptor=f.descriptor,
            attributes=[_convert_attribute(a) for a in f.attributes],
        )
        for f in rust_model.fields
    ]

    methods: list[MethodModel] = []
    for m in rust_model.methods:
        code = None
        rust_code = m.code
        if rust_code is not None:
            code = _convert_code_model(rust_code, label_map)
            if skip_debug:
                code.line_numbers = []
                code.local_variables = []
                code.local_variable_types = []
        method_attrs = [_convert_attribute(a) for a in m.attributes]
        if skip_debug:
            method_attrs = skip_debug_method_attributes(method_attrs)
        methods.append(
            MethodModel(
                access_flags=MethodAccessFlag(m.access_flags),
                name=m.name,
                descriptor=m.descriptor,
                code=code,
                attributes=method_attrs,
            )
        )

    class_attrs = [_convert_attribute(a) for a in rust_model.attributes]
    if skip_debug:
        class_attrs = strip_class_debug_attributes(class_attrs)

    return ClassModel(
        version=rust_model.version,
        access_flags=ClassAccessFlag(rust_model.access_flags),
        name=rust_model.name,
        super_name=rust_model.super_name,
        interfaces=list(rust_model.interfaces),
        fields=fields,
        methods=methods,
        attributes=class_attrs,
        constant_pool=ConstantPoolBuilder(),
    )
