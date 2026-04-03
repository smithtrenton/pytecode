"""Serialize a ClassFile tree into JVM ``.class`` file bytes (JVMS §4)."""

from __future__ import annotations

from .._internal.bytes_utils import BytesWriter
from .._internal.rust_import import import_optional_rust_module
from . import attributes, constant_pool, instructions
from .info import ClassFile, FieldInfo, MethodInfo

__all__ = ["ClassWriter"]

try:
    _rust_code = import_optional_rust_module("pytecode._rust.classfile.code")
except ModuleNotFoundError:
    _rust_write_code_bytes = None
else:
    _rust_write_code_bytes = _rust_code.write_code_bytes

_RUST_CODE_AVAILABLE = _rust_write_code_bytes is not None


class ClassWriter:
    """Serializer that converts a ``ClassFile`` tree into JVM ``.class`` bytes.

    This writer walks every section of the ``ClassFile`` structure — constant
    pool, fields, methods, and attributes — and emits the binary encoding
    defined by the JVM class-file format (JVMS §4).
    """

    @staticmethod
    def write(classfile: ClassFile) -> bytes:
        """Serialize a ``ClassFile`` into raw ``.class`` file bytes.

        Encodes all sections of the class file (magic number, version,
        constant pool, access flags, fields, methods, and attributes) into
        the binary format specified by JVMS §4.1.

        Args:
            classfile: The parsed class-file structure to serialize.

        Returns:
            The complete ``.class`` binary content.
        """
        writer = BytesWriter()
        _write_classfile(writer, classfile)
        return writer.to_bytes()


def _write_classfile(writer: BytesWriter, classfile: ClassFile) -> None:
    writer.write_u4(classfile.magic)
    writer.write_u2(classfile.minor_version)
    writer.write_u2(classfile.major_version)

    pool = classfile.constant_pool
    writer.write_u2(len(pool))
    for entry in _iter_constant_pool_entries(pool):
        _write_constant_pool_entry(writer, entry)

    writer.write_u2(int(classfile.access_flags))
    writer.write_u2(classfile.this_class)
    writer.write_u2(classfile.super_class)

    writer.write_u2(len(classfile.interfaces))
    for interface_index in classfile.interfaces:
        writer.write_u2(interface_index)

    writer.write_u2(len(classfile.fields))
    for field in classfile.fields:
        _write_field_info(writer, field)

    writer.write_u2(len(classfile.methods))
    for method in classfile.methods:
        _write_method_info(writer, method)

    _write_attributes(writer, classfile.attributes)


def _iter_constant_pool_entries(
    pool: list[constant_pool.ConstantPoolInfo | None],
) -> list[constant_pool.ConstantPoolInfo]:
    if not pool:
        raise ValueError("constant pool must include slot 0")
    if pool[0] is not None:
        raise ValueError("constant pool slot 0 must be None")

    entries: list[constant_pool.ConstantPoolInfo] = []
    expect_gap = False
    for index in range(1, len(pool)):
        entry = pool[index]
        if expect_gap:
            if entry is not None:
                raise ValueError(f"constant pool slot {index} must be empty after a Long/Double entry")
            expect_gap = False
            continue

        if entry is None:
            raise ValueError(f"constant pool slot {index} is unexpectedly empty")

        entries.append(entry)
        expect_gap = isinstance(entry, (constant_pool.LongInfo, constant_pool.DoubleInfo))

    if expect_gap:
        raise ValueError("constant pool is missing the trailing gap slot for a Long/Double entry")

    return entries


