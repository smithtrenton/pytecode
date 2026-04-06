# cython: boundscheck=False, wraparound=False, cdivision=True
"""Serialize a ClassFile tree into JVM ``.class`` file bytes (JVMS §4)."""

from ._attributes_cy cimport (
    AppendFrameInfo,
    BootstrapMethodInfo,
    ChopFrameInfo,
    CodeAttr,
    ExceptionInfo,
    FullFrameInfo,
    InnerClassInfo,
    LineNumberInfo,
    LineNumberTableAttr,
    LocalVariableInfo,
    LocalVariableTableAttr,
    LocalVariableTypeInfo,
    LocalVariableTypeTableAttr,
    MethodParameterInfo,
    ObjectVariableInfo,
    RecordComponentInfo,
    SameFrameExtendedInfo,
    SameFrameInfo,
    SameLocals1StackItemFrameExtendedInfo,
    SameLocals1StackItemFrameInfo,
    StackMapTableAttr,
    UninitializedVariableInfo,
)
from ._instructions_cy cimport (
    Branch,
    BranchW,
    ByteValue,
    ConstPoolIndex,
    IInc,
    IIncW,
    InsnInfo,
    InvokeDynamic,
    InvokeInterface,
    LocalIndex,
    LocalIndexW,
    LookupSwitch,
    MatchOffsetPair,
    MultiANewArray,
    NewArray,
    ShortValue,
    TableSwitch,
)
from pytecode._internal._bytes_utils_cy import BytesWriter
from . import attributes, constant_pool, instructions
from .info import ClassFile, FieldInfo, MethodInfo

__all__ = ["ClassWriter"]


class ClassWriter:
    """Serializer that converts a ``ClassFile`` tree into JVM ``.class`` bytes.

    This writer walks every section of the ``ClassFile`` structure — constant
    pool, fields, methods, and attributes — and emits the binary encoding
    defined by the JVM class-file format (JVMS §4).
    """

    @staticmethod
    def write(object classfile):
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


def _write_classfile(object writer, object classfile):
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


def _iter_constant_pool_entries(list pool):
    if not pool:
        raise ValueError("constant pool must include slot 0")
    if pool[0] is not None:
        raise ValueError("constant pool slot 0 must be None")

    cdef list entries = []
    cdef bint expect_gap = False
    cdef int index
    cdef int pool_len = len(pool)
    for index in range(1, pool_len):
        entry = pool[index]
        if expect_gap:
            if entry is not None:
                raise ValueError(f"constant pool slot {index} must be empty after a Long/Double entry")
            expect_gap = False
            continue

        if entry is None:
            raise ValueError(f"constant pool slot {index} is unexpectedly empty")

        entries.append(entry)
        expect_gap = type(entry) is constant_pool.LongInfo or type(entry) is constant_pool.DoubleInfo

    if expect_gap:
        raise ValueError("constant pool is missing the trailing gap slot for a Long/Double entry")

    return entries


cdef _write_constant_pool_entry(object writer, object entry):
    cdef type t = type(entry)
    writer.write_u1(entry.tag)

    if t is constant_pool.ClassInfo:
        writer.write_u2(entry.name_index)
    elif t is constant_pool.StringInfo:
        writer.write_u2(entry.string_index)
    elif t is constant_pool.MethodTypeInfo:
        writer.write_u2(entry.descriptor_index)
    elif t is constant_pool.ModuleInfo:
        writer.write_u2(entry.name_index)
    elif t is constant_pool.PackageInfo:
        writer.write_u2(entry.name_index)
    elif t is constant_pool.FieldrefInfo:
        writer.write_u2(entry.class_index)
        writer.write_u2(entry.name_and_type_index)
    elif t is constant_pool.MethodrefInfo:
        writer.write_u2(entry.class_index)
        writer.write_u2(entry.name_and_type_index)
    elif t is constant_pool.InterfaceMethodrefInfo:
        writer.write_u2(entry.class_index)
        writer.write_u2(entry.name_and_type_index)
    elif t is constant_pool.NameAndTypeInfo:
        writer.write_u2(entry.name_index)
        writer.write_u2(entry.descriptor_index)
    elif t is constant_pool.DynamicInfo:
        writer.write_u2(entry.bootstrap_method_attr_index)
        writer.write_u2(entry.name_and_type_index)
    elif t is constant_pool.InvokeDynamicInfo:
        writer.write_u2(entry.bootstrap_method_attr_index)
        writer.write_u2(entry.name_and_type_index)
    elif t is constant_pool.IntegerInfo:
        writer.write_u4(entry.value_bytes)
    elif t is constant_pool.FloatInfo:
        writer.write_u4(entry.value_bytes)
    elif t is constant_pool.LongInfo:
        writer.write_u4(entry.high_bytes)
        writer.write_u4(entry.low_bytes)
    elif t is constant_pool.DoubleInfo:
        writer.write_u4(entry.high_bytes)
        writer.write_u4(entry.low_bytes)
    elif t is constant_pool.Utf8Info:
        writer.write_u2(len(entry.str_bytes))
        writer.write_bytes(entry.str_bytes)
    elif t is constant_pool.MethodHandleInfo:
        writer.write_u1(entry.reference_kind)
        writer.write_u2(entry.reference_index)
    else:
        raise ValueError(f"Unsupported constant-pool entry type: {type(entry).__name__}")


