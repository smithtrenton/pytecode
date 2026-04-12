"""Rust-backed hierarchy resolution helpers for JVM internal names."""

from __future__ import annotations

from dataclasses import dataclass
from typing import NotRequired, TypedDict, cast

from .. import _rust
from ..classfile import ClassFile
from ..classfile.constants import ClassAccessFlag, MethodAccessFlag
from ..model import ClassModel

JAVA_LANG_OBJECT = "java/lang/Object"


class HierarchyError(Exception):
    """Base class for hierarchy-resolution failures."""


class UnresolvedClassError(HierarchyError, LookupError):
    """Raised when a hierarchy query needs a class the resolver cannot provide."""


class HierarchyCycleError(HierarchyError):
    """Raised when a malformed class graph contains an ancestry cycle."""


@dataclass(frozen=True, slots=True)
class ResolvedMethod:
    """A method declaration resolved out of raw class metadata."""

    name: str
    descriptor: str
    access_flags: MethodAccessFlag


@dataclass(frozen=True, slots=True)
class ResolvedClass:
    """Resolved hierarchy snapshot for one class or interface."""

    name: str
    super_name: str | None
    interfaces: tuple[str, ...]
    access_flags: ClassAccessFlag
    methods: tuple[ResolvedMethod, ...] = ()

    @property
    def is_interface(self) -> bool:
        """Whether this entry represents an interface."""

        return bool(self.access_flags & ClassAccessFlag.INTERFACE)

    def find_method(self, name: str, descriptor: str) -> ResolvedMethod | None:
        """Return the declared method with the given signature, if present."""

        for method in self.methods:
            if method.name == name and method.descriptor == descriptor:
                return method
        return None

    @classmethod
    def from_classfile(cls, classfile: object) -> ResolvedClass:
        """Build a resolved hierarchy snapshot from a Rust-backed classfile."""

        if not isinstance(classfile, ClassFile):
            raise TypeError("ResolvedClass.from_classfile expects a Rust ClassFile")
        return _resolved_class_from_rust(_rust.rust_resolved_classfile(bytes(classfile.to_bytes())))

    @classmethod
    def from_model(cls, model: object) -> ResolvedClass:
        """Build a resolved hierarchy snapshot from a Rust-backed class model."""

        if not isinstance(model, ClassModel):
            raise TypeError("ResolvedClass.from_model expects a ClassModel")
        return _resolved_class_from_rust(_rust.rust_resolved_classmodel(model))


@dataclass(frozen=True, slots=True)
class InheritedMethod:
    """A matching inherited method declaration found in a supertype."""

    owner: str
    name: str
    descriptor: str
    access_flags: MethodAccessFlag


ClassResolver = _rust.MappingClassResolver
MappingClassResolver = _rust.MappingClassResolver


class _RustResolvedMethodData(TypedDict):
    name: str
    descriptor: str
    access_flags: int


class _RustResolvedClassData(TypedDict):
    name: str
    super_name: str | None
    interfaces: list[str]
    access_flags: int
    methods: NotRequired[list[_RustResolvedMethodData]]


class _RustInheritedMethodData(TypedDict):
    owner: str
    name: str
    descriptor: str
    access_flags: int


def _require_resolver(resolver: object) -> _rust.MappingClassResolver:
    if not isinstance(resolver, _rust.MappingClassResolver):
        raise TypeError("Hierarchy helpers require a MappingClassResolver")
    return resolver


def _resolved_method_from_rust(data: object) -> ResolvedMethod:
    mapping = cast(_RustResolvedMethodData | None, data if isinstance(data, dict) else None)
    if mapping is None:
        raise TypeError("expected Rust hierarchy method mapping")
    return ResolvedMethod(
        name=mapping["name"],
        descriptor=mapping["descriptor"],
        access_flags=MethodAccessFlag(mapping["access_flags"]),
    )


def _resolved_class_from_rust(data: object) -> ResolvedClass:
    mapping = cast(_RustResolvedClassData | None, data if isinstance(data, dict) else None)
    if mapping is None:
        raise TypeError("expected Rust hierarchy class mapping")
    methods = mapping.get("methods", [])
    return ResolvedClass(
        name=mapping["name"],
        super_name=mapping["super_name"],
        interfaces=tuple(mapping["interfaces"]),
        access_flags=ClassAccessFlag(mapping["access_flags"]),
        methods=tuple(_resolved_method_from_rust(method) for method in methods),
    )


def _inherited_method_from_rust(data: object) -> InheritedMethod:
    mapping = cast(_RustInheritedMethodData | None, data if isinstance(data, dict) else None)
    if mapping is None:
        raise TypeError("expected Rust inherited method mapping")
    return InheritedMethod(
        owner=mapping["owner"],
        name=mapping["name"],
        descriptor=mapping["descriptor"],
        access_flags=MethodAccessFlag(mapping["access_flags"]),
    )


def iter_superclasses(
    resolver: ClassResolver,
    class_name: str,
    *,
    include_self: bool = False,
) -> list[ResolvedClass]:
    """Return the linear superclass chain for *class_name*."""

    rust_resolver = _require_resolver(resolver)
    return [
        _resolved_class_from_rust(entry)
        for entry in _rust.rust_iter_superclasses(rust_resolver, class_name, include_self=include_self)
    ]


def iter_supertypes(
    resolver: ClassResolver,
    class_name: str,
    *,
    include_self: bool = False,
) -> list[ResolvedClass]:
    """Return all reachable supertypes of *class_name*."""

    rust_resolver = _require_resolver(resolver)
    return [
        _resolved_class_from_rust(entry)
        for entry in _rust.rust_iter_supertypes(rust_resolver, class_name, include_self=include_self)
    ]


def is_subtype(resolver: ClassResolver, class_name: str, super_name: str) -> bool:
    """Return whether *class_name* is a subtype of *super_name*."""

    return _rust.rust_is_subtype(_require_resolver(resolver), class_name, super_name)


def common_superclass(resolver: ClassResolver, left: str, right: str) -> str:
    """Return the closest common superclass of *left* and *right*."""

    return _rust.rust_common_superclass(_require_resolver(resolver), left, right)


def find_overridden_methods(
    resolver: ClassResolver,
    class_name: str,
    method: ResolvedMethod,
) -> tuple[InheritedMethod, ...]:
    """Return inherited declarations overridden by *method* in *class_name*."""

    rust_resolver = _require_resolver(resolver)
    return tuple(
        _inherited_method_from_rust(entry)
        for entry in _rust.rust_find_overridden_methods(
            rust_resolver,
            class_name,
            method.name,
            method.descriptor,
            int(method.access_flags),
        )
    )


__all__ = [
    "ClassResolver",
    "HierarchyCycleError",
    "HierarchyError",
    "InheritedMethod",
    "JAVA_LANG_OBJECT",
    "MappingClassResolver",
    "ResolvedClass",
    "ResolvedMethod",
    "UnresolvedClassError",
    "common_superclass",
    "find_overridden_methods",
    "is_subtype",
    "iter_superclasses",
    "iter_supertypes",
]