def _write_constant_pool_entry(writer: BytesWriter, entry: constant_pool.ConstantPoolInfo) -> None:
    writer.write_u1(entry.tag)

    if isinstance(entry, constant_pool.ClassInfo):
        writer.write_u2(entry.name_index)
    elif isinstance(entry, constant_pool.StringInfo):
        writer.write_u2(entry.string_index)
    elif isinstance(entry, constant_pool.MethodTypeInfo):
        writer.write_u2(entry.descriptor_index)
    elif isinstance(entry, constant_pool.ModuleInfo):
        writer.write_u2(entry.name_index)
    elif isinstance(entry, constant_pool.PackageInfo):
        writer.write_u2(entry.name_index)
    elif isinstance(entry, constant_pool.FieldrefInfo):
        writer.write_u2(entry.class_index)
        writer.write_u2(entry.name_and_type_index)
    elif isinstance(entry, constant_pool.MethodrefInfo):
        writer.write_u2(entry.class_index)
        writer.write_u2(entry.name_and_type_index)
    elif isinstance(entry, constant_pool.InterfaceMethodrefInfo):
        writer.write_u2(entry.class_index)
        writer.write_u2(entry.name_and_type_index)
    elif isinstance(entry, constant_pool.NameAndTypeInfo):
        writer.write_u2(entry.name_index)
        writer.write_u2(entry.descriptor_index)
    elif isinstance(entry, constant_pool.DynamicInfo):
        writer.write_u2(entry.bootstrap_method_attr_index)
        writer.write_u2(entry.name_and_type_index)
    elif isinstance(entry, constant_pool.InvokeDynamicInfo):
        writer.write_u2(entry.bootstrap_method_attr_index)
        writer.write_u2(entry.name_and_type_index)
    elif isinstance(entry, constant_pool.IntegerInfo):
        writer.write_u4(entry.value_bytes)
    elif isinstance(entry, constant_pool.FloatInfo):
        writer.write_u4(entry.value_bytes)
    elif isinstance(entry, constant_pool.LongInfo):
        writer.write_u4(entry.high_bytes)
        writer.write_u4(entry.low_bytes)
    elif isinstance(entry, constant_pool.DoubleInfo):
        writer.write_u4(entry.high_bytes)
        writer.write_u4(entry.low_bytes)
    elif isinstance(entry, constant_pool.Utf8Info):
        writer.write_u2(len(entry.str_bytes))
        writer.write_bytes(entry.str_bytes)
    elif isinstance(entry, constant_pool.MethodHandleInfo):
        writer.write_u1(entry.reference_kind)
        writer.write_u2(entry.reference_index)
    else:
        raise ValueError(f"Unsupported constant-pool entry type: {type(entry).__name__}")


def _write_field_info(writer: BytesWriter, field: FieldInfo) -> None:
    writer.write_u2(int(field.access_flags))
    writer.write_u2(field.name_index)
    writer.write_u2(field.descriptor_index)
    _write_attributes(writer, field.attributes)


def _write_method_info(writer: BytesWriter, method: MethodInfo) -> None:
    writer.write_u2(int(method.access_flags))
    writer.write_u2(method.name_index)
    writer.write_u2(method.descriptor_index)
    _write_attributes(writer, method.attributes)


def _write_attributes(writer: BytesWriter, attrs: list[attributes.AttributeInfo]) -> None:
    writer.write_u2(len(attrs))
    for attr in attrs:
        _write_attribute(writer, attr)


def _write_attribute(writer: BytesWriter, attr: attributes.AttributeInfo) -> None:
    payload_writer = BytesWriter()
    _write_attribute_payload(payload_writer, attr)
    payload = payload_writer.to_bytes()

    writer.write_u2(attr.attribute_name_index)
    writer.write_u4(len(payload))
    writer.write_bytes(payload)


def _write_code_bytes_python(code: list[instructions.InsnInfo]) -> bytes:
    code_writer = BytesWriter()
    for insn in code:
        _write_instruction(code_writer, insn)
    return code_writer.to_bytes()


def _write_code_bytes_rust(code: list[instructions.InsnInfo]) -> bytes:
    rust_write = _rust_write_code_bytes
    assert rust_write is not None
    raw = rust_write(code)
    return bytes(raw)


