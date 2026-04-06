"""JVM type descriptor and generic signature parsing utilities.

This module re-exports from either the Cython-accelerated implementation
or the pure-Python fallback depending on availability and the
``PYTECODE_BLOCK_CYTHON`` environment variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.classfile._descriptors_py import (
        VOID as VOID,
    )
    from pytecode.classfile._descriptors_py import (
        ArrayType as ArrayType,
    )
    from pytecode.classfile._descriptors_py import (
        ArrayTypeSignature as ArrayTypeSignature,
    )
    from pytecode.classfile._descriptors_py import (
        BaseType as BaseType,
    )
    from pytecode.classfile._descriptors_py import (
        ClassSignature as ClassSignature,
    )
    from pytecode.classfile._descriptors_py import (
        ClassTypeSignature as ClassTypeSignature,
    )
    from pytecode.classfile._descriptors_py import (
        FieldDescriptor as FieldDescriptor,
    )
    from pytecode.classfile._descriptors_py import (
        FieldSignature as FieldSignature,
    )
    from pytecode.classfile._descriptors_py import (
        InnerClassType as InnerClassType,
    )
    from pytecode.classfile._descriptors_py import (
        JavaTypeSignature as JavaTypeSignature,
    )
    from pytecode.classfile._descriptors_py import (
        MethodDescriptor as MethodDescriptor,
    )
    from pytecode.classfile._descriptors_py import (
        MethodSignature as MethodSignature,
    )
    from pytecode.classfile._descriptors_py import (
        ObjectType as ObjectType,
    )
    from pytecode.classfile._descriptors_py import (
        ReferenceTypeSignature as ReferenceTypeSignature,
    )
    from pytecode.classfile._descriptors_py import (
        ReturnType as ReturnType,
    )
    from pytecode.classfile._descriptors_py import (
        TypeArgument as TypeArgument,
    )
    from pytecode.classfile._descriptors_py import (
        TypeParameter as TypeParameter,
    )
    from pytecode.classfile._descriptors_py import (
        TypeVariable as TypeVariable,
    )
    from pytecode.classfile._descriptors_py import (
        VoidType as VoidType,
    )
    from pytecode.classfile._descriptors_py import (
        is_valid_field_descriptor as is_valid_field_descriptor,
    )
    from pytecode.classfile._descriptors_py import (
        is_valid_method_descriptor as is_valid_method_descriptor,
    )
    from pytecode.classfile._descriptors_py import (
        parameter_slot_count as parameter_slot_count,
    )
    from pytecode.classfile._descriptors_py import (
        parse_class_signature as parse_class_signature,
    )
    from pytecode.classfile._descriptors_py import (
        parse_field_descriptor as parse_field_descriptor,
    )
    from pytecode.classfile._descriptors_py import (
        parse_field_signature as parse_field_signature,
    )
    from pytecode.classfile._descriptors_py import (
        parse_method_descriptor as parse_method_descriptor,
    )
    from pytecode.classfile._descriptors_py import (
        parse_method_signature as parse_method_signature,
    )
    from pytecode.classfile._descriptors_py import (
        slot_size as slot_size,
    )
    from pytecode.classfile._descriptors_py import (
        to_descriptor as to_descriptor,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.classfile._descriptors_cy",
        "pytecode.classfile._descriptors_py",
    )

    VOID = _impl.VOID
    ArrayType = _impl.ArrayType
    ArrayTypeSignature = _impl.ArrayTypeSignature
    BaseType = _impl.BaseType
    ClassSignature = _impl.ClassSignature
    ClassTypeSignature = _impl.ClassTypeSignature
    InnerClassType = _impl.InnerClassType
    MethodDescriptor = _impl.MethodDescriptor
    MethodSignature = _impl.MethodSignature
    ObjectType = _impl.ObjectType
    TypeArgument = _impl.TypeArgument
    TypeParameter = _impl.TypeParameter
    TypeVariable = _impl.TypeVariable
    VoidType = _impl.VoidType
    is_valid_field_descriptor = _impl.is_valid_field_descriptor
    is_valid_method_descriptor = _impl.is_valid_method_descriptor
    parameter_slot_count = _impl.parameter_slot_count
    parse_class_signature = _impl.parse_class_signature
    parse_field_descriptor = _impl.parse_field_descriptor
    parse_field_signature = _impl.parse_field_signature
    parse_method_descriptor = _impl.parse_method_descriptor
    parse_method_signature = _impl.parse_method_signature
    slot_size = _impl.slot_size
    to_descriptor = _impl.to_descriptor

    from pytecode.classfile._descriptors_py import FieldDescriptor as FieldDescriptor  # noqa: E402
    from pytecode.classfile._descriptors_py import FieldSignature as FieldSignature  # noqa: E402
    from pytecode.classfile._descriptors_py import JavaTypeSignature as JavaTypeSignature  # noqa: E402
    from pytecode.classfile._descriptors_py import ReferenceTypeSignature as ReferenceTypeSignature  # noqa: E402
    from pytecode.classfile._descriptors_py import ReturnType as ReturnType  # noqa: E402

__all__ = [
    "ArrayType",
    "ArrayTypeSignature",
    "BaseType",
    "ClassSignature",
    "ClassTypeSignature",
    "FieldDescriptor",
    "FieldSignature",
    "InnerClassType",
    "JavaTypeSignature",
    "MethodDescriptor",
    "MethodSignature",
    "ObjectType",
    "ReferenceTypeSignature",
    "ReturnType",
    "TypeArgument",
    "TypeParameter",
    "TypeVariable",
    "VOID",
    "VoidType",
    "is_valid_field_descriptor",
    "is_valid_method_descriptor",
    "parameter_slot_count",
    "parse_class_signature",
    "parse_field_descriptor",
    "parse_field_signature",
    "parse_method_descriptor",
    "parse_method_signature",
    "slot_size",
    "to_descriptor",
]