def _write_field_info(object writer, object field):
    writer.write_u2(int(field.access_flags))
    writer.write_u2(field.name_index)
    writer.write_u2(field.descriptor_index)
    _write_attributes(writer, field.attributes)


def _write_method_info(object writer, object method):
    writer.write_u2(int(method.access_flags))
    writer.write_u2(method.name_index)
    writer.write_u2(method.descriptor_index)
    _write_attributes(writer, method.attributes)


def _write_attributes(object writer, list attrs):
    writer.write_u2(len(attrs))
    for attr in attrs:
        _write_attribute(writer, attr)


cdef inline int _begin_attribute_write(object writer, int attribute_name_index):
    writer.write_u2(attribute_name_index)
    return writer.reserve_u4()


cdef inline void _finish_attribute_write(object writer, int payload_length_position, int payload_start):
    writer.patch_u4(payload_length_position, len(writer) - payload_start)


cdef _write_code_attr_payload(object writer, CodeAttr code_attr):
    cdef ExceptionInfo exception_info
    cdef int code_length_position
    cdef int code_start

    writer.write_u2(code_attr.max_stacks)
    writer.write_u2(code_attr.max_locals)
    code_length_position = writer.reserve_u4()
    code_start = len(writer)
    for insn in code_attr.code:
        _write_instruction(writer, insn, code_start)
    writer.patch_u4(code_length_position, len(writer) - code_start)
    writer.write_u2(len(code_attr.exception_table))
    for exception in code_attr.exception_table:
        exception_info = exception
        writer.write_u2(exception_info.start_pc)
        writer.write_u2(exception_info.end_pc)
        writer.write_u2(exception_info.handler_pc)
        writer.write_u2(exception_info.catch_type)
    _write_attributes(writer, code_attr.attributes)


cdef _write_stack_map_table_attr_payload(object writer, StackMapTableAttr stack_map_attr):
    writer.write_u2(len(stack_map_attr.entries))
    for entry in stack_map_attr.entries:
        _write_stack_map_frame_info(writer, entry)


cdef _write_line_number_table_attr_payload(object writer, LineNumberTableAttr line_number_table_attr):
    cdef LineNumberInfo line_number_info

    writer.write_u2(len(line_number_table_attr.line_number_table))
    for entry in line_number_table_attr.line_number_table:
        line_number_info = entry
        writer.write_u2(line_number_info.start_pc)
        writer.write_u2(line_number_info.line_number)


cdef _write_local_variable_table_attr_payload(object writer, LocalVariableTableAttr local_variable_table_attr):
    cdef LocalVariableInfo local_variable_info

    writer.write_u2(len(local_variable_table_attr.local_variable_table))
    for entry in local_variable_table_attr.local_variable_table:
        local_variable_info = entry
        writer.write_u2(local_variable_info.start_pc)
        writer.write_u2(local_variable_info.length)
        writer.write_u2(local_variable_info.name_index)
        writer.write_u2(local_variable_info.descriptor_index)
        writer.write_u2(local_variable_info.index)


cdef _write_local_variable_type_table_attr_payload(
    object writer,
    LocalVariableTypeTableAttr local_variable_type_table_attr,
):
    cdef LocalVariableTypeInfo local_variable_type_info

    writer.write_u2(len(local_variable_type_table_attr.local_variable_type_table))
    for entry in local_variable_type_table_attr.local_variable_type_table:
        local_variable_type_info = entry
        writer.write_u2(local_variable_type_info.start_pc)
        writer.write_u2(local_variable_type_info.length)
        writer.write_u2(local_variable_type_info.name_index)
        writer.write_u2(local_variable_type_info.signature_index)
        writer.write_u2(local_variable_type_info.index)


