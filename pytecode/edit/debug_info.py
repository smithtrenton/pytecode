"""Utilities for managing JVM debug information in class files.

Provides policies, state tracking, and stripping helpers for debug
attributes such as line numbers, local variables, and source file
references.
"""

from __future__ import annotations

from enum import Enum
from typing import TYPE_CHECKING, overload

from ..classfile.attributes import AttributeInfo, MethodParametersAttr, SourceDebugExtensionAttr, SourceFileAttr

if TYPE_CHECKING:
    from .model import ClassModel, CodeModel, MethodModel

__all__ = [
    "DebugInfoPolicy",
    "DebugInfoState",
    "apply_debug_info_policy",
    "has_class_debug_info",
    "has_code_debug_info",
    "is_class_debug_info_stale",
    "is_code_debug_info_stale",
    "mark_class_debug_info_stale",
    "mark_code_debug_info_stale",
    "normalize_debug_info_policy",
    "skip_debug_method_attributes",
    "strip_class_debug_attributes",
    "strip_debug_info",
]


class DebugInfoPolicy(Enum):
    """Policy controlling how debug information is handled.

    Attributes:
        PRESERVE: Keep existing debug information unchanged.
        STRIP: Remove all debug information from the target.
    """

    PRESERVE = "preserve"
    STRIP = "strip"


class DebugInfoState(Enum):
    """Staleness state of debug information on a model.

    Attributes:
        FRESH: Debug information is up to date.
        STALE: Debug information may be outdated due to bytecode changes.
    """

    FRESH = "fresh"
    STALE = "stale"


def normalize_debug_info_policy(policy: DebugInfoPolicy | str) -> DebugInfoPolicy:
    """Coerce a string or enum value into a ``DebugInfoPolicy``.

    Args:
        policy: A ``DebugInfoPolicy`` member or its string value.

    Returns:
        The corresponding ``DebugInfoPolicy`` member.

    Raises:
        ValueError: If the string does not match any policy value.
    """
    if isinstance(policy, DebugInfoPolicy):
        return policy
    try:
        return DebugInfoPolicy(policy)
    except ValueError as exc:
        expected = ", ".join(member.value for member in DebugInfoPolicy)
        raise ValueError(f"debug_info must be one of: {expected}") from exc


def strip_class_debug_attributes(attributes: list[AttributeInfo]) -> list[AttributeInfo]:
    """Filter out class-level debug attributes from a list.

    Removes ``SourceFileAttr`` and ``SourceDebugExtensionAttr`` entries,
    returning only the non-debug attributes.

    Args:
        attributes: The attribute list to filter.

    Returns:
        A new list with class-level debug attributes removed.
    """
    return [
        attribute for attribute in attributes if not isinstance(attribute, (SourceFileAttr, SourceDebugExtensionAttr))
    ]


def skip_debug_method_attributes(attributes: list[AttributeInfo]) -> list[AttributeInfo]:
    """Filter out method-level debug attributes from a list.

    Removes ``MethodParametersAttr`` entries, returning only the
    remaining attributes.

    Args:
        attributes: The attribute list to filter.

    Returns:
        A new list with method-level debug attributes removed.
    """
    return [attribute for attribute in attributes if not isinstance(attribute, MethodParametersAttr)]


def has_class_debug_info(target: ClassModel) -> bool:
    """Check whether a class model contains class-level debug attributes.

    Args:
        target: The class model to inspect.

    Returns:
        ``True`` if the class has ``SourceFileAttr`` or
        ``SourceDebugExtensionAttr`` attributes.
    """
    return any(isinstance(attribute, (SourceFileAttr, SourceDebugExtensionAttr)) for attribute in target.attributes)


def has_code_debug_info(target: CodeModel) -> bool:
    """Check whether a code model contains code-level debug information.

    Args:
        target: The code model to inspect.

    Returns:
        ``True`` if the code has line numbers, local variables, or local
        variable type entries.
    """
    return bool(target.line_numbers or target.local_variables or target.local_variable_types)


def is_class_debug_info_stale(target: ClassModel) -> bool:
    """Check whether a class model has stale class-level debug info.

    Args:
        target: The class model to inspect.

    Returns:
        ``True`` if the debug state is stale and debug attributes are present.
    """
    return target.debug_info_state is DebugInfoState.STALE and has_class_debug_info(target)


def is_code_debug_info_stale(target: CodeModel) -> bool:
    """Check whether a code model has stale code-level debug info.

    Args:
        target: The code model to inspect.

    Returns:
        ``True`` if the debug state is stale and debug information is present.
    """
    return target.debug_info_state is DebugInfoState.STALE and has_code_debug_info(target)


def mark_class_debug_info_stale(target: ClassModel) -> ClassModel:
    """Mark a class model's debug information as stale.

    If the class has debug attributes, sets its debug state to
    ``DebugInfoState.STALE``.

    Args:
        target: The class model to mark.

    Returns:
        The same class model, possibly with updated debug state.
    """
    if has_class_debug_info(target):
        target.debug_info_state = DebugInfoState.STALE
    target._rust_clean = False
    target._rust_bytes = None
    return target


@overload
def mark_code_debug_info_stale(target: CodeModel) -> CodeModel: ...


@overload
def mark_code_debug_info_stale(target: MethodModel) -> MethodModel: ...


@overload
def mark_code_debug_info_stale(target: ClassModel) -> ClassModel: ...


def mark_code_debug_info_stale(target: object) -> object:
    """Mark code-level debug information as stale.

    Accepts a ``CodeModel``, ``MethodModel``, or ``ClassModel``. For a
    ``CodeModel``, sets its state to stale if it has debug info. For a
    ``MethodModel``, delegates to its code attribute. For a ``ClassModel``,
    recursively marks all methods.

    Args:
        target: The model whose code debug info should be marked stale.

    Returns:
        The same model, possibly with updated debug state.

    Raises:
        TypeError: If *target* is not a supported model type.
    """
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
        target._rust_clean = False
        target._rust_bytes = None
        return target
    raise TypeError("code-debug helper expects a CodeModel, MethodModel, or ClassModel")


@overload
def apply_debug_info_policy(target: CodeModel, policy: DebugInfoPolicy | str) -> CodeModel: ...


@overload
def apply_debug_info_policy(target: MethodModel, policy: DebugInfoPolicy | str) -> MethodModel: ...


@overload
def apply_debug_info_policy(target: ClassModel, policy: DebugInfoPolicy | str) -> ClassModel: ...


def apply_debug_info_policy(target: object, policy: DebugInfoPolicy | str) -> object:
    """Apply a debug information policy to a model.

    When the policy is ``STRIP``, removes all debug information from the
    target. When the policy is ``PRESERVE``, leaves the target unchanged.

    Args:
        target: The model to apply the policy to.
        policy: The policy to apply, as an enum member or string.

    Returns:
        The same model, with debug information stripped if requested.

    Raises:
        TypeError: If *target* is not a supported model type.
        ValueError: If *policy* is an invalid string.
    """
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
    """Remove all debug information from a model.

    For a ``CodeModel``, clears line numbers, local variables, and local
    variable types. For a ``MethodModel``, delegates to its code attribute.
    For a ``ClassModel``, strips class-level debug attributes and recurses
    into all methods.

    Args:
        target: The model to strip debug information from.

    Returns:
        The same model with debug information removed and state set to fresh.

    Raises:
        TypeError: If *target* is not a supported model type.
    """
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
        target._rust_clean = False
        target._rust_bytes = None
        for method in target.methods:
            strip_debug_info(method)
        return target
    raise TypeError("debug-info helpers expect a CodeModel, MethodModel, or ClassModel")
