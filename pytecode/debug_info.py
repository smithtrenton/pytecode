from __future__ import annotations

from enum import Enum
from typing import TYPE_CHECKING, overload

from .attributes import AttributeInfo, MethodParametersAttr, SourceDebugExtensionAttr, SourceFileAttr

if TYPE_CHECKING:
    from .model import ClassModel, CodeModel, MethodModel


class DebugInfoPolicy(Enum):
    PRESERVE = "preserve"
    STRIP = "strip"


class DebugInfoState(Enum):
    FRESH = "fresh"
    STALE = "stale"


def normalize_debug_info_policy(policy: DebugInfoPolicy | str) -> DebugInfoPolicy:
    if isinstance(policy, DebugInfoPolicy):
        return policy
    try:
        return DebugInfoPolicy(policy)
    except ValueError as exc:
        expected = ", ".join(member.value for member in DebugInfoPolicy)
        raise ValueError(f"debug_info must be one of: {expected}") from exc


def strip_class_debug_attributes(attributes: list[AttributeInfo]) -> list[AttributeInfo]:
    return [
        attribute for attribute in attributes if not isinstance(attribute, (SourceFileAttr, SourceDebugExtensionAttr))
    ]


def skip_debug_method_attributes(attributes: list[AttributeInfo]) -> list[AttributeInfo]:
    return [attribute for attribute in attributes if not isinstance(attribute, MethodParametersAttr)]


def has_class_debug_info(target: ClassModel) -> bool:
    return any(isinstance(attribute, (SourceFileAttr, SourceDebugExtensionAttr)) for attribute in target.attributes)


def has_code_debug_info(target: CodeModel) -> bool:
    return bool(target.line_numbers or target.local_variables or target.local_variable_types)


def is_class_debug_info_stale(target: ClassModel) -> bool:
    return target.debug_info_state is DebugInfoState.STALE and has_class_debug_info(target)


def is_code_debug_info_stale(target: CodeModel) -> bool:
    return target.debug_info_state is DebugInfoState.STALE and has_code_debug_info(target)


def mark_class_debug_info_stale(target: ClassModel) -> ClassModel:
    if has_class_debug_info(target):
        target.debug_info_state = DebugInfoState.STALE
    return target


@overload
def mark_code_debug_info_stale(target: CodeModel) -> CodeModel: ...


@overload
def mark_code_debug_info_stale(target: MethodModel) -> MethodModel: ...


@overload
def mark_code_debug_info_stale(target: ClassModel) -> ClassModel: ...


def mark_code_debug_info_stale(target: object) -> object:
    from .model import ClassModel, CodeModel, MethodModel

    if isinstance(target, CodeModel):
        if has_code_debug_info(target):
            target.debug_info_state = DebugInfoState.STALE
        return target
    if isinstance(target, MethodModel):
        if target.code is not None:
            mark_code_debug_info_stale(target.code)
        return target
    if isinstance(target, ClassModel):
        for method in target.methods:
            mark_code_debug_info_stale(method)
        return target
    raise TypeError("code-debug helper expects a CodeModel, MethodModel, or ClassModel")


@overload
def apply_debug_info_policy(target: CodeModel, policy: DebugInfoPolicy | str) -> CodeModel: ...


@overload
def apply_debug_info_policy(target: MethodModel, policy: DebugInfoPolicy | str) -> MethodModel: ...


@overload
def apply_debug_info_policy(target: ClassModel, policy: DebugInfoPolicy | str) -> ClassModel: ...


def apply_debug_info_policy(target: object, policy: DebugInfoPolicy | str) -> object:
    from .model import ClassModel, CodeModel, MethodModel

    debug_policy = normalize_debug_info_policy(policy)
    if isinstance(target, CodeModel):
        if debug_policy is DebugInfoPolicy.STRIP:
            strip_debug_info(target)
        return target
    if isinstance(target, MethodModel):
        if debug_policy is DebugInfoPolicy.STRIP:
            strip_debug_info(target)
        return target
    if isinstance(target, ClassModel):
        if debug_policy is DebugInfoPolicy.STRIP:
            strip_debug_info(target)
        return target
    raise TypeError("debug-info helpers expect a CodeModel, MethodModel, or ClassModel")


@overload
def strip_debug_info(target: CodeModel) -> CodeModel: ...


@overload
def strip_debug_info(target: MethodModel) -> MethodModel: ...


@overload
def strip_debug_info(target: ClassModel) -> ClassModel: ...


def strip_debug_info(target: object) -> object:
    from .model import ClassModel, CodeModel, MethodModel

    if isinstance(target, CodeModel):
        target.line_numbers.clear()
        target.local_variables.clear()
        target.local_variable_types.clear()
        target.debug_info_state = DebugInfoState.FRESH
        return target
    if isinstance(target, MethodModel):
        if target.code is not None:
            strip_debug_info(target.code)
        return target
    if isinstance(target, ClassModel):
        target.attributes[:] = strip_class_debug_attributes(target.attributes)
        target.debug_info_state = DebugInfoState.FRESH
        for method in target.methods:
            strip_debug_info(method)
        return target
    raise TypeError("debug-info helpers expect a CodeModel, MethodModel, or ClassModel")