def _write_attribute_payload(writer: BytesWriter, attr: attributes.AttributeInfo) -> None:
    if isinstance(attr, (attributes.SyntheticAttr, attributes.DeprecatedAttr)):
        return

    if isinstance(attr, attributes.ConstantValueAttr):
        writer.write_u2(attr.constantvalue_index)
        return

    if isinstance(attr, attributes.SignatureAttr):
        writer.write_u2(attr.signature_index)
        return

    if isinstance(attr, attributes.SourceFileAttr):
        writer.write_u2(attr.sourcefile_index)
        return

    if isinstance(attr, attributes.ModuleMainClassAttr):
        writer.write_u2(attr.main_class_index)
        return

    if isinstance(attr, attributes.NestHostAttr):
        writer.write_u2(attr.host_class_index)
        return

    if isinstance(attr, attributes.CodeAttr):
        if _RUST_CODE_AVAILABLE:
            code_bytes = _write_code_bytes_rust(attr.code)
        else:
            code_bytes = _write_code_bytes_python(attr.code)

        writer.write_u2(attr.max_stacks)
        writer.write_u2(attr.max_locals)
        writer.write_u4(len(code_bytes))
        writer.write_bytes(code_bytes)
        writer.write_u2(len(attr.exception_table))
        for exception in attr.exception_table:
            writer.write_u2(exception.start_pc)
            writer.write_u2(exception.end_pc)
            writer.write_u2(exception.handler_pc)
            writer.write_u2(exception.catch_type)
        _write_attributes(writer, attr.attributes)
        return

    if isinstance(attr, attributes.StackMapTableAttr):
        writer.write_u2(len(attr.entries))
        for entry in attr.entries:
            _write_stack_map_frame_info(writer, entry)
        return

    if isinstance(attr, attributes.ExceptionsAttr):
        writer.write_u2(len(attr.exception_index_table))
        for exception_index in attr.exception_index_table:
            writer.write_u2(exception_index)
        return

    if isinstance(attr, attributes.InnerClassesAttr):
        writer.write_u2(len(attr.classes))
        for entry in attr.classes:
            writer.write_u2(entry.inner_class_info_index)
            writer.write_u2(entry.outer_class_info_index)
            writer.write_u2(entry.inner_name_index)
            writer.write_u2(int(entry.inner_class_access_flags))
        return

    if isinstance(attr, attributes.EnclosingMethodAttr):
        writer.write_u2(attr.class_index)
        writer.write_u2(attr.method_index)
        return

    if isinstance(attr, attributes.SourceDebugExtensionAttr):
        writer.write_bytes(attr.debug_extension.encode("utf-8"))
        return

    if isinstance(attr, attributes.LineNumberTableAttr):
        writer.write_u2(len(attr.line_number_table))
        for entry in attr.line_number_table:
            writer.write_u2(entry.start_pc)
            writer.write_u2(entry.line_number)
        return

    if isinstance(attr, attributes.LocalVariableTableAttr):
        writer.write_u2(len(attr.local_variable_table))
        for entry in attr.local_variable_table:
            writer.write_u2(entry.start_pc)
            writer.write_u2(entry.length)
            writer.write_u2(entry.name_index)
            writer.write_u2(entry.descriptor_index)
            writer.write_u2(entry.index)
        return

    if isinstance(attr, attributes.LocalVariableTypeTableAttr):
        writer.write_u2(len(attr.local_variable_type_table))
        for entry in attr.local_variable_type_table:
            writer.write_u2(entry.start_pc)
            writer.write_u2(entry.length)
            writer.write_u2(entry.name_index)
            writer.write_u2(entry.signature_index)
            writer.write_u2(entry.index)
        return

    if isinstance(
        attr,
        (
            attributes.RuntimeVisibleAnnotationsAttr,
            attributes.RuntimeInvisibleAnnotationsAttr,
        ),
    ):
        writer.write_u2(len(attr.annotations))
        for annotation in attr.annotations:
            _write_annotation_info(writer, annotation)
        return

    if isinstance(
        attr,
        (
            attributes.RuntimeVisibleParameterAnnotationsAttr,
            attributes.RuntimeInvisibleParameterAnnotationsAttr,
        ),
    ):
        writer.write_u1(len(attr.parameter_annotations))
        for parameter in attr.parameter_annotations:
            writer.write_u2(len(parameter.annotations))
            for annotation in parameter.annotations:
                _write_annotation_info(writer, annotation)
        return

    if isinstance(
        attr,
        (
            attributes.RuntimeVisibleTypeAnnotationsAttr,
            attributes.RuntimeInvisibleTypeAnnotationsAttr,
        ),
    ):
        writer.write_u2(len(attr.annotations))
        for annotation in attr.annotations:
            _write_type_annotation_info(writer, annotation)
        return

    if isinstance(attr, attributes.AnnotationDefaultAttr):
        _write_element_value_info(writer, attr.default_value)
        return

    if isinstance(attr, attributes.BootstrapMethodsAttr):
        writer.write_u2(len(attr.bootstrap_methods))
        for method in attr.bootstrap_methods:
            writer.write_u2(method.bootstrap_method_ref)
            writer.write_u2(len(method.boostrap_arguments))
            for argument in method.boostrap_arguments:
                writer.write_u2(argument)
        return

    if isinstance(attr, attributes.MethodParametersAttr):
        writer.write_u1(len(attr.parameters))
        for parameter in attr.parameters:
            writer.write_u2(parameter.name_index)
            writer.write_u2(int(parameter.access_flags))
        return

    if isinstance(attr, attributes.ModuleAttr):
        writer.write_u2(attr.module_name_index)
        writer.write_u2(int(attr.module_flags))
        writer.write_u2(attr.module_version_index)

        writer.write_u2(len(attr.requires))
        for require in attr.requires:
            writer.write_u2(require.requires_index)
            writer.write_u2(int(require.requires_flag))
            writer.write_u2(require.requires_version_index)

        writer.write_u2(len(attr.exports))
        for export in attr.exports:
            writer.write_u2(export.exports_index)
            writer.write_u2(int(export.exports_flags))
            writer.write_u2(len(export.exports_to_index))
            for target in export.exports_to_index:
                writer.write_u2(target)

        writer.write_u2(len(attr.opens))
        for opened in attr.opens:
            writer.write_u2(opened.opens_index)
            writer.write_u2(int(opened.opens_flags))
            writer.write_u2(len(opened.opens_to_index))
            for target in opened.opens_to_index:
                writer.write_u2(target)

        writer.write_u2(len(attr.uses_index))
        for use in attr.uses_index:
            writer.write_u2(use)

        writer.write_u2(len(attr.provides))
        for provide in attr.provides:
            writer.write_u2(provide.provides_index)
            writer.write_u2(len(provide.provides_with_index))
            for implementation in provide.provides_with_index:
                writer.write_u2(implementation)
        return

    if isinstance(attr, attributes.ModulePackagesAttr):
        writer.write_u2(len(attr.package_index))
        for package_index in attr.package_index:
            writer.write_u2(package_index)
        return

    if isinstance(attr, attributes.NestMembersAttr):
        writer.write_u2(len(attr.classes))
        for class_index in attr.classes:
            writer.write_u2(class_index)
        return

    if isinstance(attr, attributes.RecordAttr):
        writer.write_u2(len(attr.components))
        for component in attr.components:
            writer.write_u2(component.name_index)
            writer.write_u2(component.descriptor_index)
            _write_attributes(writer, component.attributes)
        return

    if isinstance(attr, attributes.PermittedSubclassesAttr):
        writer.write_u2(len(attr.classes))
        for class_index in attr.classes:
            writer.write_u2(class_index)
        return

    if isinstance(attr, attributes.UnimplementedAttr):
        writer.write_bytes(attr.info)
        return

    raise ValueError(f"Unsupported attribute type: {type(attr).__name__}")


