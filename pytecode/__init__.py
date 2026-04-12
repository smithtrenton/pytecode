"""Rust-backed top-level entry points for common pytecode workflows.

Import from this module when you want the shortest path to the core editing
surface:

- ``ClassReader`` and ``ClassWriter`` for raw classfile IO
- ``ClassModel`` for mutable symbolic editing
- ``JarFile`` for archive reads and rewrites

For more specialized APIs, prefer the semantic submodules such as
``pytecode.classfile``, ``pytecode.model``, ``pytecode.archive``, and
``pytecode.analysis``.
"""

from .archive import JarFile
from .classfile import ClassReader, ClassWriter
from .model import ClassModel

__all__ = [
    "ClassModel",
    "ClassReader",
    "ClassWriter",
    "JarFile",
]
