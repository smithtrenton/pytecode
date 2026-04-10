"""Rust-first Python bindings for the pytecode engine."""

from . import _rust
from ._rust_api import Diagnostic, MappingClassResolver, RustClassModel, verify_classfile, verify_classmodel
from .archive import JarFile

RustClassReader = _rust.ClassReader
RustClassWriter = _rust.ClassWriter
ClassReader = RustClassReader
ClassWriter = RustClassWriter
ClassModel = RustClassModel
backend_info = _rust.backend_info

__all__ = [
    "ClassModel",
    "ClassReader",
    "ClassWriter",
    "JarFile",
    "RustClassReader",
    "RustClassWriter",
    "RustClassModel",
    "MappingClassResolver",
    "Diagnostic",
    "verify_classfile",
    "verify_classmodel",
    "backend_info",
]