def _write_attribute(object writer, object attr):
    cdef type t = type(attr)
    cdef int payload_length_position
    cdef int payload_start
    cdef CodeAttr code_attr
    cdef StackMapTableAttr stack_map_attr
    cdef LineNumberTableAttr line_number_table_attr
    cdef LocalVariableTableAttr local_variable_table_attr
    cdef LocalVariableTypeTableAttr local_variable_type_table_attr

    if t is CodeAttr:
        code_attr = attr
        payload_length_position = _begin_attribute_write(writer, code_attr.attribute_name_index)
        payload_start = len(writer)
        _write_code_attr_payload(writer, code_attr)
        _finish_attribute_write(writer, payload_length_position, payload_start)
        return

    if t is StackMapTableAttr:
        stack_map_attr = attr
        payload_length_position = _begin_attribute_write(writer, stack_map_attr.attribute_name_index)
        payload_start = len(writer)
        _write_stack_map_table_attr_payload(writer, stack_map_attr)
        _finish_attribute_write(writer, payload_length_position, payload_start)
        return

    if t is LineNumberTableAttr:
        line_number_table_attr = attr
        payload_length_position = _begin_attribute_write(writer, line_number_table_attr.attribute_name_index)
        payload_start = len(writer)
        _write_line_number_table_attr_payload(writer, line_number_table_attr)
        _finish_attribute_write(writer, payload_length_position, payload_start)
        return

    if t is LocalVariableTableAttr:
        local_variable_table_attr = attr
        payload_length_position = _begin_attribute_write(writer, local_variable_table_attr.attribute_name_index)
        payload_start = len(writer)
        _write_local_variable_table_attr_payload(writer, local_variable_table_attr)
        _finish_attribute_write(writer, payload_length_position, payload_start)
        return

    if t is LocalVariableTypeTableAttr:
        local_variable_type_table_attr = attr
        payload_length_position = _begin_attribute_write(writer, local_variable_type_table_attr.attribute_name_index)
        payload_start = len(writer)
        _write_local_variable_type_table_attr_payload(writer, local_variable_type_table_attr)
        _finish_attribute_write(writer, payload_length_position, payload_start)
        return

    payload_writer = BytesWriter()
    _write_attribute_payload(payload_writer, attr)
    payload = payload_writer.to_bytes()

    writer.write_u2(attr.attribute_name_index)
    writer.write_u4(len(payload))
    writer.write_bytes(payload)


