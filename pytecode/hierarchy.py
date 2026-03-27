"""Class hierarchy resolution helpers for JVM internal names.

This module provides a small, pluggable foundation for hierarchy-aware JVM
analysis. It intentionally stays narrower than full verifier type merging:
it resolves class/interface metadata, answers subtype queries, walks ancestor
chains, finds common superclasses, and reports inherited method declarations
that a class method overrides.
"""

from __future__ import annotations

from collections.abc import Iterable, Iterator
from dataclasses import dataclass
from typing import TYPE_CHECKING, Protocol

from .constant_pool import ClassInfo
from .constant_pool_builder import ConstantPoolBuilder
from .constants import ClassAccessFlag, MethodAccessFlag
from .info import ClassFile

if TYPE_CHECKING:
    from .model import ClassModel

JAVA_LANG_OBJECT = "java/lang/Object"


class HierarchyError(Exception):
    """Base class for hierarchy-resolution failures."""


class UnresolvedClassError(HierarchyError, LookupError):
    """Raised when a hierarchy query needs a class the resolver cannot provide."""

    def __init__(self, class_name: str) -> None:
        self.class_name = class_name
        super().__init__(f"Could not resolve class {class_name!r}")


class HierarchyCycleError(HierarchyError):
    """Raised when a malformed class graph contains an ancestry cycle."""

    def __init__(self, cycle: tuple[str, ...]) -> None:
        self.cycle = cycle
        joined = " -> ".join(cycle)
        super().__init__(f"Hierarchy cycle detected: {joined}")


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
    def from_classfile(cls, classfile: ClassFile) -> ResolvedClass:
        """Build a resolved hierarchy snapshot from a parsed ``ClassFile``."""

        cp = ConstantPoolBuilder.from_pool(classfile.constant_pool)
        methods = tuple(
            ResolvedMethod(
                cp.resolve_utf8(method.name_index),
                cp.resolve_utf8(method.descriptor_index),
                method.access_flags,
            )
            for method in classfile.methods
        )
        return cls(
            name=_resolve_class_name(cp, classfile.this_class),
            super_name=None if classfile.super_class == 0 else _resolve_class_name(cp, classfile.super_class),
            interfaces=tuple(_resolve_class_name(cp, index) for index in classfile.interfaces),
            access_flags=classfile.access_flags,
            methods=methods,
        )

    @classmethod
    def from_model(cls, model: ClassModel) -> ResolvedClass:
        """Build a resolved hierarchy snapshot from a ``ClassModel``."""

        return cls(
            name=model.name,
            super_name=model.super_name,
            interfaces=tuple(model.interfaces),
            access_flags=model.access_flags,
            methods=tuple(
                ResolvedMethod(method.name, method.descriptor, method.access_flags) for method in model.methods
            ),
        )


@dataclass(frozen=True, slots=True)
class InheritedMethod:
    """A matching inherited method declaration found in a supertype."""

    owner: str
    name: str
    descriptor: str
    access_flags: MethodAccessFlag


class ClassResolver(Protocol):
    """Protocol for supplying resolved class metadata by internal name."""

    def resolve_class(self, class_name: str) -> ResolvedClass | None:
        """Return resolved metadata for *class_name*, or ``None`` if unavailable."""


class MappingClassResolver:
    """Simple in-memory ``ClassResolver`` backed by resolved class snapshots."""

    __slots__ = ("_classes",)

    def __init__(self, classes: Iterable[ResolvedClass]) -> None:
        mapping: dict[str, ResolvedClass] = {}
        for resolved in classes:
            if resolved.name in mapping:
                raise ValueError(f"Duplicate resolved class {resolved.name!r}")
            mapping[resolved.name] = resolved
        self._classes = mapping

    def resolve_class(self, class_name: str) -> ResolvedClass | None:
        """Resolve *class_name* from the in-memory mapping."""

        return self._classes.get(class_name)

    @classmethod
    def from_classfiles(cls, classfiles: Iterable[ClassFile]) -> MappingClassResolver:
        """Build a resolver from parsed ``ClassFile`` objects."""

        return cls(ResolvedClass.from_classfile(classfile) for classfile in classfiles)

    @classmethod
    def from_models(cls, models: Iterable[ClassModel]) -> MappingClassResolver:
        """Build a resolver from ``ClassModel`` objects."""

        return cls(ResolvedClass.from_model(model) for model in models)


_IMPLICIT_OBJECT = ResolvedClass(
    name=JAVA_LANG_OBJECT,
    super_name=None,
    interfaces=(),
    access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
    methods=(),
)
_NON_OVERRIDABLE_METHOD_FLAGS = MethodAccessFlag.PRIVATE | MethodAccessFlag.STATIC | MethodAccessFlag.FINAL


def iter_superclasses(
    resolver: ClassResolver,
    class_name: str,
    *,
    include_self: bool = False,
) -> Iterator[ResolvedClass]:
    """Yield the linear superclass chain for *class_name*.

    The returned chain always terminates at ``java/lang/Object``. If the
    resolver does not explicitly provide that root class, an implicit stub is
    used so ordinary user-defined class hierarchies can still terminate cleanly.
    """

    current_name = class_name if include_self else _resolve_required(resolver, class_name).super_name
    seen: set[str] = set()
    path: list[str] = [class_name]

    while current_name is not None:
        if current_name in seen:
            cycle_start = path.index(current_name) if current_name in path else 0
            cycle = tuple(path[cycle_start:] + [current_name])
            raise HierarchyCycleError(cycle)

        seen.add(current_name)
        current = _resolve_required(resolver, current_name)
        yield current
        if path[-1] != current.name:
            path.append(current.name)
        current_name = current.super_name


