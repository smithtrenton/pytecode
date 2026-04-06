"""Definitions for JVM class file attributes (JVM spec §4.7)."""

import copy
from enum import Enum

from . import constants, instructions

__all__ = [
    "AnnotationDefaultAttr",
    "AnnotationInfo",
    "AppendFrameInfo",
    "ArrayValueInfo",
    "AttributeInfo",
    "AttributeInfoType",
    "BootstrapMethodInfo",
    "BootstrapMethodsAttr",
    "CatchTargetInfo",
    "ChopFrameInfo",
    "ClassInfoValueInfo",
    "CodeAttr",
    "ConstValueInfo",
    "ConstantValueAttr",
    "DeprecatedAttr",
    "DoubleVariableInfo",
    "ElementValueInfo",
    "ElementValuePairInfo",
    "EmptyTargetInfo",
    "EnclosingMethodAttr",
    "EnumConstantValueInfo",
    "ExceptionInfo",
    "ExceptionsAttr",
    "ExportInfo",
    "FloatVariableInfo",
    "FormalParameterTargetInfo",
    "FullFrameInfo",
    "InnerClassInfo",
    "InnerClassesAttr",
    "IntegerVariableInfo",
    "LineNumberInfo",
    "LineNumberTableAttr",
    "LocalVariableInfo",
    "LocalVariableTableAttr",
    "LocalVariableTypeInfo",
    "LocalVariableTypeTableAttr",
    "LocalvarTargetInfo",
    "LongVariableInfo",
    "MethodParameterInfo",
    "MethodParametersAttr",
    "ModuleAttr",
    "ModuleMainClassAttr",
    "ModulePackagesAttr",
    "NestHostAttr",
    "NestMembersAttr",
    "NullVariableInfo",
    "ObjectVariableInfo",
    "OffsetTargetInfo",
    "OpensInfo",
    "ParameterAnnotationInfo",
    "PathInfo",
    "PermittedSubclassesAttr",
    "ProvidesInfo",
    "RecordAttr",
    "RecordComponentInfo",
    "RequiresInfo",
    "RuntimeInvisibleAnnotationsAttr",
    "RuntimeInvisibleParameterAnnotationsAttr",
    "RuntimeInvisibleTypeAnnotationsAttr",
    "RuntimeTypeAnnotationsAttr",
    "RuntimeVisibleAnnotationsAttr",
    "RuntimeVisibleParameterAnnotationsAttr",
    "RuntimeVisibleTypeAnnotationsAttr",
    "SameFrameExtendedInfo",
    "SameFrameInfo",
    "SameLocals1StackItemFrameExtendedInfo",
    "SameLocals1StackItemFrameInfo",
    "SignatureAttr",
    "SourceDebugExtensionAttr",
    "SourceFileAttr",
    "StackMapFrameInfo",
    "StackMapTableAttr",
    "SupertypeTargetInfo",
    "SyntheticAttr",
    "TableInfo",
    "TargetInfo",
    "ThrowsTargetInfo",
    "TopVariableInfo",
    "TypeAnnotationInfo",
    "TypeArgumentTargetInfo",
    "TypeParameterBoundTargetInfo",
    "TypeParameterTargetInfo",
    "TypePathInfo",
    "UnimplementedAttr",
    "UninitializedThisVariableInfo",
    "UninitializedVariableInfo",
    "VerificationTypeInfo",
]

def _repr_fields(str class_name, tuple fields):
    return f"{class_name}(" + ", ".join(f"{name}={value!r}" for name, value in fields) + ")"


cdef class AttributeInfo:
    """Base class for all JVM class file attribute structures (§4.7)."""

    def __init__(self, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length):
        self.attribute_name_index = attribute_name_index
        self.attribute_length = attribute_length

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length)

    def _field_items(self):
        return (
            ("attribute_name_index", self.attribute_name_index),
            ("attribute_length", self.attribute_length),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class UnimplementedAttr(AttributeInfo):
    """Placeholder for attribute types not yet implemented by pytecode."""

    cdef public object info
    cdef public object attr_type

    def __init__(self, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length, object info, object attr_type):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.info = info
        self.attr_type = attr_type

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.info, self.attr_type)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (("info", self.info), ("attr_type", self.attr_type))


cdef class ConstantValueAttr(AttributeInfo):
    """Represents the ConstantValue attribute (§4.7.2)."""

    cdef public Py_ssize_t constantvalue_index

    def __init__(self, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length, Py_ssize_t constantvalue_index):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.constantvalue_index = constantvalue_index

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.constantvalue_index)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (("constantvalue_index", self.constantvalue_index),)


cdef class ExceptionInfo:
    """Entry in the Code attribute exception_table (§4.7.3)."""

    def __init__(self, Py_ssize_t start_pc, Py_ssize_t end_pc, Py_ssize_t handler_pc, Py_ssize_t catch_type):
        self.start_pc = start_pc
        self.end_pc = end_pc
        self.handler_pc = handler_pc
        self.catch_type = catch_type

    def _field_values(self):
        return (self.start_pc, self.end_pc, self.handler_pc, self.catch_type)

    def _field_items(self):
        return (
            ("start_pc", self.start_pc),
            ("end_pc", self.end_pc),
            ("handler_pc", self.handler_pc),
            ("catch_type", self.catch_type),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class CodeAttr(AttributeInfo):
    """Represents the Code attribute (§4.7.3)."""

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t max_stacks,
        Py_ssize_t max_locals,
        Py_ssize_t code_length,
        list code,
        Py_ssize_t exception_table_length,
        list exception_table,
        Py_ssize_t attributes_count,
        list attributes,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.max_stacks = max_stacks
        self.max_locals = max_locals
        self.code_length = code_length
        self.code = code
        self.exception_table_length = exception_table_length
        self.exception_table = exception_table
        self.attributes_count = attributes_count
        self.attributes = attributes

    def _field_values(self):
        return (
            self.attribute_name_index,
            self.attribute_length,
            self.max_stacks,
            self.max_locals,
            self.code_length,
            self.code,
            self.exception_table_length,
            self.exception_table,
            self.attributes_count,
            self.attributes,
        )

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("max_stacks", self.max_stacks),
            ("max_locals", self.max_locals),
            ("code_length", self.code_length),
            ("code", self.code),
            ("exception_table_length", self.exception_table_length),
            ("exception_table", self.exception_table),
            ("attributes_count", self.attributes_count),
            ("attributes", self.attributes),
        )


