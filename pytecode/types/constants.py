from enum import Enum, IntFlag

from . import attributes, constant_pool


MAGIC = 0xCAFEBABE


class ModuleAccessFlag(IntFlag):
    OPEN = 0x0020
    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class ModuleRequiresAccessFlag(IntFlag):
    TRANSITIVE = 0x0020
    STATIC_PHASE = 0x0040
    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class ModuleExportsAccessFlag(IntFlag):
    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class ModuleOpensAccessFlag(IntFlag):
    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class ClassAccessFlag(IntFlag):
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
    FINAL = 0x0010
    SYNTHETIC = 0x1000
    MANDATED = 0x8000


class FieldAccessFlag(IntFlag):
    PUBLIC = 0x0001
    PRIVATE = 0x0002
    PROTECTED = 0x0004
    STATIC = 0x0008
    FINAL = 0x0010
    VOLATILE = 0x0040
    TRANSIENT = 0x0080
    SYNTHETIC = 0x1000
    ENUM = 0x4000


class FieldType(Enum):
    BYTE = "B"
    CHAR = "C"
    DOUBLE = "D"
    FLOAT = "F"
    INT = "I"
    LONG = "J"
    REF = "L"
    SHORT = "S"
    BOOL = "Z"
    ARRAY = "["
