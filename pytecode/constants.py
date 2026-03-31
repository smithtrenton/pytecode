"""Constants for JVM class file parsing (JVMS §4)."""

from enum import Enum, IntEnum, IntFlag

__all__ = [
    "ClassAccessFlag",
    "FieldAccessFlag",
    "MAGIC",
    "MethodAccessFlag",
    "MethodParameterAccessFlag",
    "ModuleAccessFlag",
    "ModuleExportsAccessFlag",
    "ModuleOpensAccessFlag",
    "ModuleRequiresAccessFlag",
    "NestedClassAccessFlag",
    "TargetInfoType",
    "TargetType",
    "TypePathKind",
    "VerificationType",
]

MAGIC = 0xCAFEBABE
"""The magic number identifying a valid Java class file (§4.1)."""


class ModuleAccessFlag(IntFlag):
    """Access flags for modules (§4.7.25, Table 4.7.25-A)."""

    OPEN = 0x0020
    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class ModuleRequiresAccessFlag(IntFlag):
    """Access flags for module requires directives (§4.7.25, Table 4.7.25-B)."""

    TRANSITIVE = 0x0020
    STATIC_PHASE = 0x0040
    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class ModuleExportsAccessFlag(IntFlag):
    """Access flags for module exports directives (§4.7.25, Table 4.7.25-C)."""

    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class ModuleOpensAccessFlag(IntFlag):
    """Access flags for module opens directives (§4.7.25, Table 4.7.25-D)."""

    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class ClassAccessFlag(IntFlag):
    """Access flags for classes (§4.1, Table 4.1-B)."""

    PUBLIC = 0x0001
    FINAL = 0x0010
    SUPER = 0x0020
    INTERFACE = 0x0200
    ABSTRACT = 0x0400
    SYNTHETIC = 0x1000
    ANNOTATION = 0x2000
    ENUM = 0x4000
    MODULE = 0x8000


class NestedClassAccessFlag(IntFlag):
    """Access flags for nested classes (§4.7.6, Table 4.7.6-A)."""

    PUBLIC = 0x0001
    PRIVATE = 0x0002
    PROTECTED = 0x0004
    STATIC = 0x0008
    FINAL = 0x0010
    INTERFACE = 0x0200
    ABSTRACT = 0x0400
    SYNTHETIC = 0x1000
    ANNOTATION = 0x2000
    ENUM = 0x4000


class MethodAccessFlag(IntFlag):
    """Access flags for methods (§4.6, Table 4.6-A)."""

    PUBLIC = 0x0001
    PRIVATE = 0x0002
    PROTECTED = 0x0004
    STATIC = 0x0008
    FINAL = 0x0010
    SYNCHRONIZED = 0x0020
    BRIDGE = 0x0040
    VARARGS = 0x0080
    NATIVE = 0x0100
    ABSTRACT = 0x0400
    STRICT = 0x0800
    SYNTHETIC = 0x1000


class MethodParameterAccessFlag(IntFlag):
    """Access flags for method parameters (§4.7.24, Table 4.7.24-A)."""

    FINAL = 0x0010
    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class FieldAccessFlag(IntFlag):
    """Access flags for fields (§4.5, Table 4.5-A)."""

    PUBLIC = 0x0001
    PRIVATE = 0x0002
    PROTECTED = 0x0004
    STATIC = 0x0008
    FINAL = 0x0010
    VOLATILE = 0x0040
    TRANSIENT = 0x0080
    SYNTHETIC = 0x1000
    ENUM = 0x4000


