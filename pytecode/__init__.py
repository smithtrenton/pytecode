"""Python library for parsing, inspecting, manipulating, and emitting JVM class files.

Provides four top-level entry points:

- ``ClassReader`` — parse ``.class`` bytes into a ``ClassFile`` tree.
- ``ClassWriter`` — serialize a ``ClassFile`` tree back to ``.class`` bytes.
- ``ClassModel`` — mutable editing model with symbolic references and
  label-aware code editing.
- ``JarFile`` — read, mutate, and rewrite JAR archives.

Additional submodules expose transforms, descriptors, analysis, hierarchy
resolution, validation, operands, labels, debug-info helpers, and the
underlying data types.  See ``pytecode.transforms``, ``pytecode.analysis``,
``pytecode.verify``, and the other documented submodules for details.
"""

from .class_reader import ClassReader
from .class_writer import ClassWriter
from .jar import JarFile
from .model import ClassModel

__all__ = ["ClassModel", "ClassReader", "ClassWriter", "JarFile"]