cdef class VerificationTypeInfo:
    """Base class for verification type info entries in StackMapTable frames (§4.7.4)."""

    def __init__(self, object tag):
        self.tag = tag

    def _field_values(self):
        return (self.tag,)

    def _field_items(self):
        return (("tag", self.tag),)

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class TopVariableInfo(VerificationTypeInfo):
    """Verification type indicating the top type (§4.7.4)."""

    pass


cdef class IntegerVariableInfo(VerificationTypeInfo):
    """Verification type indicating the integer type (§4.7.4)."""

    pass


cdef class FloatVariableInfo(VerificationTypeInfo):
    """Verification type indicating the float type (§4.7.4)."""

    pass


cdef class DoubleVariableInfo(VerificationTypeInfo):
    """Verification type indicating the double type (§4.7.4)."""

    pass


cdef class LongVariableInfo(VerificationTypeInfo):
    """Verification type indicating the long type (§4.7.4)."""

    pass


cdef class NullVariableInfo(VerificationTypeInfo):
    """Verification type indicating the null type (§4.7.4)."""

    pass


cdef class UninitializedThisVariableInfo(VerificationTypeInfo):
    """Verification type indicating the uninitializedThis type (§4.7.4)."""

    pass


cdef class ObjectVariableInfo(VerificationTypeInfo):
    """Verification type indicating an object type (§4.7.4)."""

    def __init__(self, object tag, Py_ssize_t cpool_index):
        VerificationTypeInfo.__init__(self, tag)
        self.cpool_index = cpool_index

    def _field_values(self):
        return (self.tag, self.cpool_index)

    def _field_items(self):
        return VerificationTypeInfo._field_items(self) + (("cpool_index", self.cpool_index),)


cdef class UninitializedVariableInfo(VerificationTypeInfo):
    """Verification type indicating an uninitialized type (§4.7.4)."""

    def __init__(self, object tag, Py_ssize_t offset):
        VerificationTypeInfo.__init__(self, tag)
        self.offset = offset

    def _field_values(self):
        return (self.tag, self.offset)

    def _field_items(self):
        return VerificationTypeInfo._field_items(self) + (("offset", self.offset),)


cdef class StackMapFrameInfo:
    """Base class for stack map frame entries in the StackMapTable attribute (§4.7.4)."""

    def __init__(self, Py_ssize_t frame_type):
        self.frame_type = frame_type

    def _field_values(self):
        return (self.frame_type,)

    def _field_items(self):
        return (("frame_type", self.frame_type),)

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class SameFrameInfo(StackMapFrameInfo):
    """Stack map frame with the same locals and empty stack (§4.7.4)."""

    pass


cdef class SameLocals1StackItemFrameInfo(StackMapFrameInfo):
    """Stack map frame with the same locals and one stack item (§4.7.4)."""

    def __init__(self, Py_ssize_t frame_type, object stack):
        StackMapFrameInfo.__init__(self, frame_type)
        self.stack = stack

    def _field_values(self):
        return (self.frame_type, self.stack)

    def _field_items(self):
        return StackMapFrameInfo._field_items(self) + (("stack", self.stack),)


cdef class SameLocals1StackItemFrameExtendedInfo(StackMapFrameInfo):
    """Extended same-locals-1-stack-item frame with explicit offset_delta (§4.7.4)."""

    def __init__(self, Py_ssize_t frame_type, Py_ssize_t offset_delta, object stack):
        StackMapFrameInfo.__init__(self, frame_type)
        self.offset_delta = offset_delta
        self.stack = stack

    def _field_values(self):
        return (self.frame_type, self.offset_delta, self.stack)

    def _field_items(self):
        return StackMapFrameInfo._field_items(self) + (
            ("offset_delta", self.offset_delta),
            ("stack", self.stack),
        )


cdef class ChopFrameInfo(StackMapFrameInfo):
    """Stack map frame indicating removal of locals (§4.7.4)."""

    def __init__(self, Py_ssize_t frame_type, Py_ssize_t offset_delta):
        StackMapFrameInfo.__init__(self, frame_type)
        self.offset_delta = offset_delta

    def _field_values(self):
        return (self.frame_type, self.offset_delta)

    def _field_items(self):
        return StackMapFrameInfo._field_items(self) + (("offset_delta", self.offset_delta),)


cdef class SameFrameExtendedInfo(StackMapFrameInfo):
    """Extended same frame with explicit offset_delta (§4.7.4)."""

    def __init__(self, Py_ssize_t frame_type, Py_ssize_t offset_delta):
        StackMapFrameInfo.__init__(self, frame_type)
        self.offset_delta = offset_delta

    def _field_values(self):
        return (self.frame_type, self.offset_delta)

    def _field_items(self):
        return StackMapFrameInfo._field_items(self) + (("offset_delta", self.offset_delta),)


cdef class AppendFrameInfo(StackMapFrameInfo):
    """Stack map frame indicating additional locals (§4.7.4)."""

    def __init__(self, Py_ssize_t frame_type, Py_ssize_t offset_delta, list locals):
        StackMapFrameInfo.__init__(self, frame_type)
        self.offset_delta = offset_delta
        self.locals = locals

    def _field_values(self):
        return (self.frame_type, self.offset_delta, self.locals)

    def _field_items(self):
        return StackMapFrameInfo._field_items(self) + (
            ("offset_delta", self.offset_delta),
            ("locals", self.locals),
        )