def _write_stack_map_frame_info(writer: BytesWriter, entry: attributes.StackMapFrameInfo) -> None:
    writer.write_u1(entry.frame_type)

    if isinstance(entry, attributes.SameFrameInfo):
        return
    if isinstance(entry, attributes.SameLocals1StackItemFrameInfo):
        _write_verification_type_info(writer, entry.stack)
        return
    if isinstance(entry, attributes.SameLocals1StackItemFrameExtendedInfo):
        writer.write_u2(entry.offset_delta)
        _write_verification_type_info(writer, entry.stack)
        return
    if isinstance(entry, attributes.ChopFrameInfo):
        writer.write_u2(entry.offset_delta)
        return
    if isinstance(entry, attributes.SameFrameExtendedInfo):
        writer.write_u2(entry.offset_delta)
        return
    if isinstance(entry, attributes.AppendFrameInfo):
        writer.write_u2(entry.offset_delta)
        for local in entry.locals:
            _write_verification_type_info(writer, local)
        return
    if isinstance(entry, attributes.FullFrameInfo):
        writer.write_u2(entry.offset_delta)
        writer.write_u2(len(entry.locals))
        for local in entry.locals:
            _write_verification_type_info(writer, local)
        writer.write_u2(len(entry.stack))
        for stack_item in entry.stack:
            _write_verification_type_info(writer, stack_item)
        return

    raise ValueError(f"Unsupported stack-map frame type: {type(entry).__name__}")