cdef _write_attribute_payload(object writer, object attr):
    cdef type t = type(attr)
    if t is attributes.SyntheticAttr or t is attributes.DeprecatedAttr:
        return

    if t is attributes.ConstantValueAttr:
        writer.write_u2(attr.constantvalue_index)
        return

    if t is attributes.SignatureAttr:
        writer.write_u2(attr.signature_index)
        return

    if t is attributes.SourceFileAttr:
        writer.write_u2(attr.sourcefile_index)
        return

    if t is attributes.ModuleMainClassAttr:
        writer.write_u2(attr.main_class_index)
        return

    if t is attributes.NestHostAttr:
        writer.write_u2(attr.host_class_index)
        return

    cdef BootstrapMethodInfo bootstrap_method_info
    cdef InnerClassInfo inner_class_info
    cdef MethodParameterInfo method_parameter_info
    cdef RecordComponentInfo record_component_info

    if t is CodeAttr:
        _write_code_attr_payload(writer, attr)
        return

    if t is StackMapTableAttr:
        _write_stack_map_table_attr_payload(writer, attr)
        return

    if t is attributes.ExceptionsAttr:
        writer.write_u2(len(attr.exception_index_table))
        for exception_index in attr.exception_index_table:
            writer.write_u2(exception_index)
        return

    if t is attributes.InnerClassesAttr:
        writer.write_u2(len(attr.classes))
        for entry in attr.classes:
            inner_class_info = entry
            writer.write_u2(inner_class_info.inner_class_info_index)
            writer.write_u2(inner_class_info.outer_class_info_index)
            writer.write_u2(inner_class_info.inner_name_index)
            writer.write_u2(int(inner_class_info.inner_class_access_flags))
        return

    if t is attributes.EnclosingMethodAttr:
        writer.write_u2(attr.class_index)
        writer.write_u2(attr.method_index)
        return

    if t is attributes.SourceDebugExtensionAttr:
        writer.write_bytes(attr.debug_extension.encode("utf-8"))
        return

    if t is LineNumberTableAttr:
        _write_line_number_table_attr_payload(writer, attr)
        return

    if t is LocalVariableTableAttr:
        _write_local_variable_table_attr_payload(writer, attr)
        return

    if t is LocalVariableTypeTableAttr:
        _write_local_variable_type_table_attr_payload(writer, attr)
        return

    if t is attributes.RuntimeVisibleAnnotationsAttr or t is attributes.RuntimeInvisibleAnnotationsAttr:
        writer.write_u2(len(attr.annotations))
        for annotation in attr.annotations:
            _write_annotation_info(writer, annotation)
        return

    if t is attributes.RuntimeVisibleParameterAnnotationsAttr or t is attributes.RuntimeInvisibleParameterAnnotationsAttr:
        writer.write_u1(len(attr.parameter_annotations))
        for parameter in attr.parameter_annotations:
            writer.write_u2(len(parameter.annotations))
            for annotation in parameter.annotations:
                _write_annotation_info(writer, annotation)
        return

    if t is attributes.RuntimeVisibleTypeAnnotationsAttr or t is attributes.RuntimeInvisibleTypeAnnotationsAttr:
        writer.write_u2(len(attr.annotations))
        for annotation in attr.annotations:
            _write_type_annotation_info(writer, annotation)
        return

    if t is attributes.AnnotationDefaultAttr:
        _write_element_value_info(writer, attr.default_value)
        return

    if t is attributes.BootstrapMethodsAttr:
        writer.write_u2(len(attr.bootstrap_methods))
        for method in attr.bootstrap_methods:
            bootstrap_method_info = method
            writer.write_u2(bootstrap_method_info.bootstrap_method_ref)
            writer.write_u2(len(bootstrap_method_info.boostrap_arguments))
            for argument in bootstrap_method_info.boostrap_arguments:
                writer.write_u2(argument)
        return

    if t is attributes.MethodParametersAttr:
        writer.write_u1(len(attr.parameters))
        for parameter in attr.parameters:
            method_parameter_info = parameter
            writer.write_u2(method_parameter_info.name_index)
            writer.write_u2(int(method_parameter_info.access_flags))
        return

    if t is attributes.ModuleAttr:
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

    if t is attributes.ModulePackagesAttr:
        writer.write_u2(len(attr.package_index))
        for package_index in attr.package_index:
            writer.write_u2(package_index)
        return

    if t is attributes.NestMembersAttr:
        writer.write_u2(len(attr.classes))
        for class_index in attr.classes:
            writer.write_u2(class_index)
        return

    if t is attributes.RecordAttr:
        writer.write_u2(len(attr.components))
        for component in attr.components:
            record_component_info = component
            writer.write_u2(record_component_info.name_index)
            writer.write_u2(record_component_info.descriptor_index)
            _write_attributes(writer, record_component_info.attributes)
        return

    if t is attributes.PermittedSubclassesAttr:
        writer.write_u2(len(attr.classes))
        for class_index in attr.classes:
            writer.write_u2(class_index)
        return

    if t is attributes.UnimplementedAttr:
        writer.write_bytes(attr.info)
        return

    raise ValueError(f"Unsupported attribute type: {type(attr).__name__}")


cdef _write_stack_map_frame_info(object writer, object entry):
    cdef type t = type(entry)
    writer.write_u1(entry.frame_type)

    if t is SameFrameInfo:
        return
    if t is SameLocals1StackItemFrameInfo:
        _write_verification_type_info(writer, entry.stack)
        return
    if t is SameLocals1StackItemFrameExtendedInfo:
        writer.write_u2(entry.offset_delta)
        _write_verification_type_info(writer, entry.stack)
        return
    if t is ChopFrameInfo:
        writer.write_u2(entry.offset_delta)
        return
    if t is SameFrameExtendedInfo:
        writer.write_u2(entry.offset_delta)
        return
    if t is AppendFrameInfo:
        writer.write_u2(entry.offset_delta)
        for local in entry.locals:
            _write_verification_type_info(writer, local)
        return
    if t is FullFrameInfo:
        writer.write_u2(entry.offset_delta)
        writer.write_u2(len(entry.locals))
        for local in entry.locals:
            _write_verification_type_info(writer, local)
        writer.write_u2(len(entry.stack))
        for stack_item in entry.stack:
            _write_verification_type_info(writer, stack_item)
        return

    raise ValueError(f"Unsupported stack-map frame type: {type(entry).__name__}")