cdef class FullFrameInfo(StackMapFrameInfo):
    """Full stack map frame with explicit locals and stack (§4.7.4)."""

    def __init__(
        self,
        Py_ssize_t frame_type,
        Py_ssize_t offset_delta,
        Py_ssize_t number_of_locals,
        list locals,
        Py_ssize_t number_of_stack_items,
        list stack,
    ):
        StackMapFrameInfo.__init__(self, frame_type)
        self.offset_delta = offset_delta
        self.number_of_locals = number_of_locals
        self.locals = locals
        self.number_of_stack_items = number_of_stack_items
        self.stack = stack

    def _field_values(self):
        return (
            self.frame_type,
            self.offset_delta,
            self.number_of_locals,
            self.locals,
            self.number_of_stack_items,
            self.stack,
        )

    def _field_items(self):
        return StackMapFrameInfo._field_items(self) + (
            ("offset_delta", self.offset_delta),
            ("number_of_locals", self.number_of_locals),
            ("locals", self.locals),
            ("number_of_stack_items", self.number_of_stack_items),
            ("stack", self.stack),
        )


cdef class StackMapTableAttr(AttributeInfo):
    """Represents the StackMapTable attribute (§4.7.4)."""

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t number_of_entries,
        list entries,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.number_of_entries = number_of_entries
        self.entries = entries

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.number_of_entries, self.entries)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("number_of_entries", self.number_of_entries),
            ("entries", self.entries),
        )


cdef class ExceptionsAttr(AttributeInfo):
    """Represents the Exceptions attribute (§4.7.5)."""

    cdef public Py_ssize_t number_of_exceptions
    cdef public list exception_index_table

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t number_of_exceptions,
        list exception_index_table,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.number_of_exceptions = number_of_exceptions
        self.exception_index_table = exception_index_table

    def _field_values(self):
        return (
            self.attribute_name_index,
            self.attribute_length,
            self.number_of_exceptions,
            self.exception_index_table,
        )

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("number_of_exceptions", self.number_of_exceptions),
            ("exception_index_table", self.exception_index_table),
        )


cdef class InnerClassInfo:
    """Entry in the InnerClasses attribute classes table (§4.7.6)."""

    def __init__(
        self,
        Py_ssize_t inner_class_info_index,
        Py_ssize_t outer_class_info_index,
        Py_ssize_t inner_name_index,
        object inner_class_access_flags,
    ):
        self.inner_class_info_index = inner_class_info_index
        self.outer_class_info_index = outer_class_info_index
        self.inner_name_index = inner_name_index
        self.inner_class_access_flags = inner_class_access_flags

    def _field_values(self):
        return (
            self.inner_class_info_index,
            self.outer_class_info_index,
            self.inner_name_index,
            self.inner_class_access_flags,
        )

    def _field_items(self):
        return (
            ("inner_class_info_index", self.inner_class_info_index),
            ("outer_class_info_index", self.outer_class_info_index),
            ("inner_name_index", self.inner_name_index),
            ("inner_class_access_flags", self.inner_class_access_flags),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class InnerClassesAttr(AttributeInfo):
    """Represents the InnerClasses attribute (§4.7.6)."""

    cdef public Py_ssize_t number_of_classes
    cdef public list classes

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t number_of_classes,
        list classes,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.number_of_classes = number_of_classes
        self.classes = classes

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.number_of_classes, self.classes)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("number_of_classes", self.number_of_classes),
            ("classes", self.classes),
        )


cdef class EnclosingMethodAttr(AttributeInfo):
    """Represents the EnclosingMethod attribute (§4.7.7)."""

    cdef public Py_ssize_t class_index
    cdef public Py_ssize_t method_index

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t class_index,
        Py_ssize_t method_index,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.class_index = class_index
        self.method_index = method_index

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.class_index, self.method_index)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("class_index", self.class_index),
            ("method_index", self.method_index),
        )


cdef class SyntheticAttr(AttributeInfo):
    """Represents the Synthetic attribute (§4.7.8)."""

    pass


cdef class SignatureAttr(AttributeInfo):
    """Represents the Signature attribute (§4.7.9)."""

    cdef public Py_ssize_t signature_index

    def __init__(self, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length, Py_ssize_t signature_index):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.signature_index = signature_index

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.signature_index)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (("signature_index", self.signature_index),)


cdef class SourceFileAttr(AttributeInfo):
    """Represents the SourceFile attribute (§4.7.10)."""

    cdef public Py_ssize_t sourcefile_index

    def __init__(self, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length, Py_ssize_t sourcefile_index):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.sourcefile_index = sourcefile_index

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.sourcefile_index)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (("sourcefile_index", self.sourcefile_index),)


cdef class SourceDebugExtensionAttr(AttributeInfo):
    """Represents the SourceDebugExtension attribute (§4.7.11)."""

    cdef public object debug_extension

    def __init__(self, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length, object debug_extension):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.debug_extension = debug_extension

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.debug_extension)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (("debug_extension", self.debug_extension),)


cdef class LineNumberInfo:
    """Entry in the LineNumberTable attribute (§4.7.12)."""

    def __init__(self, Py_ssize_t start_pc, Py_ssize_t line_number):
        self.start_pc = start_pc
        self.line_number = line_number

    def _field_values(self):
        return (self.start_pc, self.line_number)

    def _field_items(self):
        return (("start_pc", self.start_pc), ("line_number", self.line_number))

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class LineNumberTableAttr(AttributeInfo):
    """Represents the LineNumberTable attribute (§4.7.12)."""

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t line_number_table_length,
        list line_number_table,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.line_number_table_length = line_number_table_length
        self.line_number_table = line_number_table

    def _field_values(self):
        return (
            self.attribute_name_index,
            self.attribute_length,
            self.line_number_table_length,
            self.line_number_table,
        )

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("line_number_table_length", self.line_number_table_length),
            ("line_number_table", self.line_number_table),
        )


