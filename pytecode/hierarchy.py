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
        """Initialize with the unresolved class name.

        Args:
            class_name: JVM internal name that could not be resolved.
        """
        self.class_name = class_name
        super().__init__(f"Could not resolve class {class_name!r}")


class HierarchyCycleError(HierarchyError):
    """Raised when a malformed class graph contains an ancestry cycle."""

    def __init__(self, cycle: tuple[str, ...]) -> None:
        """Initialize with the detected cycle.

        Args:
            cycle: Sequence of internal names forming the cycle.
        """
        self.cycle = cycle
        joined = " -> ".join(cycle)
        super().__init__(f"Hierarchy cycle detected: {joined}")


@dataclass(frozen=True, slots=True)
class ResolvedMethod:
    """A method declaration resolved out of raw class metadata.

    Attributes:
        name: Method name (e.g. ``<init>`` or ``toString``).
        descriptor: JVM method descriptor (e.g. ``(I)V``).
        access_flags: Bitfield of method access and property flags.
    """

    name: str
    descriptor: str
    access_flags: MethodAccessFlag


@dataclass(frozen=True, slots=True)
class ResolvedClass:
    """Resolved hierarchy snapshot for one class or interface.

    Attributes:
        name: JVM internal name (e.g. ``java/lang/String``).
        super_name: Internal name of the direct superclass, or ``None`` for ``java/lang/Object``.
        interfaces: Internal names of directly implemented interfaces.
        access_flags: Bitfield of class access and property flags.
        methods: Declared methods extracted from the class.
    """

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
        """Return the declared method with the given signature, if present.

        Args:
            name: Method name to match.
            descriptor: JVM method descriptor to match.

        Returns:
            The matching method, or ``None`` if not declared in this class.
        """

        for method in self.methods:
            if method.name == name and method.descriptor == descriptor:
                return method
        return None

    @classmethod
    def from_classfile(cls, classfile: ClassFile) -> ResolvedClass:
        """Build a resolved hierarchy snapshot from a parsed ``ClassFile``.

        Args:
            classfile: A parsed JVM class file.

        Returns:
            A new ``ResolvedClass`` populated from the class file metadata.
        """

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
        """Build a resolved hierarchy snapshot from a ``ClassModel``.

        Args:
            model: A high-level class model.

        Returns:
            A new ``ResolvedClass`` populated from the model.
        """

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
    """A matching inherited method declaration found in a supertype.

    Attributes:
        owner: Internal name of the class that declares the method.
        name: The method name.
        descriptor: The JVM method descriptor.
        access_flags: Bitfield of method access and property flags.
    """

    owner: str
    name: str
    descriptor: str
    access_flags: MethodAccessFlag


class ClassResolver(Protocol):
    """Protocol for supplying resolved class metadata by internal name."""

    def resolve_class(self, class_name: str) -> ResolvedClass | None:
        """Return resolved metadata for *class_name*, or ``None`` if unavailable.

        Args:
            class_name: JVM internal class name to resolve.

        Returns:
            Resolved class metadata, or ``None`` if the class cannot be found.
        """


class MappingClassResolver:
    """Simple in-memory ``ClassResolver`` backed by resolved class snapshots."""

    __slots__ = ("_classes",)

    def __init__(self, classes: Iterable[ResolvedClass]) -> None:
        """Initialize from an iterable of resolved class snapshots.

        Args:
            classes: Resolved class entries to index by internal name.

        Raises:
            ValueError: If duplicate class names are found.
        """
        mapping: dict[str, ResolvedClass] = {}
        for resolved in classes:
            if resolved.name in mapping:
                raise ValueError(f"Duplicate resolved class {resolved.name!r}")
            mapping[resolved.name] = resolved
        self._classes = mapping

    def resolve_class(self, class_name: str) -> ResolvedClass | None:
        """Resolve *class_name* from the in-memory mapping.

        Args:
            class_name: JVM internal class name to look up.

        Returns:
            Resolved class metadata, or ``None`` if not in the mapping.
        """

        return self._classes.get(class_name)

    @classmethod
    def from_classfiles(cls, classfiles: Iterable[ClassFile]) -> MappingClassResolver:
        """Build a resolver from parsed ``ClassFile`` objects.

        Args:
            classfiles: Parsed JVM class files to index.

        Returns:
            A new resolver backed by the given class files.
        """

        return cls(ResolvedClass.from_classfile(classfile) for classfile in classfiles)

    @classmethod
    def from_models(cls, models: Iterable[ClassModel]) -> MappingClassResolver:
        """Build a resolver from ``ClassModel`` objects.

        Args:
            models: High-level class models to index.

        Returns:
            A new resolver backed by the given models.
        """

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

    The returned chain always terminates at ``java/lang/Object``.  If the
    resolver does not explicitly provide that root class, an implicit stub is
    used so ordinary user-defined class hierarchies can still terminate cleanly.

    Args:
        resolver: Provider of class metadata.
        class_name: JVM internal name of the starting class.
        include_self: If ``True``, include the starting class itself.

    Yields:
        Each superclass from the immediate parent up to ``java/lang/Object``.

    Raises:
        UnresolvedClassError: If a class in the chain cannot be resolved.
        HierarchyCycleError: If a cycle is detected in the superclass chain.
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

    Args:
        resolver: Provider of class metadata.
        class_name: JVM internal name of the starting class.
        include_self: If ``True``, include the starting class itself.

    Yields:
        Each unique supertype in depth-first order, superclasses before interfaces.

    Raises:
        UnresolvedClassError: If a class in the graph cannot be resolved.
        HierarchyCycleError: If a cycle is detected in the type graph.
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
    """Return whether *class_name* is assignable to *super_name* by ancestry.

    Args:
        resolver: Provider of class metadata.
        class_name: JVM internal name of the candidate subtype.
        super_name: JVM internal name of the candidate supertype.

    Returns:
        ``True`` if *class_name* equals or extends/implements *super_name*.

    Raises:
        UnresolvedClassError: If any class in the hierarchy cannot be resolved.
    """

    _resolve_required(resolver, super_name)
    return any(resolved.name == super_name for resolved in iter_supertypes(resolver, class_name, include_self=True))