def _write_verification_type_info(writer: BytesWriter, entry: attributes.VerificationTypeInfo) -> None:
    writer.write_u1(int(entry.tag))

    if isinstance(entry, attributes.ObjectVariableInfo):
        writer.write_u2(entry.cpool_index)
    elif isinstance(entry, attributes.UninitializedVariableInfo):
        writer.write_u2(entry.offset)


def _write_annotation_info(writer: BytesWriter, annotation: attributes.AnnotationInfo) -> None:
    writer.write_u2(annotation.type_index)
    writer.write_u2(len(annotation.element_value_pairs))
    for pair in annotation.element_value_pairs:
        writer.write_u2(pair.element_name_index)
        _write_element_value_info(writer, pair.element_value)


def _write_element_value_info(writer: BytesWriter, element_value: attributes.ElementValueInfo) -> None:
    tag = _element_value_tag(element_value.tag)
    writer.write_u1(tag)

    if tag in {ord("B"), ord("C"), ord("D"), ord("F"), ord("I"), ord("J"), ord("S"), ord("Z"), ord("s")}:
        if not isinstance(element_value.value, attributes.ConstValueInfo):
            raise ValueError("const element value must carry ConstValueInfo")
        writer.write_u2(element_value.value.const_value_index)
        return

    if tag == ord("e"):
        if not isinstance(element_value.value, attributes.EnumConstantValueInfo):
            raise ValueError("enum element value must carry EnumConstantValueInfo")
        writer.write_u2(element_value.value.type_name_index)
        writer.write_u2(element_value.value.const_name_index)
        return

    if tag == ord("c"):
        if not isinstance(element_value.value, attributes.ClassInfoValueInfo):
            raise ValueError("class element value must carry ClassInfoValueInfo")
        writer.write_u2(element_value.value.class_info_index)
        return

    if tag == ord("@"):
        if not isinstance(element_value.value, attributes.AnnotationInfo):
            raise ValueError("annotation element value must carry AnnotationInfo")
        _write_annotation_info(writer, element_value.value)
        return

    if tag == ord("["):
        if not isinstance(element_value.value, attributes.ArrayValueInfo):
            raise ValueError("array element value must carry ArrayValueInfo")
        writer.write_u2(len(element_value.value.values))
        for nested in element_value.value.values:
            _write_element_value_info(writer, nested)
        return

    raise ValueError(f"Unsupported element-value tag: {tag!r}")


def _element_value_tag(tag: int | str) -> int:
    if isinstance(tag, int):
        if not 0 <= tag <= 255:
            raise ValueError(f"element-value tag must fit in u1, got {tag}")
        return tag
    if len(tag) != 1:
        raise ValueError(f"element-value tag must be a single character, got {tag!r}")
    return ord(tag)


def _write_type_annotation_info(writer: BytesWriter, annotation: attributes.TypeAnnotationInfo) -> None:
    writer.write_u1(annotation.target_type)
    _write_target_info(writer, annotation.target_info)
    _write_type_path_info(writer, annotation.target_path)
    writer.write_u2(annotation.type_index)
    writer.write_u2(len(annotation.element_value_pairs))
    for pair in annotation.element_value_pairs:
        writer.write_u2(pair.element_name_index)
        _write_element_value_info(writer, pair.element_value)