cdef class LocalVariableInfo:
    """Entry in the LocalVariableTable attribute (§4.7.13)."""

    def __init__(
        self,
        Py_ssize_t start_pc,
        Py_ssize_t length,
        Py_ssize_t name_index,
        Py_ssize_t descriptor_index,
        Py_ssize_t index,
    ):
        self.start_pc = start_pc
        self.length = length
        self.name_index = name_index
        self.descriptor_index = descriptor_index
        self.index = index

    def _field_values(self):
        return (self.start_pc, self.length, self.name_index, self.descriptor_index, self.index)

    def _field_items(self):
        return (
            ("start_pc", self.start_pc),
            ("length", self.length),
            ("name_index", self.name_index),
            ("descriptor_index", self.descriptor_index),
            ("index", self.index),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class LocalVariableTableAttr(AttributeInfo):
    """Represents the LocalVariableTable attribute (§4.7.13)."""

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t local_variable_table_length,
        list local_variable_table,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.local_variable_table_length = local_variable_table_length
        self.local_variable_table = local_variable_table

    def _field_values(self):
        return (
            self.attribute_name_index,
            self.attribute_length,
            self.local_variable_table_length,
            self.local_variable_table,
        )

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("local_variable_table_length", self.local_variable_table_length),
            ("local_variable_table", self.local_variable_table),
        )


cdef class LocalVariableTypeInfo:
    """Entry in the LocalVariableTypeTable attribute (§4.7.14)."""

    def __init__(
        self,
        Py_ssize_t start_pc,
        Py_ssize_t length,
        Py_ssize_t name_index,
        Py_ssize_t signature_index,
        Py_ssize_t index,
    ):
        self.start_pc = start_pc
        self.length = length
        self.name_index = name_index
        self.signature_index = signature_index
        self.index = index

    def _field_values(self):
        return (self.start_pc, self.length, self.name_index, self.signature_index, self.index)

    def _field_items(self):
        return (
            ("start_pc", self.start_pc),
            ("length", self.length),
            ("name_index", self.name_index),
            ("signature_index", self.signature_index),
            ("index", self.index),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class LocalVariableTypeTableAttr(AttributeInfo):
    """Represents the LocalVariableTypeTable attribute (§4.7.14)."""

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t local_variable_type_table_length,
        list local_variable_type_table,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.local_variable_type_table_length = local_variable_type_table_length
        self.local_variable_type_table = local_variable_type_table

    def _field_values(self):
        return (
            self.attribute_name_index,
            self.attribute_length,
            self.local_variable_type_table_length,
            self.local_variable_type_table,
        )

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("local_variable_type_table_length", self.local_variable_type_table_length),
            ("local_variable_type_table", self.local_variable_type_table),
        )


cdef class DeprecatedAttr(AttributeInfo):
    """Represents the Deprecated attribute (§4.7.15)."""

    pass


cdef class _PayloadInfoBase:
    def _field_values(self):
        return ()

    def _field_items(self):
        return ()

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class ConstValueInfo(_PayloadInfoBase):
    """Constant value in an element_value structure (§4.7.16.1)."""

    def __init__(self, Py_ssize_t const_value_index):
        self.const_value_index = const_value_index

    def _field_values(self):
        return (self.const_value_index,)

    def _field_items(self):
        return (("const_value_index", self.const_value_index),)


cdef class EnumConstantValueInfo(_PayloadInfoBase):
    """Enum constant value in an element_value structure (§4.7.16.1)."""

    def __init__(self, Py_ssize_t type_name_index, Py_ssize_t const_name_index):
        self.type_name_index = type_name_index
        self.const_name_index = const_name_index

    def _field_values(self):
        return (self.type_name_index, self.const_name_index)

    def _field_items(self):
        return (
            ("type_name_index", self.type_name_index),
            ("const_name_index", self.const_name_index),
        )


cdef class ClassInfoValueInfo(_PayloadInfoBase):
    """Class literal value in an element_value structure (§4.7.16.1)."""

    def __init__(self, Py_ssize_t class_info_index):
        self.class_info_index = class_info_index

    def _field_values(self):
        return (self.class_info_index,)

    def _field_items(self):
        return (("class_info_index", self.class_info_index),)


cdef class ArrayValueInfo(_PayloadInfoBase):
    """Array value in an element_value structure (§4.7.16.1)."""

    def __init__(self, Py_ssize_t num_values, list values):
        self.num_values = num_values
        self.values = values

    def _field_values(self):
        return (self.num_values, self.values)

    def _field_items(self):
        return (("num_values", self.num_values), ("values", self.values))


cdef class ElementValueInfo(_PayloadInfoBase):
    """Represents an element_value structure (§4.7.16.1)."""

    def __init__(self, object tag, object value):
        self.tag = tag
        self.value = value

    def _field_values(self):
        return (self.tag, self.value)

    def _field_items(self):
        return (("tag", self.tag), ("value", self.value))


cdef class ElementValuePairInfo(_PayloadInfoBase):
    """Represents an element-value pair in an annotation (§4.7.16)."""

    def __init__(self, Py_ssize_t element_name_index, object element_value):
        self.element_name_index = element_name_index
        self.element_value = element_value

    def _field_values(self):
        return (self.element_name_index, self.element_value)

    def _field_items(self):
        return (
            ("element_name_index", self.element_name_index),
            ("element_value", self.element_value),
        )


cdef class AnnotationInfo(_PayloadInfoBase):
    """Represents an annotation structure (§4.7.16)."""

    def __init__(self, Py_ssize_t type_index, Py_ssize_t num_element_value_pairs, list element_value_pairs):
        self.type_index = type_index
        self.num_element_value_pairs = num_element_value_pairs
        self.element_value_pairs = element_value_pairs

    def _field_values(self):
        return (self.type_index, self.num_element_value_pairs, self.element_value_pairs)

    def _field_items(self):
        return (
            ("type_index", self.type_index),
            ("num_element_value_pairs", self.num_element_value_pairs),
            ("element_value_pairs", self.element_value_pairs),
        )