def iter_supertypes(
    resolver: ClassResolver,
    class_name: str,
    *,
    include_self: bool = False,
) -> Iterator[ResolvedClass]:
    """Yield all reachable supertypes of *class_name*.

    Traversal is deterministic: the direct superclass chain is explored before
    interface edges, and diamond/interface duplicates are yielded only once.
    Malformed ancestry cycles still raise ``HierarchyCycleError``.
    """

    root = _resolve_required(resolver, class_name)
    seen: set[str] = set()

    def visit(name: str, stack: tuple[str, ...]) -> Iterator[ResolvedClass]:
        if name in stack:
            cycle = stack[stack.index(name) :] + (name,)
            raise HierarchyCycleError(cycle)
        if name in seen:
            return

        resolved = _resolve_required(resolver, name)
        seen.add(name)
        yield resolved

        next_stack = stack + (name,)
        if resolved.super_name is not None:
            yield from visit(resolved.super_name, next_stack)
        for interface_name in resolved.interfaces:
            yield from visit(interface_name, next_stack)

    if include_self:
        yield from visit(root.name, ())
        return

    if root.super_name is not None:
        yield from visit(root.super_name, (root.name,))
    for interface_name in root.interfaces:
        yield from visit(interface_name, (root.name,))


def is_subtype(resolver: ClassResolver, class_name: str, super_name: str) -> bool:
    """Return whether *class_name* is assignable to *super_name* by ancestry."""

    _resolve_required(resolver, super_name)
    return any(resolved.name == super_name for resolved in iter_supertypes(resolver, class_name, include_self=True))


def common_superclass(resolver: ClassResolver, left: str, right: str) -> str:
    """Return the nearest shared superclass for two internal class names.

    This helper follows superclass edges only. Interface relationships therefore
    collapse to ``java/lang/Object`` unless both inputs are the same interface
    name.
    """

    left_chain = {resolved.name for resolved in iter_superclasses(resolver, left, include_self=True)}
    for resolved in iter_superclasses(resolver, right, include_self=True):
        if resolved.name in left_chain:
            return resolved.name
    return JAVA_LANG_OBJECT


def find_overridden_methods(
    resolver: ClassResolver,
    class_name: str,
    method: ResolvedMethod,
) -> tuple[InheritedMethod, ...]:
    """Return inherited declarations overridden by *method* in *class_name*.

    The check is intentionally classfile-oriented:

    - constructors and class initializers never override
    - declaring methods that are ``private`` or ``static`` never override
    - inherited declarations that are ``private``, ``static``, or ``final``
      are excluded
    - package-private declarations only match within the same package
    """

    _resolve_required(resolver, class_name)
    if method.name in ("<init>", "<clinit>"):
        return ()
    if method.access_flags & (MethodAccessFlag.PRIVATE | MethodAccessFlag.STATIC):
        return ()

    matches: list[InheritedMethod] = []
    for supertype in iter_supertypes(resolver, class_name):
        inherited = supertype.find_method(method.name, method.descriptor)
        if inherited is None:
            continue
        if not _can_override(class_name, supertype.name, inherited):
            continue
        matches.append(
            InheritedMethod(
                owner=supertype.name,
                name=inherited.name,
                descriptor=inherited.descriptor,
                access_flags=inherited.access_flags,
            )
        )
    return tuple(matches)


def _resolve_required(resolver: ClassResolver, class_name: str) -> ResolvedClass:
    """Resolve *class_name* or raise ``UnresolvedClassError``."""

    resolved = resolver.resolve_class(class_name)
    if resolved is not None:
        return resolved
    if class_name == JAVA_LANG_OBJECT:
        return _IMPLICIT_OBJECT
    raise UnresolvedClassError(class_name)


def _resolve_class_name(cp: ConstantPoolBuilder, index: int) -> str:
    """Resolve a ``CONSTANT_Class`` index to its internal name."""

    entry = cp.get(index)
    if not isinstance(entry, ClassInfo):
        raise ValueError(f"CP index {index} is not a CONSTANT_Class: {type(entry).__name__}")
    return cp.resolve_utf8(entry.name_index)


def _package_name(class_name: str) -> str:
    """Return the internal package prefix for *class_name*."""

    package, _, _ = class_name.rpartition("/")
    return package


def _can_override(
    declaring_owner: str,
    inherited_owner: str,
    inherited_method: ResolvedMethod,
) -> bool:
    """Return whether *inherited_method* is overridable from *declaring_owner*."""

    if inherited_method.name in ("<init>", "<clinit>"):
        return False
    if inherited_method.access_flags & _NON_OVERRIDABLE_METHOD_FLAGS:
        return False
    if inherited_method.access_flags & (MethodAccessFlag.PUBLIC | MethodAccessFlag.PROTECTED):
        return True
    return _package_name(declaring_owner) == _package_name(inherited_owner)


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
