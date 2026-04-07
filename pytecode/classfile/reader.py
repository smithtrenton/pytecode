"""Parse JVM ``.class`` file bytes into a :class:`ClassFile` tree.

This module implements a single-pass reader that deserialises the binary
class-file format defined in *The Java Virtual Machine Specification* (JVMS §4)
into the in-memory ``ClassFile`` structure exposed by :mod:`pytecode.classfile.info`.
"""

from __future__ import annotations

import os

from .._internal.bytes_utils import BytesReader
from . import attributes, constant_pool, constants, info, instructions
from ._rust_bridge import coerce_python_classfile
from .modified_utf8 import decode_modified_utf8

__all__ = ["ClassReader", "MalformedClassException"]


class MalformedClassException(Exception):
    """Raised when the input bytes do not conform to the JVM class-file format (JVMS §4)."""


class ClassReader(BytesReader):
    """Single-pass parser that converts ``.class`` file bytes into a :class:`~pytecode.classfile.info.ClassFile` tree.

    The reader walks the binary layout defined in JVMS §4.1, populating the
    constant pool first (§4.4) and then deserialising fields, methods, and
    attributes in declaration order.  The resulting :attr:`class_info` object
    mirrors the on-disk ``ClassFile`` structure.
    """

    class_info: info.ClassFile
    _rust_reader: object | None

    def __init__(self, bytes_or_bytearray: bytes | bytearray) -> None:
        """Initialise the reader and immediately parse the class-file bytes.

        Args:
            bytes_or_bytearray: Raw bytes of a ``.class`` file.

        Raises:
            MalformedClassException: If the bytes are not a valid class file.
        """
        super().__init__(bytes_or_bytearray)
        self.constant_pool: list[constant_pool.ConstantPoolInfo | None] = []
        self._rust_reader = None
        try:
            from pytecode import _rust
        except ModuleNotFoundError:
            self.read_class()
            return

        try:
            self._rust_reader = _rust.ClassReader.from_bytes(bytes_or_bytearray)
        except _rust.MalformedClassException:
            self.read_class()
            return

        self.class_info = coerce_python_classfile(self._rust_reader.class_info)
        self.constant_pool = self.class_info.constant_pool

    @classmethod
    def from_file(cls, path: str | os.PathLike[str]) -> ClassReader:
        """Construct a :class:`ClassReader` from a ``.class`` file on disk.

        Args:
            path: Filesystem path to the ``.class`` file.

        Returns:
            A fully-parsed :class:`ClassReader` instance.
        """
        with open(path, "rb") as f:
            file_bytes = f.read()
        return cls(file_bytes)

    @classmethod
    def from_bytes(cls, bytes_or_bytearray: bytes | bytearray) -> ClassReader:
        """Construct a :class:`ClassReader` from raw bytes.

        Args:
            bytes_or_bytearray: Raw bytes of a ``.class`` file.

        Returns:
            A fully-parsed :class:`ClassReader` instance.
        """
        return cls(bytes_or_bytearray)

    def read_constant_pool_index(self, index: int) -> tuple[constant_pool.ConstantPoolInfo, int]:
        """Read a single constant-pool entry at the given logical index (JVMS §4.4).

        Args:
            index: One-based constant-pool index for the entry being read.

        Returns:
            A tuple of the parsed constant-pool info object and the number of
            extra index slots consumed (1 for ``long``/``double``, else 0).

        Raises:
            ValueError: If the constant-pool tag is unrecognised.
        """
        index_extra, offset, tag = 0, self.offset, self.read_u1()
        cp_type = constant_pool.ConstantPoolInfoType(tag)

        if cp_type is constant_pool.ConstantPoolInfoType.CLASS:
            cp_info = constant_pool.ClassInfo(index, offset, tag, self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.STRING:
            cp_info = constant_pool.StringInfo(index, offset, tag, self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.METHOD_TYPE:
            cp_info = constant_pool.MethodTypeInfo(index, offset, tag, self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.MODULE:
            cp_info = constant_pool.ModuleInfo(index, offset, tag, self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.PACKAGE:
            cp_info = constant_pool.PackageInfo(index, offset, tag, self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.FIELD_REF:
            cp_info = constant_pool.FieldrefInfo(index, offset, tag, self.read_u2(), self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.METHOD_REF:
            cp_info = constant_pool.MethodrefInfo(index, offset, tag, self.read_u2(), self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.INTERFACE_METHOD_REF:
            cp_info = constant_pool.InterfaceMethodrefInfo(index, offset, tag, self.read_u2(), self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.NAME_AND_TYPE:
            cp_info = constant_pool.NameAndTypeInfo(index, offset, tag, self.read_u2(), self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.DYNAMIC:
            cp_info = constant_pool.DynamicInfo(index, offset, tag, self.read_u2(), self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.INVOKE_DYNAMIC:
            cp_info = constant_pool.InvokeDynamicInfo(index, offset, tag, self.read_u2(), self.read_u2())
        elif cp_type is constant_pool.ConstantPoolInfoType.INTEGER:
            cp_info = constant_pool.IntegerInfo(index, offset, tag, self.read_u4())
        elif cp_type is constant_pool.ConstantPoolInfoType.FLOAT:
            cp_info = constant_pool.FloatInfo(index, offset, tag, self.read_u4())
        elif cp_type is constant_pool.ConstantPoolInfoType.LONG:
            cp_info = constant_pool.LongInfo(index, offset, tag, self.read_u4(), self.read_u4())
            index_extra = 1
        elif cp_type is constant_pool.ConstantPoolInfoType.DOUBLE:
            cp_info = constant_pool.DoubleInfo(index, offset, tag, self.read_u4(), self.read_u4())
            index_extra = 1
        elif cp_type is constant_pool.ConstantPoolInfoType.UTF8:
            length = self.read_u2()
            str_bytes = self.read_bytes(length)
            cp_info = constant_pool.Utf8Info(index, offset, tag, length, str_bytes)
        elif cp_type is constant_pool.ConstantPoolInfoType.METHOD_HANDLE:
            cp_info = constant_pool.MethodHandleInfo(index, offset, tag, self.read_u1(), self.read_u2())
        else:
            raise ValueError(f"Unknown ConstantPoolInfoType: {cp_type}")
        return cp_info, index_extra

    def read_align_bytes(self, current_offset: int) -> bytes:
        """Read and discard padding bytes to reach 4-byte alignment.

        Used by ``tableswitch`` and ``lookupswitch`` instructions whose
        operands must be 4-byte aligned (JVMS §6.5).

        Args:
            current_offset: Current bytecode offset within the method body.

        Returns:
            The consumed padding bytes (0–3 bytes).
        """
        align_bytes = (4 - current_offset % 4) % 4
        return self.read_bytes(align_bytes)

    def read_instruction(self, current_method_offset: int) -> instructions.InsnInfo:
        """Read a single JVM bytecode instruction (JVMS §6.5).

        Args:
            current_method_offset: Byte offset of this instruction relative to
                the start of the method's ``Code`` attribute bytecode array.

        Returns:
            The decoded instruction info object.

        Raises:
            Exception: If the opcode or its ``wide`` variant is invalid.
        """
        opcode = self.read_u1()
        inst_type = instructions.InsnInfoType(opcode)
        instinfo = inst_type.instinfo
        if instinfo is instructions.LocalIndex:
            return instructions.LocalIndex(inst_type, current_method_offset, self.read_u1())
        elif instinfo is instructions.ConstPoolIndex:
            return instructions.ConstPoolIndex(inst_type, current_method_offset, self.read_u2())
        elif instinfo is instructions.ByteValue:
            return instructions.ByteValue(inst_type, current_method_offset, self.read_i1())
        elif instinfo is instructions.ShortValue:
            return instructions.ShortValue(inst_type, current_method_offset, self.read_i2())
        elif instinfo is instructions.Branch:
            return instructions.Branch(inst_type, current_method_offset, self.read_i2())
        elif instinfo is instructions.BranchW:
            return instructions.BranchW(inst_type, current_method_offset, self.read_i4())
        elif instinfo is instructions.IInc:
            index, value = self.read_u1(), self.read_i1()
            return instructions.IInc(inst_type, current_method_offset, index, value)
        elif instinfo is instructions.InvokeDynamic:
            index, unused = self.read_u2(), self.read_bytes(2)
            return instructions.InvokeDynamic(inst_type, current_method_offset, index, unused)
        elif instinfo is instructions.InvokeInterface:
            index, count, unused = self.read_u2(), self.read_u1(), self.read_bytes(1)
            return instructions.InvokeInterface(inst_type, current_method_offset, index, count, unused)
        elif instinfo is instructions.MultiANewArray:
            index, dimensions = self.read_u2(), self.read_u1()
            return instructions.MultiANewArray(inst_type, current_method_offset, index, dimensions)
        elif instinfo is instructions.NewArray:
            atype = instructions.ArrayType(self.read_u1())
            return instructions.NewArray(inst_type, current_method_offset, atype)
        elif instinfo is instructions.LookupSwitch:
            self.read_align_bytes(current_method_offset + 1)
            default, npairs = self.read_i4(), self.read_u4()
            pairs = [instructions.MatchOffsetPair(self.read_i4(), self.read_i4()) for _ in range(npairs)]
            return instructions.LookupSwitch(inst_type, current_method_offset, default, npairs, pairs)
        elif instinfo is instructions.TableSwitch:
            self.read_align_bytes(current_method_offset + 1)
            default, low, high = self.read_i4(), self.read_i4(), self.read_i4()
            offsets = [self.read_i4() for _ in range(high - low + 1)]
            return instructions.TableSwitch(inst_type, current_method_offset, default, low, high, offsets)
        elif inst_type is instructions.InsnInfoType.WIDE:
            wide_opcode = self.read_u1()
            wide_inst_type = instructions.InsnInfoType(opcode + wide_opcode)
            if wide_inst_type.instinfo is instructions.LocalIndexW:
                return instructions.LocalIndexW(wide_inst_type, current_method_offset, self.read_u2())
            elif wide_inst_type.instinfo is instructions.IIncW:
                index, value = self.read_u2(), self.read_i2()
                return instructions.IIncW(wide_inst_type, current_method_offset, index, value)
        elif instinfo is instructions.InsnInfo:
            return instructions.InsnInfo(inst_type, current_method_offset)

        raise Exception(f"Invalid InstInfoType: {inst_type.name} {inst_type.instinfo}")

    def read_code_bytes(self, code_length: int) -> list[instructions.InsnInfo]:
        """Read the full bytecode array of a ``Code`` attribute (JVMS §4.7.3).

        Args:
            code_length: Number of bytes in the bytecode array.

        Returns:
            Ordered list of decoded instructions.
        """
        start_method_offset = self.offset
        results: list[instructions.InsnInfo] = []
        while (current_method_offset := self.offset - start_method_offset) < code_length:
            insn = self.read_instruction(current_method_offset)
            results.append(insn)
        return results

    def read_verification_type_info(self) -> attributes.VerificationTypeInfo:
        """Read a single ``verification_type_info`` union (JVMS §4.7.4).

        Returns:
            The decoded verification-type info variant.

        Raises:
            ValueError: If the verification-type tag is unrecognised.
        """
        tag = self.read_u1()
        match tag:
            case constants.VerificationType.TOP:
                return attributes.TopVariableInfo(tag)
            case constants.VerificationType.INTEGER:
                return attributes.IntegerVariableInfo(tag)
            case constants.VerificationType.FLOAT:
                return attributes.FloatVariableInfo(tag)
            case constants.VerificationType.DOUBLE:
                return attributes.DoubleVariableInfo(tag)
            case constants.VerificationType.LONG:
                return attributes.LongVariableInfo(tag)
            case constants.VerificationType.NULL:
                return attributes.NullVariableInfo(tag)
            case constants.VerificationType.UNINITIALIZED_THIS:
                return attributes.UninitializedThisVariableInfo(tag)
            case constants.VerificationType.OBJECT:
                return attributes.ObjectVariableInfo(tag, self.read_u2())
            case constants.VerificationType.UNINITIALIZED:
                return attributes.UninitializedVariableInfo(tag, self.read_u2())
            case _:
                raise ValueError(f"Unknown verification type tag: {tag}")

    def read_element_value_info(self) -> attributes.ElementValueInfo:
        """Read an ``element_value`` structure from an annotation (JVMS §4.7.16.1).

        Returns:
            The decoded element-value info.

        Raises:
            ValueError: If the element-value tag character is unrecognised.
        """
        tag = self.read_u1().to_bytes(1, "big").decode("ascii")

        match tag:
            case x if x in ("B", "C", "D", "F", "I", "J", "S", "Z", "s"):
                return attributes.ElementValueInfo(tag, attributes.ConstValueInfo(self.read_u2()))
            case "e":
                return attributes.ElementValueInfo(
                    tag,
                    attributes.EnumConstantValueInfo(self.read_u2(), self.read_u2()),
                )
            case "c":
                return attributes.ElementValueInfo(tag, attributes.ClassInfoValueInfo(self.read_u2()))
            case "@":
                return attributes.ElementValueInfo(tag, self.read_annotation_info())
            case "[":
                num_values = self.read_u2()
                values = [self.read_element_value_info() for _ in range(num_values)]
                return attributes.ElementValueInfo(tag, attributes.ArrayValueInfo(num_values, values))
            case _:
                raise ValueError(f"Unknown element value tag: {tag}")

    def read_annotation_info(self) -> attributes.AnnotationInfo:
        """Read an ``annotation`` structure (JVMS §4.7.16).

        Returns:
            The decoded annotation info including its element-value pairs.
        """
        type_index = self.read_u2()
        num_element_value_pairs = self.read_u2()
        element_value_pairs = [
            attributes.ElementValuePairInfo(self.read_u2(), self.read_element_value_info())
            for _ in range(num_element_value_pairs)
        ]
        return attributes.AnnotationInfo(type_index, num_element_value_pairs, element_value_pairs)

    def read_target_info(self, target_type: int) -> attributes.TargetInfo:
        """Read a ``target_info`` union for a type annotation (JVMS §4.7.20).

        Args:
            target_type: The ``target_type`` byte that selects the union variant.

        Returns:
            The decoded target info variant.

        Raises:
            ValueError: If the target type is unrecognised.
        """
        match target_type:
            case x if x in constants.TargetInfoType.TYPE_PARAMETER.value:
                return attributes.TypeParameterTargetInfo(self.read_u1())
            case x if x in constants.TargetInfoType.SUPERTYPE.value:
                return attributes.SupertypeTargetInfo(self.read_u2())
            case x if x in constants.TargetInfoType.TYPE_PARAMETER_BOUND.value:
                return attributes.TypeParameterBoundTargetInfo(self.read_u1(), self.read_u1())
            case x if x in constants.TargetInfoType.EMPTY.value:
                return attributes.EmptyTargetInfo()
            case x if x in constants.TargetInfoType.FORMAL_PARAMETER.value:
                return attributes.FormalParameterTargetInfo(self.read_u1())
            case x if x in constants.TargetInfoType.THROWS.value:
                return attributes.ThrowsTargetInfo(self.read_u2())
            case x if x in constants.TargetInfoType.LOCALVAR.value:
                table_length = self.read_u2()
                table = [
                    attributes.TableInfo(self.read_u2(), self.read_u2(), self.read_u2()) for _ in range(table_length)
                ]
                return attributes.LocalvarTargetInfo(table_length, table)
            case x if x in constants.TargetInfoType.CATCH.value:
                return attributes.CatchTargetInfo(self.read_u2())
            case x if x in constants.TargetInfoType.OFFSET.value:
                return attributes.OffsetTargetInfo(self.read_u2())
            case x if x in constants.TargetInfoType.TYPE_ARGUMENT.value:
                return attributes.TypeArgumentTargetInfo(self.read_u2(), self.read_u1())
            case _:
                raise ValueError(f"Unknown target info type: {target_type}")

    def read_target_path(self) -> attributes.TypePathInfo:
        """Read a ``type_path`` structure for a type annotation (JVMS §4.7.20.2).

        Returns:
            The decoded type-path info.
        """
        path_length = self.read_u1()
        path = [attributes.PathInfo(self.read_u1(), self.read_u1()) for _ in range(path_length)]
        return attributes.TypePathInfo(path_length, path)

    def read_type_annotation_info(self) -> attributes.TypeAnnotationInfo:
        """Read a ``type_annotation`` structure (JVMS §4.7.20).

        Returns:
            The decoded type-annotation info.
        """
        target_type = self.read_u1()
        target_info = self.read_target_info(target_type)
        target_path = self.read_target_path()
        type_index = self.read_u2()
        num_element_value_pairs = self.read_u2()
        element_value_pairs = [
            attributes.ElementValuePairInfo(self.read_u2(), self.read_element_value_info())
            for _ in range(num_element_value_pairs)
        ]
        return attributes.TypeAnnotationInfo(
            target_type,
            target_info,
            target_path,
            type_index,
            num_element_value_pairs,
            element_value_pairs,
        )

    def read_attribute(self) -> attributes.AttributeInfo:
        """Read a single ``attribute_info`` structure (JVMS §4.7).

        Recognised attribute names are decoded into their specific subtypes;
        unknown attributes are returned as :class:`~pytecode.classfile.attributes.UnimplementedAttr`.

        Returns:
            The decoded attribute info.

        Raises:
            ValueError: If the attribute name index does not reference a
                ``CONSTANT_Utf8_info`` entry.
        """
        name_index, length = self.read_u2(), self.read_u4()

        name_cp = self.constant_pool[name_index]
        if not isinstance(name_cp, constant_pool.Utf8Info):
            raise ValueError(f"name_index({name_index}) should be Utf8Info, not {type(name_cp)}")

        name = decode_modified_utf8(name_cp.str_bytes)
        attr_type = attributes.AttributeInfoType(name)

        if attr_type is attributes.AttributeInfoType.SYNTHETIC:
            return attributes.SyntheticAttr(name_index, length)

        elif attr_type is attributes.AttributeInfoType.DEPRECATED:
            return attributes.DeprecatedAttr(name_index, length)

        elif attr_type is attributes.AttributeInfoType.CONSTANT_VALUE:
            return attributes.ConstantValueAttr(name_index, length, self.read_u2())

        elif attr_type is attributes.AttributeInfoType.SIGNATURE:
            return attributes.SignatureAttr(name_index, length, self.read_u2())

        elif attr_type is attributes.AttributeInfoType.SOURCE_FILE:
            return attributes.SourceFileAttr(name_index, length, self.read_u2())

        elif attr_type is attributes.AttributeInfoType.MODULE_MAIN_CLASS:
            return attributes.ModuleMainClassAttr(name_index, length, self.read_u2())

        elif attr_type is attributes.AttributeInfoType.NEST_HOST:
            return attributes.NestHostAttr(name_index, length, self.read_u2())

        elif attr_type is attributes.AttributeInfoType.CODE:
            max_stack, max_locals = self.read_u2(), self.read_u2()
            code_length = self.read_u4()
            code = self.read_code_bytes(code_length)
            exception_table_length = self.read_u2()
            exception_table = [
                attributes.ExceptionInfo(self.read_u2(), self.read_u2(), self.read_u2(), self.read_u2())
                for _ in range(exception_table_length)
            ]
            attributes_count = self.read_u2()
            attributes_list = [self.read_attribute() for _ in range(attributes_count)]
            return attributes.CodeAttr(
                name_index,
                length,
                max_stack,
                max_locals,
                code_length,
                code,
                exception_table_length,
                exception_table,
                attributes_count,
                attributes_list,
            )

        elif attr_type is attributes.AttributeInfoType.STACK_MAP_TABLE:
            number_of_entries = self.read_u2()
            entries: list[attributes.StackMapFrameInfo] = []
            for _ in range(number_of_entries):
                frame_type = self.read_u1()

                match frame_type:
                    case x if x in range(0, 64):
                        entries.append(attributes.SameFrameInfo(frame_type))
                    case x if x in range(64, 128):
                        entries.append(
                            attributes.SameLocals1StackItemFrameInfo(frame_type, self.read_verification_type_info())
                        )
                    case 247:
                        entries.append(
                            attributes.SameLocals1StackItemFrameExtendedInfo(
                                frame_type,
                                self.read_u2(),
                                self.read_verification_type_info(),
                            )
                        )
                    case x if x in range(248, 251):
                        entries.append(attributes.ChopFrameInfo(frame_type, self.read_u2()))
                    case 251:
                        entries.append(attributes.SameFrameExtendedInfo(frame_type, self.read_u2()))
                    case x if x in range(252, 255):
                        offset_delta = self.read_u2()
                        verification_type_infos = [self.read_verification_type_info() for __ in range(frame_type - 251)]
                        entries.append(attributes.AppendFrameInfo(frame_type, offset_delta, verification_type_infos))
                    case 255:
                        offset_delta = self.read_u2()
                        number_of_locals = self.read_u2()
                        locals = [self.read_verification_type_info() for __ in range(number_of_locals)]
                        number_of_stack_items = self.read_u2()
                        stack = [self.read_verification_type_info() for __ in range(number_of_stack_items)]
                        entries.append(
                            attributes.FullFrameInfo(
                                frame_type,
                                offset_delta,
                                number_of_locals,
                                locals,
                                number_of_stack_items,
                                stack,
                            )
                        )
                    case _:
                        raise ValueError(f"Unknown stack map frame type: {frame_type}")

            return attributes.StackMapTableAttr(name_index, length, number_of_entries, entries)

        elif attr_type is attributes.AttributeInfoType.EXCEPTIONS:
            number_of_exceptions = self.read_u2()
            exception_index_table = [self.read_u2() for _ in range(number_of_exceptions)]
            return attributes.ExceptionsAttr(name_index, length, number_of_exceptions, exception_index_table)

        elif attr_type is attributes.AttributeInfoType.INNER_CLASSES:
            number_of_classes = self.read_u2()
            classes = [
                attributes.InnerClassInfo(
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    constants.NestedClassAccessFlag(self.read_u2()),
                )
                for _ in range(number_of_classes)
            ]
            return attributes.InnerClassesAttr(name_index, length, number_of_classes, classes)

        elif attr_type is attributes.AttributeInfoType.ENCLOSING_METHOD:
            return attributes.EnclosingMethodAttr(name_index, length, self.read_u2(), self.read_u2())

        elif attr_type is attributes.AttributeInfoType.SOURCE_DEBUG_EXTENSION:
            return attributes.SourceDebugExtensionAttr(name_index, length, self.read_bytes(length).decode("utf-8"))

        elif attr_type is attributes.AttributeInfoType.LINE_NUMBER_TABLE:
            line_number_table_length = self.read_u2()
            line_number_table = [
                attributes.LineNumberInfo(self.read_u2(), self.read_u2()) for _ in range(line_number_table_length)
            ]
            return attributes.LineNumberTableAttr(name_index, length, line_number_table_length, line_number_table)

        elif attr_type is attributes.AttributeInfoType.LOCAL_VARIABLE_TABLE:
            local_variable_table_length = self.read_u2()
            local_variable_table = [
                attributes.LocalVariableInfo(
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                )
                for _ in range(local_variable_table_length)
            ]
            return attributes.LocalVariableTableAttr(
                name_index, length, local_variable_table_length, local_variable_table
            )

        elif attr_type is attributes.AttributeInfoType.LOCAL_VARIABLE_TYPE_TABLE:
            local_variable_type_table_length = self.read_u2()
            local_variable_type_table = [
                attributes.LocalVariableTypeInfo(
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                )
                for _ in range(local_variable_type_table_length)
            ]
            return attributes.LocalVariableTypeTableAttr(
                name_index, length, local_variable_type_table_length, local_variable_type_table
            )

        elif attr_type is attributes.AttributeInfoType.RUNTIME_VISIBLE_ANNOTATIONS:
            num_annotations = self.read_u2()
            annotation_list = [self.read_annotation_info() for _ in range(num_annotations)]
            return attributes.RuntimeVisibleAnnotationsAttr(name_index, length, num_annotations, annotation_list)

        elif attr_type is attributes.AttributeInfoType.RUNTIME_INVISIBLE_ANNOTATIONS:
            num_annotations = self.read_u2()
            annotation_list = [self.read_annotation_info() for _ in range(num_annotations)]
            return attributes.RuntimeInvisibleAnnotationsAttr(name_index, length, num_annotations, annotation_list)

        elif attr_type is attributes.AttributeInfoType.RUNTIME_VISIBLE_PARAMETER_ANNOTATIONS:
            num_parameters = self.read_u1()
            parameter_annotations: list[attributes.ParameterAnnotationInfo] = []
            for _ in range(num_parameters):
                num_annotations = self.read_u2()
                annotation_list = [self.read_annotation_info() for _ in range(num_annotations)]
                parameter_annotations.append(attributes.ParameterAnnotationInfo(num_annotations, annotation_list))
            return attributes.RuntimeVisibleParameterAnnotationsAttr(
                name_index, length, num_parameters, parameter_annotations
            )

        elif attr_type is attributes.AttributeInfoType.RUNTIME_INVISIBLE_PARAMETER_ANNOTATIONS:
            num_parameters = self.read_u1()
            parameter_annotations_list: list[attributes.ParameterAnnotationInfo] = []
            for _ in range(num_parameters):
                num_annotations = self.read_u2()
                annotation_list = [self.read_annotation_info() for _ in range(num_annotations)]
                parameter_annotations_list.append(attributes.ParameterAnnotationInfo(num_annotations, annotation_list))
            return attributes.RuntimeInvisibleParameterAnnotationsAttr(
                name_index, length, num_parameters, parameter_annotations_list
            )

        elif attr_type is attributes.AttributeInfoType.RUNTIME_VISIBLE_TYPE_ANNOTATIONS:
            num_annotations = self.read_u2()
            type_annotation_list = [self.read_type_annotation_info() for _ in range(num_annotations)]
            return attributes.RuntimeVisibleTypeAnnotationsAttr(
                name_index, length, num_annotations, type_annotation_list
            )

        elif attr_type is attributes.AttributeInfoType.RUNTIME_INVISIBLE_TYPE_ANNOTATIONS:
            num_annotations = self.read_u2()
            type_annotation_list = [self.read_type_annotation_info() for _ in range(num_annotations)]
            return attributes.RuntimeInvisibleTypeAnnotationsAttr(
                name_index, length, num_annotations, type_annotation_list
            )

        elif attr_type is attributes.AttributeInfoType.ANNOTATION_DEFAULT:
            return attributes.AnnotationDefaultAttr(name_index, length, self.read_element_value_info())

        elif attr_type is attributes.AttributeInfoType.BOOTSTRAP_METHODS:
            num_bootstrap_methods = self.read_u2()
            bootstrap_methods: list[attributes.BootstrapMethodInfo] = []
            for _ in range(num_bootstrap_methods):
                bootstrap_method_ref = self.read_u2()
                num_bootstrap_arguments = self.read_u2()
                bootstrap_arguments = [self.read_u2() for __ in range(num_bootstrap_arguments)]
                bootstrap_methods.append(
                    attributes.BootstrapMethodInfo(
                        bootstrap_method_ref,
                        num_bootstrap_arguments,
                        bootstrap_arguments,
                    )
                )
            return attributes.BootstrapMethodsAttr(name_index, length, num_bootstrap_methods, bootstrap_methods)

        elif attr_type is attributes.AttributeInfoType.METHOD_PARAMETERS:
            parameters_count = self.read_u1()
            parameters = [
                attributes.MethodParameterInfo(self.read_u2(), constants.MethodParameterAccessFlag(self.read_u2()))
                for _ in range(parameters_count)
            ]
            return attributes.MethodParametersAttr(name_index, length, parameters_count, parameters)

        elif attr_type is attributes.AttributeInfoType.MODULE:
            module_name_index = self.read_u2()
            module_flags = constants.ModuleAccessFlag(self.read_u2())
            module_version_index = self.read_u2()

            requires_count = self.read_u2()
            requires = [
                attributes.RequiresInfo(
                    self.read_u2(),
                    constants.ModuleRequiresAccessFlag(self.read_u2()),
                    self.read_u2(),
                )
                for _ in range(requires_count)
            ]

            exports_count = self.read_u2()
            exports: list[attributes.ExportInfo] = []
            for _ in range(exports_count):
                exports_index = self.read_u2()
                exports_flags = constants.ModuleExportsAccessFlag(self.read_u2())
                exports_to_count = self.read_u2()
                exports_to_index = [self.read_u2() for __ in range(exports_to_count)]
                exports.append(attributes.ExportInfo(exports_index, exports_flags, exports_to_count, exports_to_index))

            opens_count = self.read_u2()
            opens: list[attributes.OpensInfo] = []
            for _ in range(opens_count):
                opens_index = self.read_u2()
                opens_flags = constants.ModuleOpensAccessFlag(self.read_u2())
                opens_to_count = self.read_u2()
                opens_to_index = [self.read_u2() for __ in range(opens_to_count)]
                opens.append(attributes.OpensInfo(opens_index, opens_flags, opens_to_count, opens_to_index))

            uses_count = self.read_u2()
            uses = [self.read_u2() for _ in range(uses_count)]

            provides_count = self.read_u2()
            provides: list[attributes.ProvidesInfo] = []
            for _ in range(provides_count):
                provides_index = self.read_u2()
                provides_with_count = self.read_u2()
                provides_with_index = [self.read_u2() for __ in range(provides_with_count)]
                provides.append(attributes.ProvidesInfo(provides_index, provides_with_count, provides_with_index))

            return attributes.ModuleAttr(
                name_index,
                length,
                module_name_index,
                module_flags,
                module_version_index,
                requires_count,
                requires,
                exports_count,
                exports,
                opens_count,
                opens,
                uses_count,
                uses,
                provides_count,
                provides,
            )

        elif attr_type is attributes.AttributeInfoType.MODULE_PACKAGES:
            package_count = self.read_u2()
            package_index = [self.read_u2() for _ in range(package_count)]
            return attributes.ModulePackagesAttr(name_index, length, package_count, package_index)

        elif attr_type is attributes.AttributeInfoType.NEST_MEMBERS:
            number_of_classes = self.read_u2()
            classes_list = [self.read_u2() for _ in range(number_of_classes)]
            return attributes.NestMembersAttr(name_index, length, number_of_classes, classes_list)

        elif attr_type is attributes.AttributeInfoType.RECORD:
            components_count = self.read_u2()
            components: list[attributes.RecordComponentInfo] = []
            for _ in range(components_count):
                comp_name_index = self.read_u2()
                descriptor_index = self.read_u2()
                attributes_count = self.read_u2()
                _attributes = [self.read_attribute() for _ in range(attributes_count)]
                components.append(
                    attributes.RecordComponentInfo(comp_name_index, descriptor_index, attributes_count, _attributes)
                )
            return attributes.RecordAttr(name_index, length, components_count, components)

        elif attr_type is attributes.AttributeInfoType.PERMITTED_SUBCLASSES:
            number_of_classes = self.read_u2()
            classes_list = [self.read_u2() for _ in range(number_of_classes)]
            return attributes.PermittedSubclassesAttr(name_index, length, number_of_classes, classes_list)

        return attributes.UnimplementedAttr(name_index, length, self.read_bytes(length), attr_type)

    def read_field(self) -> info.FieldInfo:
        """Read a single ``field_info`` structure (JVMS §4.5).

        Returns:
            The decoded field info including its attributes.
        """
        access_flags = constants.FieldAccessFlag(self.read_u2())
        name_index = self.read_u2()
        descriptor_index = self.read_u2()
        attributes_count = self.read_u2()
        attributes = [self.read_attribute() for _ in range(attributes_count)]
        return info.FieldInfo(access_flags, name_index, descriptor_index, attributes_count, attributes)

    def read_method(self) -> info.MethodInfo:
        """Read a single ``method_info`` structure (JVMS §4.6).

        Returns:
            The decoded method info including its attributes.
        """
        access_flags = constants.MethodAccessFlag(self.read_u2())
        name_index = self.read_u2()
        descriptor_index = self.read_u2()
        attributes_count = self.read_u2()
        attributes = [self.read_attribute() for _ in range(attributes_count)]
        return info.MethodInfo(access_flags, name_index, descriptor_index, attributes_count, attributes)

    def read_class(self) -> None:
        """Parse the complete ``ClassFile`` structure (JVMS §4.1).

        Validates the magic number and version, reads the constant pool,
        access flags, class hierarchy info, fields, methods, and attributes.
        The result is stored in :attr:`class_info`.

        Raises:
            MalformedClassException: If the magic number or version is invalid.
        """
        self.rewind()
        magic = self.read_u4()
        if magic != constants.MAGIC:
            raise MalformedClassException(f"Invalid magic number 0x{magic:x}, requires 0x{constants.MAGIC:x}")

        minor, major = self.read_u2(), self.read_u2()
        if major >= 56 and minor not in (0, 65535):
            raise MalformedClassException(f"Invalid version {major}/{minor}")

        cp_count = self.read_u2()

        self.constant_pool = [None] * cp_count
        index = 1
        while index < cp_count:
            cp_info, index_extra = self.read_constant_pool_index(index)
            self.constant_pool[index] = cp_info
            index += 1 + index_extra

        access_flags = constants.ClassAccessFlag(self.read_u2())
        this_class = self.read_u2()
        super_class = self.read_u2()

        interfaces_count = self.read_u2()
        interfaces = [self.read_u2() for _ in range(interfaces_count)]

        fields_count = self.read_u2()
        fields = [self.read_field() for _ in range(fields_count)]

        methods_count = self.read_u2()
        methods = [self.read_method() for _ in range(methods_count)]

        attributes_count = self.read_u2()
        attributes = [self.read_attribute() for _ in range(attributes_count)]

        self.class_info = info.ClassFile(
            magic,
            minor,
            major,
            cp_count,
            self.constant_pool,
            access_flags,
            this_class,
            super_class,
            interfaces_count,
            interfaces,
            fields_count,
            fields,
            methods_count,
            methods,
            attributes_count,
            attributes,
        )