cdef class RuntimeVisibleAnnotationsAttr(AttributeInfo):
    """Represents the RuntimeVisibleAnnotations attribute (§4.7.16)."""

    cdef public Py_ssize_t num_annotations
    cdef public list annotations

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t num_annotations,
        list annotations,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.num_annotations = num_annotations
        self.annotations = annotations

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.num_annotations, self.annotations)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("num_annotations", self.num_annotations),
            ("annotations", self.annotations),
        )


cdef class RuntimeInvisibleAnnotationsAttr(AttributeInfo):
    """Represents the RuntimeInvisibleAnnotations attribute (§4.7.17)."""

    cdef public Py_ssize_t num_annotations
    cdef public list annotations

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t num_annotations,
        list annotations,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.num_annotations = num_annotations
        self.annotations = annotations

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.num_annotations, self.annotations)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("num_annotations", self.num_annotations),
            ("annotations", self.annotations),
        )


cdef class ParameterAnnotationInfo(_PayloadInfoBase):
    """Annotations for a single parameter (§4.7.18)."""

    def __init__(self, Py_ssize_t num_annotations, list annotations):
        self.num_annotations = num_annotations
        self.annotations = annotations

    def _field_values(self):
        return (self.num_annotations, self.annotations)

    def _field_items(self):
        return (("num_annotations", self.num_annotations), ("annotations", self.annotations))


cdef class RuntimeVisibleParameterAnnotationsAttr(AttributeInfo):
    """Represents the RuntimeVisibleParameterAnnotations attribute (§4.7.18)."""

    cdef public Py_ssize_t num_parameters
    cdef public list parameter_annotations

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t num_parameters,
        list parameter_annotations,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.num_parameters = num_parameters
        self.parameter_annotations = parameter_annotations

    def _field_values(self):
        return (
            self.attribute_name_index,
            self.attribute_length,
            self.num_parameters,
            self.parameter_annotations,
        )

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("num_parameters", self.num_parameters),
            ("parameter_annotations", self.parameter_annotations),
        )


cdef class RuntimeInvisibleParameterAnnotationsAttr(AttributeInfo):
    """Represents the RuntimeInvisibleParameterAnnotations attribute (§4.7.19)."""

    cdef public Py_ssize_t num_parameters
    cdef public list parameter_annotations

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t num_parameters,
        list parameter_annotations,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.num_parameters = num_parameters
        self.parameter_annotations = parameter_annotations

    def _field_values(self):
        return (
            self.attribute_name_index,
            self.attribute_length,
            self.num_parameters,
            self.parameter_annotations,
        )

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("num_parameters", self.num_parameters),
            ("parameter_annotations", self.parameter_annotations),
        )


cdef class TargetInfo(_PayloadInfoBase):
    """Base class for type annotation target_info union variants (§4.7.20.1)."""

    pass


cdef class TypeParameterTargetInfo(TargetInfo):
    """Target info for type parameter declarations (§4.7.20.1)."""

    def __init__(self, Py_ssize_t type_parameter_index):
        self.type_parameter_index = type_parameter_index

    def _field_values(self):
        return (self.type_parameter_index,)

    def _field_items(self):
        return (("type_parameter_index", self.type_parameter_index),)


cdef class SupertypeTargetInfo(TargetInfo):
    """Target info for extends/implements clauses (§4.7.20.1)."""

    def __init__(self, Py_ssize_t supertype_index):
        self.supertype_index = supertype_index

    def _field_values(self):
        return (self.supertype_index,)

    def _field_items(self):
        return (("supertype_index", self.supertype_index),)


cdef class TypeParameterBoundTargetInfo(TargetInfo):
    """Target info for type parameter bounds (§4.7.20.1)."""

    def __init__(self, Py_ssize_t type_parameter_index, Py_ssize_t bound_index):
        self.type_parameter_index = type_parameter_index
        self.bound_index = bound_index

    def _field_values(self):
        return (self.type_parameter_index, self.bound_index)

    def _field_items(self):
        return (
            ("type_parameter_index", self.type_parameter_index),
            ("bound_index", self.bound_index),
        )


cdef class EmptyTargetInfo(TargetInfo):
    """Target info for return types, receiver types, or field types (§4.7.20.1)."""

    pass


cdef class FormalParameterTargetInfo(TargetInfo):
    """Target info for formal parameter declarations (§4.7.20.1)."""

    def __init__(self, Py_ssize_t formal_parameter_index):
        self.formal_parameter_index = formal_parameter_index

    def _field_values(self):
        return (self.formal_parameter_index,)

    def _field_items(self):
        return (("formal_parameter_index", self.formal_parameter_index),)


cdef class ThrowsTargetInfo(TargetInfo):
    """Target info for throws clause types (§4.7.20.1)."""

    def __init__(self, Py_ssize_t throws_type_index):
        self.throws_type_index = throws_type_index

    def _field_values(self):
        return (self.throws_type_index,)

    def _field_items(self):
        return (("throws_type_index", self.throws_type_index),)


cdef class TableInfo(_PayloadInfoBase):
    """Entry in the localvar_target table (§4.7.20.1)."""

    def __init__(self, Py_ssize_t start_pc, Py_ssize_t length, Py_ssize_t index):
        self.start_pc = start_pc
        self.length = length
        self.index = index

    def _field_values(self):
        return (self.start_pc, self.length, self.index)

    def _field_items(self):
        return (
            ("start_pc", self.start_pc),
            ("length", self.length),
            ("index", self.index),
        )


cdef class LocalvarTargetInfo(TargetInfo):
    """Target info for local variable type annotations (§4.7.20.1)."""

    def __init__(self, Py_ssize_t table_length, list table):
        self.table_length = table_length
        self.table = table

    def _field_values(self):
        return (self.table_length, self.table)

    def _field_items(self):
        return (("table_length", self.table_length), ("table", self.table))