cdef _write_verification_type_info(object writer, object entry):
    cdef type t = type(entry)
    writer.write_u1(int(entry.tag))

    if t is ObjectVariableInfo:
        writer.write_u2(entry.cpool_index)
    elif t is UninitializedVariableInfo:
        writer.write_u2(entry.offset)


cdef _write_annotation_info(object writer, object annotation):
    writer.write_u2(annotation.type_index)
    writer.write_u2(len(annotation.element_value_pairs))
    for pair in annotation.element_value_pairs:
        writer.write_u2(pair.element_name_index)
        _write_element_value_info(writer, pair.element_value)


cdef _write_element_value_info(object writer, object element_value):
    cdef int tag = _element_value_tag(element_value.tag)
    writer.write_u1(tag)

    if tag in {ord("B"), ord("C"), ord("D"), ord("F"), ord("I"), ord("J"), ord("S"), ord("Z"), ord("s")}:
        if type(element_value.value) is not attributes.ConstValueInfo:
            raise ValueError("const element value must carry ConstValueInfo")
        writer.write_u2(element_value.value.const_value_index)
        return

    if tag == ord("e"):
        if type(element_value.value) is not attributes.EnumConstantValueInfo:
            raise ValueError("enum element value must carry EnumConstantValueInfo")
        writer.write_u2(element_value.value.type_name_index)
        writer.write_u2(element_value.value.const_name_index)
        return

    if tag == ord("c"):
        if type(element_value.value) is not attributes.ClassInfoValueInfo:
            raise ValueError("class element value must carry ClassInfoValueInfo")
        writer.write_u2(element_value.value.class_info_index)
        return

    if tag == ord("@"):
        if type(element_value.value) is not attributes.AnnotationInfo:
            raise ValueError("annotation element value must carry AnnotationInfo")
        _write_annotation_info(writer, element_value.value)
        return

    if tag == ord("["):
        if type(element_value.value) is not attributes.ArrayValueInfo:
            raise ValueError("array element value must carry ArrayValueInfo")
        writer.write_u2(len(element_value.value.values))
        for nested in element_value.value.values:
            _write_element_value_info(writer, nested)
        return

    raise ValueError(f"Unsupported element-value tag: {tag!r}")


cdef int _element_value_tag(object tag):
    cdef int val
    if type(tag) is int:
        val = <int>tag
        if not 0 <= val <= 255:
            raise ValueError(f"element-value tag must fit in u1, got {val}")
        return val
    if len(tag) != 1:
        raise ValueError(f"element-value tag must be a single character, got {tag!r}")
    return ord(tag)


cdef _write_type_annotation_info(object writer, object annotation):
    writer.write_u1(annotation.target_type)
    _write_target_info(writer, annotation.target_info)
    _write_type_path_info(writer, annotation.target_path)
    writer.write_u2(annotation.type_index)
    writer.write_u2(len(annotation.element_value_pairs))
    for pair in annotation.element_value_pairs:
        writer.write_u2(pair.element_name_index)
        _write_element_value_info(writer, pair.element_value)


cdef _write_target_info(object writer, object target_info):
    cdef type t = type(target_info)
    if t is attributes.TypeParameterTargetInfo:
        writer.write_u1(target_info.type_parameter_index)
        return
    if t is attributes.SupertypeTargetInfo:
        writer.write_u2(target_info.supertype_index)
        return
    if t is attributes.TypeParameterBoundTargetInfo:
        writer.write_u1(target_info.type_parameter_index)
        writer.write_u1(target_info.bound_index)
        return
    if t is attributes.EmptyTargetInfo:
        return
    if t is attributes.FormalParameterTargetInfo:
        writer.write_u1(target_info.formal_parameter_index)
        return
    if t is attributes.ThrowsTargetInfo:
        writer.write_u2(target_info.throws_type_index)
        return
    if t is attributes.LocalvarTargetInfo:
        writer.write_u2(len(target_info.table))
        for table_entry in target_info.table:
            writer.write_u2(table_entry.start_pc)
            writer.write_u2(table_entry.length)
            writer.write_u2(table_entry.index)
        return
    if t is attributes.CatchTargetInfo:
        writer.write_u2(target_info.exception_table_index)
        return
    if t is attributes.OffsetTargetInfo:
        writer.write_u2(target_info.offset)
        return
    if t is attributes.TypeArgumentTargetInfo:
        writer.write_u2(target_info.offset)
        writer.write_u1(target_info.type_argument_index)
        return

    raise ValueError(f"Unsupported target-info type: {type(target_info).__name__}")