def common_superclass(resolver: ClassResolver, left: str, right: str) -> str:
    """Return the nearest shared superclass for two internal class names.

    Follows superclass edges only (cf. JVM spec §4.10.1.2 type merging).
    Interface relationships collapse to ``java/lang/Object`` unless both
    inputs share the same class name.

    Args:
        resolver: Provider of class metadata.
        left: JVM internal name of the first class.
        right: JVM internal name of the second class.

    Returns:
        Internal name of the nearest common superclass.

    Raises:
        UnresolvedClassError: If any class in either chain cannot be resolved.
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

    The check is intentionally classfile-oriented (cf. JVM spec §5.4.5):

    - Constructors and class initializers never override.
    - Declaring methods that are ``private`` or ``static`` never override.
    - Inherited declarations that are ``private``, ``static``, or ``final``
      are excluded.
    - Package-private declarations only match within the same runtime package.

    Args:
        resolver: Provider of class metadata.
        class_name: JVM internal name of the declaring class.
        method: The method whose overridden ancestors to find.

    Returns:
        Matching inherited method declarations, possibly empty.

    Raises:
        UnresolvedClassError: If any class in the hierarchy cannot be resolved.
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
    """Resolve *class_name* or raise ``UnresolvedClassError``.

    Falls back to an implicit ``java/lang/Object`` stub when the resolver
    does not provide it.

    Args:
        resolver: Provider of class metadata.
        class_name: JVM internal name to resolve.

    Returns:
        The resolved class metadata.

    Raises:
        UnresolvedClassError: If the class cannot be resolved and is not ``java/lang/Object``.
    """

    resolved = resolver.resolve_class(class_name)
    if resolved is not None:
        return resolved
    if class_name == JAVA_LANG_OBJECT:
        return _IMPLICIT_OBJECT
    raise UnresolvedClassError(class_name)


def _resolve_class_name(cp: ConstantPoolBuilder, index: int) -> str:
    """Resolve a ``CONSTANT_Class`` index to its internal name.

    Args:
        cp: Constant pool to read from.
        index: Constant pool index pointing to a ``CONSTANT_Class_info`` entry (JVM spec §4.4.1).

    Returns:
        The UTF-8 internal name referenced by the class-info entry.

    Raises:
        ValueError: If the entry at *index* is not a ``ClassInfo``.
    """

    entry = cp.peek(index)
    if not isinstance(entry, ClassInfo):
        raise ValueError(f"CP index {index} is not a CONSTANT_Class: {type(entry).__name__}")
    return cp.resolve_utf8(entry.name_index)


def _package_name(class_name: str) -> str:
    """Return the internal package prefix for *class_name*.

    Args:
        class_name: JVM internal name (e.g. ``java/lang/String``).

    Returns:
        The package portion (e.g. ``java/lang``), or an empty string for the default package.
    """

    package, _, _ = class_name.rpartition("/")
    return package


def _can_override(
    declaring_owner: str,
    inherited_owner: str,
    inherited_method: ResolvedMethod,
) -> bool:
    """Return whether *inherited_method* is overridable from *declaring_owner*.

    Applies JVM override rules (JVM spec §5.4.5): constructors, ``private``,
    ``static``, and ``final`` methods cannot be overridden, and package-private
    access requires the same runtime package.

    Args:
        declaring_owner: Internal name of the overriding class.
        inherited_owner: Internal name of the class declaring the inherited method.
        inherited_method: The candidate inherited method declaration.

    Returns:
        ``True`` if the inherited method can be overridden from the declaring class.
    """

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