cdef class CatchTargetInfo(TargetInfo):
    """Target info for exception parameter types (§4.7.20.1)."""

    def __init__(self, Py_ssize_t exception_table_index):
        self.exception_table_index = exception_table_index

    def _field_values(self):
        return (self.exception_table_index,)

    def _field_items(self):
        return (("exception_table_index", self.exception_table_index),)


cdef class OffsetTargetInfo(TargetInfo):
    """Target info for instanceof, new, or method reference expressions (§4.7.20.1)."""

    def __init__(self, Py_ssize_t offset):
        self.offset = offset

    def _field_values(self):
        return (self.offset,)

    def _field_items(self):
        return (("offset", self.offset),)


cdef class TypeArgumentTargetInfo(TargetInfo):
    """Target info for cast or type argument expressions (§4.7.20.1)."""

    def __init__(self, Py_ssize_t offset, Py_ssize_t type_argument_index):
        self.offset = offset
        self.type_argument_index = type_argument_index

    def _field_values(self):
        return (self.offset, self.type_argument_index)

    def _field_items(self):
        return (
            ("offset", self.offset),
            ("type_argument_index", self.type_argument_index),
        )


cdef class PathInfo(_PayloadInfoBase):
    """Single entry in a type_path structure (§4.7.20.2)."""

    def __init__(self, Py_ssize_t type_path_kind, Py_ssize_t type_argument_index):
        self.type_path_kind = type_path_kind
        self.type_argument_index = type_argument_index

    def _field_values(self):
        return (self.type_path_kind, self.type_argument_index)

    def _field_items(self):
        return (
            ("type_path_kind", self.type_path_kind),
            ("type_argument_index", self.type_argument_index),
        )


cdef class TypePathInfo(_PayloadInfoBase):
    """Represents a type_path structure (§4.7.20.2)."""

    def __init__(self, Py_ssize_t path_length, list path):
        self.path_length = path_length
        self.path = path

    def _field_values(self):
        return (self.path_length, self.path)

    def _field_items(self):
        return (("path_length", self.path_length), ("path", self.path))


cdef class TypeAnnotationInfo(_PayloadInfoBase):
    """Represents a type_annotation structure (§4.7.20)."""

    def __init__(
        self,
        Py_ssize_t target_type,
        object target_info,
        object target_path,
        Py_ssize_t type_index,
        Py_ssize_t num_element_value_pairs,
        list element_value_pairs,
    ):
        self.target_type = target_type
        self.target_info = target_info
        self.target_path = target_path
        self.type_index = type_index
        self.num_element_value_pairs = num_element_value_pairs
        self.element_value_pairs = element_value_pairs

    def _field_values(self):
        return (
            self.target_type,
            self.target_info,
            self.target_path,
            self.type_index,
            self.num_element_value_pairs,
            self.element_value_pairs,
        )

    def _field_items(self):
        return (
            ("target_type", self.target_type),
            ("target_info", self.target_info),
            ("target_path", self.target_path),
            ("type_index", self.type_index),
            ("num_element_value_pairs", self.num_element_value_pairs),
            ("element_value_pairs", self.element_value_pairs),
        )


cdef class RuntimeTypeAnnotationsAttr(AttributeInfo):
    """Base class for RuntimeVisibleTypeAnnotations and RuntimeInvisibleTypeAnnotations."""

    cdef public Py_ssize_t num_annotations
    cdef public list annotations

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t num_annotations,
        list annotations,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.num_annotations = num_annotations
        self.annotations = annotations

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.num_annotations, self.annotations)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("num_annotations", self.num_annotations),
            ("annotations", self.annotations),
        )


cdef class RuntimeVisibleTypeAnnotationsAttr(RuntimeTypeAnnotationsAttr):
    """Represents the RuntimeVisibleTypeAnnotations attribute (§4.7.20)."""

    pass


cdef class RuntimeInvisibleTypeAnnotationsAttr(RuntimeTypeAnnotationsAttr):
    """Represents the RuntimeInvisibleTypeAnnotations attribute (§4.7.21)."""

    pass


cdef class AnnotationDefaultAttr(AttributeInfo):
    """Represents the AnnotationDefault attribute (§4.7.22)."""

    cdef public object default_value

    def __init__(self, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length, object default_value):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.default_value = default_value

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.default_value)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (("default_value", self.default_value),)


cdef class BootstrapMethodInfo:
    """Entry in the BootstrapMethods attribute (§4.7.23)."""

    def __init__(self, Py_ssize_t bootstrap_method_ref, Py_ssize_t num_boostrap_arguments, list boostrap_arguments):
        self.bootstrap_method_ref = bootstrap_method_ref
        self.num_boostrap_arguments = num_boostrap_arguments
        self.boostrap_arguments = boostrap_arguments

    def _field_values(self):
        return (self.bootstrap_method_ref, self.num_boostrap_arguments, self.boostrap_arguments)

    def _field_items(self):
        return (
            ("bootstrap_method_ref", self.bootstrap_method_ref),
            ("num_boostrap_arguments", self.num_boostrap_arguments),
            ("boostrap_arguments", self.boostrap_arguments),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class BootstrapMethodsAttr(AttributeInfo):
    """Represents the BootstrapMethods attribute (§4.7.23)."""

    cdef public Py_ssize_t num_bootstrap_methods
    cdef public list bootstrap_methods

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t num_bootstrap_methods,
        list bootstrap_methods,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.num_bootstrap_methods = num_bootstrap_methods
        self.bootstrap_methods = bootstrap_methods

    def _field_values(self):
        return (
            self.attribute_name_index,
            self.attribute_length,
            self.num_bootstrap_methods,
            self.bootstrap_methods,
        )

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("num_bootstrap_methods", self.num_bootstrap_methods),
            ("bootstrap_methods", self.bootstrap_methods),
        )


