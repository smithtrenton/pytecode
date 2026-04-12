"""Raw classfile parsing and bytecode inspection surface.

Use this module when you want a close-to-the-bytes view of a class file rather
than the mutable symbolic editing API in :mod:`pytecode.model`.
"""

from __future__ import annotations

from .. import _rust
from .bytecode import ArrayType, InsnInfoType


def _document_property(cls: type[object], name: str, doc: str, return_annotation: object) -> None:
    descriptor = cls.__dict__[name]

    def getter(self: object) -> object:
        return descriptor.__get__(self, type(self))

    getter.__name__ = name
    getter.__doc__ = doc
    getter.__annotations__ = {"return": return_annotation}
    setattr(cls, name, property(getter, doc=doc))


ClassFile = _rust.ClassFile
ClassReader = _rust.ClassReader
ClassWriter = _rust.ClassWriter
InsnInfo = _rust.InsnInfo
MatchOffsetPair = _rust.MatchOffsetPair
ExceptionInfo = _rust.ExceptionInfo

ClassReader.__doc__ = "Parse raw classfile bytes into a read-only ``ClassFile`` view."

_classreader_from_bytes = ClassReader.__dict__["from_bytes"]


def _documented_classreader_from_bytes(cls: type[ClassReader], bytes_or_bytearray: bytes) -> ClassReader:
    """Create a reader from in-memory classfile bytes."""

    return _classreader_from_bytes.__get__(cls, cls)(bytes_or_bytearray)


ClassReader.from_bytes = classmethod(_documented_classreader_from_bytes)

_classreader_from_file = ClassReader.__dict__["from_file"]


def _documented_classreader_from_file(cls: type[ClassReader], path: str) -> ClassReader:
    """Read a classfile from disk and return a new reader."""

    return _classreader_from_file.__get__(cls, cls)(path)


ClassReader.from_file = classmethod(_documented_classreader_from_file)

_document_property(
    ClassReader,
    "class_info",
    "Parsed raw classfile structure exposed by this reader.",
    ClassFile,
)

ClassWriter.__doc__ = "Serialize raw ``ClassFile`` objects back to bytes."

_classwriter_write = ClassWriter.write


def _documented_classwriter_write(classfile: ClassFile) -> bytes:
    """Encode a raw classfile object into JVM classfile bytes."""

    return _classwriter_write(classfile)


ClassWriter.write = staticmethod(_documented_classwriter_write)

__all__ = [
    "ArrayType",
    "ClassFile",
    "ClassReader",
    "ClassWriter",
    "ExceptionInfo",
    "InsnInfo",
    "InsnInfoType",
    "MatchOffsetPair",
]