class TargetType(IntEnum):
    """Target type values for type annotations (§4.7.20, Table 4.7.20-A/B)."""

    TYPE_PARAMETER_GENERIC_CLASS_OR_INTERFACE = 0x00
    TYPE_PARAMETER_GENERIC_METHOD_OR_CONSTRUCTOR = 0x01
    SUPERTYPE = 0x10
    TYPE_PARAMETER_BOUND_GENERIC_CLASS_OR_INTERFACE = 0x11
    TYPE_PARAMETER_BOUND_GENERIC_METHOD_OR_CONSTRUCTOR = 0x12
    TYPE_IN_FIELD_OR_RECORD = 0x13
    RETURN_OR_OBJECT_TYPE = 0x14
    RECEIVER_TYPE_METHOD_OR_CONSTRUCTOR = 0x15
    FORMAL_PARAMETER_METHOD_CONSTRUCTOR_OR_LAMBDA = 0x16
    TYPE_THROWS = 0x17
    TYPE_LOCAL_VARIABLE = 0x40
    TYPE_RESOURCE_VARIABLE = 0x41
    TYPE_EXCEPTION_PARAMETER = 0x42
    TYPE_INSTANCEOF = 0x43
    TYPE_NEW = 0x44
    TYPE_METHOD_NEW = 0x45
    TYPE_METHOD_IDENTIFIER = 0x46
    TYPE_CAST = 0x47
    TYPE_GENERIC_CONSTRUCTOR = 0x48
    TYPE_GENERIC_METHOD = 0x49
    TYPE_GENERIC_CONSTRUCTOR_NEW = 0x4A
    TYPE_GENERIC_METHOD_IDENTIFIER = 0x4B


class TargetInfoType(Enum):
    """Mapping from target_info union discriminants to target types (§4.7.20.1)."""

    TYPE_PARAMETER = (
        TargetType.TYPE_PARAMETER_GENERIC_CLASS_OR_INTERFACE,
        TargetType.TYPE_PARAMETER_GENERIC_METHOD_OR_CONSTRUCTOR,
    )
    SUPERTYPE = (TargetType.SUPERTYPE,)
    TYPE_PARAMETER_BOUND = (
        TargetType.TYPE_PARAMETER_BOUND_GENERIC_CLASS_OR_INTERFACE,
        TargetType.TYPE_PARAMETER_BOUND_GENERIC_METHOD_OR_CONSTRUCTOR,
    )
    EMPTY = (
        TargetType.TYPE_IN_FIELD_OR_RECORD,
        TargetType.RETURN_OR_OBJECT_TYPE,
        TargetType.RECEIVER_TYPE_METHOD_OR_CONSTRUCTOR,
    )
    FORMAL_PARAMETER = (TargetType.FORMAL_PARAMETER_METHOD_CONSTRUCTOR_OR_LAMBDA,)
    THROWS = (TargetType.TYPE_THROWS,)
    LOCALVAR = (TargetType.TYPE_LOCAL_VARIABLE, TargetType.TYPE_RESOURCE_VARIABLE)
    CATCH = (TargetType.TYPE_EXCEPTION_PARAMETER,)
    OFFSET = (
        TargetType.TYPE_INSTANCEOF,
        TargetType.TYPE_NEW,
        TargetType.TYPE_METHOD_NEW,
        TargetType.TYPE_METHOD_IDENTIFIER,
    )
    TYPE_ARGUMENT = (
        TargetType.TYPE_CAST,
        TargetType.TYPE_GENERIC_CONSTRUCTOR,
        TargetType.TYPE_GENERIC_METHOD,
        TargetType.TYPE_GENERIC_CONSTRUCTOR_NEW,
        TargetType.TYPE_GENERIC_METHOD_IDENTIFIER,
    )


class TypePathKind(IntEnum):
    """Kind values for type_path entries in type annotations (§4.7.20.2, Table 4.7.20.2-A)."""

    ARRAY_TYPE = 0
    NESTED_TYPE = 1
    WILDCARD_TYPE = 2
    PARAMETERIZED_TYPE = 3


class VerificationType(IntEnum):
    """Verification type info tags for StackMapTable entries (§4.7.4, Table 4.7.4-A)."""

    TOP = 0
    INTEGER = 1
    FLOAT = 2
    DOUBLE = 3
    LONG = 4
    NULL = 5
    UNINITIALIZED_THIS = 6
    OBJECT = 7
    UNINITIALIZED = 8