cdef class MethodParameterInfo:
    """Entry in the MethodParameters attribute (§4.7.24)."""

    def __init__(self, Py_ssize_t name_index, object access_flags):
        self.name_index = name_index
        self.access_flags = access_flags

    def _field_values(self):
        return (self.name_index, self.access_flags)

    def _field_items(self):
        return (("name_index", self.name_index), ("access_flags", self.access_flags))

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class MethodParametersAttr(AttributeInfo):
    """Represents the MethodParameters attribute (§4.7.24)."""

    cdef public Py_ssize_t parameters_count
    cdef public list parameters

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t parameters_count,
        list parameters,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.parameters_count = parameters_count
        self.parameters = parameters

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.parameters_count, self.parameters)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("parameters_count", self.parameters_count),
            ("parameters", self.parameters),
        )


cdef class RequiresInfo:
    """Entry in the Module attribute requires table (§4.7.25)."""

    def __init__(self, Py_ssize_t requires_index, object requires_flag, Py_ssize_t requires_version_index):
        self.requires_index = requires_index
        self.requires_flag = requires_flag
        self.requires_version_index = requires_version_index

    def _field_values(self):
        return (self.requires_index, self.requires_flag, self.requires_version_index)

    def _field_items(self):
        return (
            ("requires_index", self.requires_index),
            ("requires_flag", self.requires_flag),
            ("requires_version_index", self.requires_version_index),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class ExportInfo:
    """Entry in the Module attribute exports table (§4.7.25)."""

    def __init__(
        self,
        Py_ssize_t exports_index,
        object exports_flags,
        Py_ssize_t exports_to_count,
        list exports_to_index,
    ):
        self.exports_index = exports_index
        self.exports_flags = exports_flags
        self.exports_to_count = exports_to_count
        self.exports_to_index = exports_to_index

    def _field_values(self):
        return (self.exports_index, self.exports_flags, self.exports_to_count, self.exports_to_index)

    def _field_items(self):
        return (
            ("exports_index", self.exports_index),
            ("exports_flags", self.exports_flags),
            ("exports_to_count", self.exports_to_count),
            ("exports_to_index", self.exports_to_index),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class OpensInfo:
    """Entry in the Module attribute opens table (§4.7.25)."""

    def __init__(
        self,
        Py_ssize_t opens_index,
        object opens_flags,
        Py_ssize_t opens_to_count,
        list opens_to_index,
    ):
        self.opens_index = opens_index
        self.opens_flags = opens_flags
        self.opens_to_count = opens_to_count
        self.opens_to_index = opens_to_index

    def _field_values(self):
        return (self.opens_index, self.opens_flags, self.opens_to_count, self.opens_to_index)

    def _field_items(self):
        return (
            ("opens_index", self.opens_index),
            ("opens_flags", self.opens_flags),
            ("opens_to_count", self.opens_to_count),
            ("opens_to_index", self.opens_to_index),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class ProvidesInfo:
    """Entry in the Module attribute provides table (§4.7.25)."""

    def __init__(self, Py_ssize_t provides_index, Py_ssize_t provides_with_count, list provides_with_index):
        self.provides_index = provides_index
        self.provides_with_count = provides_with_count
        self.provides_with_index = provides_with_index

    def _field_values(self):
        return (self.provides_index, self.provides_with_count, self.provides_with_index)

    def _field_items(self):
        return (
            ("provides_index", self.provides_index),
            ("provides_with_count", self.provides_with_count),
            ("provides_with_index", self.provides_with_index),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class ModuleAttr(AttributeInfo):
    """Represents the Module attribute (§4.7.25)."""

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t module_name_index,
        object module_flags,
        Py_ssize_t module_version_index,
        Py_ssize_t requires_count,
        list requires,
        Py_ssize_t exports_count,
        list exports,
        Py_ssize_t opens_count,
        list opens,
        Py_ssize_t uses_count,
        list uses_index,
        Py_ssize_t provides_count,
        list provides,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.module_name_index = module_name_index
        self.module_flags = module_flags
        self.module_version_index = module_version_index
        self.requires_count = requires_count
        self.requires = requires
        self.exports_count = exports_count
        self.exports = exports
        self.opens_count = opens_count
        self.opens = opens
        self.uses_count = uses_count
        self.uses_index = uses_index
        self.provides_count = provides_count
        self.provides = provides

    def _field_values(self):
        return (
            self.attribute_name_index,
            self.attribute_length,
            self.module_name_index,
            self.module_flags,
            self.module_version_index,
            self.requires_count,
            self.requires,
            self.exports_count,
            self.exports,
            self.opens_count,
            self.opens,
            self.uses_count,
            self.uses_index,
            self.provides_count,
            self.provides,
        )

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("module_name_index", self.module_name_index),
            ("module_flags", self.module_flags),
            ("module_version_index", self.module_version_index),
            ("requires_count", self.requires_count),
            ("requires", self.requires),
            ("exports_count", self.exports_count),
            ("exports", self.exports),
            ("opens_count", self.opens_count),
            ("opens", self.opens),
            ("uses_count", self.uses_count),
            ("uses_index", self.uses_index),
            ("provides_count", self.provides_count),
            ("provides", self.provides),
        )


cdef class ModulePackagesAttr(AttributeInfo):
    """Represents the ModulePackages attribute (§4.7.26)."""

    cdef public Py_ssize_t package_count
    cdef public list package_index

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t package_count,
        list package_index,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.package_count = package_count
        self.package_index = package_index

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.package_count, self.package_index)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("package_count", self.package_count),
            ("package_index", self.package_index),
        )


cdef class ModuleMainClassAttr(AttributeInfo):
    """Represents the ModuleMainClass attribute (§4.7.27)."""

    cdef public Py_ssize_t main_class_index

    def __init__(self, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length, Py_ssize_t main_class_index):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.main_class_index = main_class_index

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.main_class_index)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (("main_class_index", self.main_class_index),)


cdef class NestHostAttr(AttributeInfo):
    """Represents the NestHost attribute (§4.7.28)."""

    cdef public Py_ssize_t host_class_index

    def __init__(self, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length, Py_ssize_t host_class_index):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.host_class_index = host_class_index

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.host_class_index)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (("host_class_index", self.host_class_index),)