cdef _write_type_path_info(object writer, object type_path):
    writer.write_u1(len(type_path.path))
    for path_item in type_path.path:
        writer.write_u1(path_item.type_path_kind)
        writer.write_u1(path_item.type_argument_index)


cdef inline void _write_code_alignment(object writer, int code_start):
    cdef int remainder = (len(writer) - code_start) % 4
    if remainder != 0:
        writer.write_bytes(b"\x00" * (4 - remainder))


cdef _write_instruction(object writer, object insn, int code_start):
    cdef type t = type(insn)
    cdef InsnInfo base_insn
    cdef LocalIndexW local_index_w
    cdef IIncW iinc_w
    cdef LocalIndex local_index
    cdef ConstPoolIndex const_pool_index
    cdef ByteValue byte_value
    cdef ShortValue short_value
    cdef Branch branch
    cdef BranchW branch_w
    cdef IInc iinc
    cdef InvokeDynamic invoke_dynamic
    cdef InvokeInterface invoke_interface
    cdef MultiANewArray multi_anew_array
    cdef NewArray new_array
    cdef LookupSwitch lookup_switch
    cdef MatchOffsetPair pair
    cdef TableSwitch table_switch
    cdef Py_ssize_t offset
    if t is LocalIndexW:
        local_index_w = insn
        writer.write_u1(int(instructions.InsnInfoType.WIDE))
        writer.write_u1(int(local_index_w.type) - int(instructions.InsnInfoType.WIDE))
        writer.write_u2(local_index_w.index)
        return

    if t is IIncW:
        iinc_w = insn
        writer.write_u1(int(instructions.InsnInfoType.WIDE))
        writer.write_u1(int(iinc_w.type) - int(instructions.InsnInfoType.WIDE))
        writer.write_u2(iinc_w.index)
        writer.write_i2(iinc_w.value)
        return

    base_insn = insn
    writer.write_u1(int(base_insn.type))

    if t is LocalIndex:
        local_index = insn
        writer.write_u1(local_index.index)
    elif t is ConstPoolIndex:
        const_pool_index = insn
        writer.write_u2(const_pool_index.index)
    elif t is ByteValue:
        byte_value = insn
        writer.write_i1(byte_value.value)
    elif t is ShortValue:
        short_value = insn
        writer.write_i2(short_value.value)
    elif t is Branch:
        branch = insn
        writer.write_i2(branch.offset)
    elif t is BranchW:
        branch_w = insn
        writer.write_i4(branch_w.offset)
    elif t is IInc:
        iinc = insn
        writer.write_u1(iinc.index)
        writer.write_i1(iinc.value)
    elif t is InvokeDynamic:
        invoke_dynamic = insn
        if len(invoke_dynamic.unused) != 2:
            raise ValueError("InvokeDynamic unused bytes must be exactly 2 bytes")
        writer.write_u2(invoke_dynamic.index)
        writer.write_bytes(invoke_dynamic.unused)
    elif t is InvokeInterface:
        invoke_interface = insn
        if len(invoke_interface.unused) != 1:
            raise ValueError("InvokeInterface unused bytes must be exactly 1 byte")
        writer.write_u2(invoke_interface.index)
        writer.write_u1(invoke_interface.count)
        writer.write_bytes(invoke_interface.unused)
    elif t is MultiANewArray:
        multi_anew_array = insn
        writer.write_u2(multi_anew_array.index)
        writer.write_u1(multi_anew_array.dimensions)
    elif t is NewArray:
        new_array = insn
        writer.write_u1(int(new_array.atype))
    elif t is LookupSwitch:
        lookup_switch = insn
        _write_code_alignment(writer, code_start)
        writer.write_i4(lookup_switch.default)
        writer.write_u4(len(lookup_switch.pairs))
        for pair in lookup_switch.pairs:
            writer.write_i4(pair.match)
            writer.write_i4(pair.offset)
    elif t is TableSwitch:
        table_switch = insn
        _write_code_alignment(writer, code_start)
        writer.write_i4(table_switch.default)
        writer.write_i4(table_switch.low)
        writer.write_i4(table_switch.high)
        for offset in table_switch.offsets:
            writer.write_i4(offset)