def _write_target_info(writer: BytesWriter, target_info: attributes.TargetInfo) -> None:
    if isinstance(target_info, attributes.TypeParameterTargetInfo):
        writer.write_u1(target_info.type_parameter_index)
        return
    if isinstance(target_info, attributes.SupertypeTargetInfo):
        writer.write_u2(target_info.supertype_index)
        return
    if isinstance(target_info, attributes.TypeParameterBoundTargetInfo):
        writer.write_u1(target_info.type_parameter_index)
        writer.write_u1(target_info.bound_index)
        return
    if isinstance(target_info, attributes.EmptyTargetInfo):
        return
    if isinstance(target_info, attributes.FormalParameterTargetInfo):
        writer.write_u1(target_info.formal_parameter_index)
        return
    if isinstance(target_info, attributes.ThrowsTargetInfo):
        writer.write_u2(target_info.throws_type_index)
        return
    if isinstance(target_info, attributes.LocalvarTargetInfo):
        writer.write_u2(len(target_info.table))
        for table_entry in target_info.table:
            writer.write_u2(table_entry.start_pc)
            writer.write_u2(table_entry.length)
            writer.write_u2(table_entry.index)
        return
    if isinstance(target_info, attributes.CatchTargetInfo):
        writer.write_u2(target_info.exception_table_index)
        return
    if isinstance(target_info, attributes.OffsetTargetInfo):
        writer.write_u2(target_info.offset)
        return
    if isinstance(target_info, attributes.TypeArgumentTargetInfo):
        writer.write_u2(target_info.offset)
        writer.write_u1(target_info.type_argument_index)
        return

    raise ValueError(f"Unsupported target-info type: {type(target_info).__name__}")


def _write_type_path_info(writer: BytesWriter, type_path: attributes.TypePathInfo) -> None:
    writer.write_u1(len(type_path.path))
    for path_item in type_path.path:
        writer.write_u1(path_item.type_path_kind)
        writer.write_u1(path_item.type_argument_index)


def _write_instruction(writer: BytesWriter, insn: instructions.InsnInfo) -> None:
    if isinstance(insn, instructions.LocalIndexW):
        writer.write_u1(int(instructions.InsnInfoType.WIDE))
        writer.write_u1(int(insn.type) - int(instructions.InsnInfoType.WIDE))
        writer.write_u2(insn.index)
        return

    if isinstance(insn, instructions.IIncW):
        writer.write_u1(int(instructions.InsnInfoType.WIDE))
        writer.write_u1(int(insn.type) - int(instructions.InsnInfoType.WIDE))
        writer.write_u2(insn.index)
        writer.write_i2(insn.value)
        return

    writer.write_u1(int(insn.type))

    if isinstance(insn, instructions.LocalIndex):
        writer.write_u1(insn.index)
    elif isinstance(insn, instructions.ConstPoolIndex):
        writer.write_u2(insn.index)
    elif isinstance(insn, instructions.ByteValue):
        writer.write_i1(insn.value)
    elif isinstance(insn, instructions.ShortValue):
        writer.write_i2(insn.value)
    elif isinstance(insn, instructions.Branch):
        writer.write_i2(insn.offset)
    elif isinstance(insn, instructions.BranchW):
        writer.write_i4(insn.offset)
    elif isinstance(insn, instructions.IInc):
        writer.write_u1(insn.index)
        writer.write_i1(insn.value)
    elif isinstance(insn, instructions.InvokeDynamic):
        if len(insn.unused) != 2:
            raise ValueError("InvokeDynamic unused bytes must be exactly 2 bytes")
        writer.write_u2(insn.index)
        writer.write_bytes(insn.unused)
    elif isinstance(insn, instructions.InvokeInterface):
        if len(insn.unused) != 1:
            raise ValueError("InvokeInterface unused bytes must be exactly 1 byte")
        writer.write_u2(insn.index)
        writer.write_u1(insn.count)
        writer.write_bytes(insn.unused)
    elif isinstance(insn, instructions.MultiANewArray):
        writer.write_u2(insn.index)
        writer.write_u1(insn.dimensions)
    elif isinstance(insn, instructions.NewArray):
        writer.write_u1(int(insn.atype))
    elif isinstance(insn, instructions.LookupSwitch):
        writer.align(4)
        writer.write_i4(insn.default)
        writer.write_u4(len(insn.pairs))
        for pair in insn.pairs:
            writer.write_i4(pair.match)
            writer.write_i4(pair.offset)
    elif isinstance(insn, instructions.TableSwitch):
        writer.align(4)
        writer.write_i4(insn.default)
        writer.write_i4(insn.low)
        writer.write_i4(insn.high)
        for offset in insn.offsets:
            writer.write_i4(offset)