cdef class NestMembersAttr(AttributeInfo):
    """Represents the NestMembers attribute (§4.7.29)."""

    cdef public Py_ssize_t number_of_classes
    cdef public list classes

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t number_of_classes,
        list classes,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.number_of_classes = number_of_classes
        self.classes = classes

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.number_of_classes, self.classes)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("number_of_classes", self.number_of_classes),
            ("classes", self.classes),
        )


cdef class RecordComponentInfo:
    """Describes a single record component in the Record attribute (§4.7.30)."""

    def __init__(
        self,
        Py_ssize_t name_index,
        Py_ssize_t descriptor_index,
        Py_ssize_t attributes_count,
        list attributes,
    ):
        self.name_index = name_index
        self.descriptor_index = descriptor_index
        self.attributes_count = attributes_count
        self.attributes = attributes

    def _field_values(self):
        return (self.name_index, self.descriptor_index, self.attributes_count, self.attributes)

    def _field_items(self):
        return (
            ("name_index", self.name_index),
            ("descriptor_index", self.descriptor_index),
            ("attributes_count", self.attributes_count),
            ("attributes", self.attributes),
        )

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class RecordAttr(AttributeInfo):
    """Represents the Record attribute (§4.7.30)."""

    cdef public Py_ssize_t components_count
    cdef public list components

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t components_count,
        list components,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.components_count = components_count
        self.components = components

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.components_count, self.components)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("components_count", self.components_count),
            ("components", self.components),
        )


cdef class PermittedSubclassesAttr(AttributeInfo):
    """Represents the PermittedSubclasses attribute (§4.7.31)."""

    cdef public Py_ssize_t number_of_classes
    cdef public list classes

    def __init__(
        self,
        Py_ssize_t attribute_name_index,
        Py_ssize_t attribute_length,
        Py_ssize_t number_of_classes,
        list classes,
    ):
        AttributeInfo.__init__(self, attribute_name_index, attribute_length)
        self.number_of_classes = number_of_classes
        self.classes = classes

    def _field_values(self):
        return (self.attribute_name_index, self.attribute_length, self.number_of_classes, self.classes)

    def _field_items(self):
        return AttributeInfo._field_items(self) + (
            ("number_of_classes", self.number_of_classes),
            ("classes", self.classes),
        )


class AttributeInfoType(Enum):
    """Enum mapping JVM attribute names to their attribute classes.

    Attributes:
        attr_class: The type that represents this attribute.
    """

    CONSTANT_VALUE = "ConstantValue", ConstantValueAttr
    CODE = "Code", CodeAttr
    STACK_MAP_TABLE = "StackMapTable", StackMapTableAttr
    EXCEPTIONS = "Exceptions", ExceptionsAttr
    INNER_CLASSES = "InnerClasses", InnerClassesAttr
    ENCLOSING_METHOD = "EnclosingMethod", EnclosingMethodAttr
    SYNTHETIC = "Synthetic", SyntheticAttr
    SIGNATURE = "Signature", SignatureAttr
    SOURCE_FILE = "SourceFile", SourceFileAttr
    SOURCE_DEBUG_EXTENSION = "SourceDebugExtension", SourceDebugExtensionAttr
    LINE_NUMBER_TABLE = "LineNumberTable", LineNumberTableAttr
    LOCAL_VARIABLE_TABLE = "LocalVariableTable", LocalVariableTableAttr
    LOCAL_VARIABLE_TYPE_TABLE = "LocalVariableTypeTable", LocalVariableTypeTableAttr
    DEPRECATED = "Deprecated", DeprecatedAttr
    RUNTIME_VISIBLE_ANNOTATIONS = (
        "RuntimeVisibleAnnotations",
        RuntimeVisibleAnnotationsAttr,
    )
    RUNTIME_INVISIBLE_ANNOTATIONS = (
        "RuntimeInvisibleAnnotations",
        RuntimeInvisibleAnnotationsAttr,
    )
    RUNTIME_VISIBLE_PARAMETER_ANNOTATIONS = (
        "RuntimeVisibleParameterAnnotations",
        RuntimeVisibleParameterAnnotationsAttr,
    )
    RUNTIME_INVISIBLE_PARAMETER_ANNOTATIONS = (
        "RuntimeInvisibleParameterAnnotations",
        RuntimeInvisibleParameterAnnotationsAttr,
    )
    RUNTIME_VISIBLE_TYPE_ANNOTATIONS = (
        "RuntimeVisibleTypeAnnotations",
        RuntimeVisibleTypeAnnotationsAttr,
    )
    RUNTIME_INVISIBLE_TYPE_ANNOTATIONS = (
        "RuntimeInvisibleTypeAnnotations",
        RuntimeInvisibleTypeAnnotationsAttr,
    )
    ANNOTATION_DEFAULT = "AnnotationDefault", AnnotationDefaultAttr
    BOOTSTRAP_METHODS = "BootstrapMethods", BootstrapMethodsAttr
    METHOD_PARAMETERS = "MethodParameters", MethodParametersAttr
    MODULE = "Module", ModuleAttr
    MODULE_PACKAGES = "ModulePackages", ModulePackagesAttr
    MODULE_MAIN_CLASS = "ModuleMainClass", ModuleMainClassAttr
    NEST_HOST = "NestHost", NestHostAttr
    NEST_MEMBERS = "NestMembers", NestMembersAttr
    RECORD = "Record", RecordAttr
    PERMITTED_SUBCLASSES = "PermittedSubclasses", PermittedSubclassesAttr

    UNIMPLEMENTED = "", UnimplementedAttr

    attr_class: type[AttributeInfo]

    def __new__(cls, name: str, attr_class: type[AttributeInfo]) -> AttributeInfoType:
        obj = object.__new__(cls)
        obj._value_ = name
        obj.attr_class = attr_class
        return obj

    @classmethod
    def _missing_(cls, value: object) -> AttributeInfoType:
        obj = cls.UNIMPLEMENTED
        obj._value_ = value
        return obj
