"""JVM type descriptor and generic signature parsing utilities.

Provides structured representations for JVM field descriptors (§4.3.2),
method descriptors (§4.3.3), and generic signatures (§4.7.9.1), along
with parsing, construction, validation, and slot-counting helpers.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Literal, Never

# ---------------------------------------------------------------------------
# Descriptor data model (JVM spec §4.3.2, §4.3.3)
# ---------------------------------------------------------------------------


class BaseType(Enum):
    """JVM primitive types, each carrying its single-character descriptor."""

    BOOLEAN = "Z"
    BYTE = "B"
    CHAR = "C"
    SHORT = "S"
    INT = "I"
    LONG = "J"
    FLOAT = "F"
    DOUBLE = "D"


_BASE_TYPE_BY_CHAR: dict[str, BaseType] = {t.value: t for t in BaseType}

_TWO_SLOT_TYPES = frozenset({BaseType.LONG, BaseType.DOUBLE})


class VoidType(Enum):
    """Sentinel for the void return type (``V``). Only valid in return position."""

    VOID = "V"


VOID = VoidType.VOID


@dataclass(frozen=True, slots=True)
class ObjectType:
    """Reference to a class or interface in internal form (e.g. ``java/lang/String``)."""

    class_name: str


@dataclass(frozen=True, slots=True)
class ArrayType:
    """Array type whose component may be any field descriptor, including another array."""

    component_type: FieldDescriptor


@dataclass(frozen=True, slots=True)
class MethodDescriptor:
    """Parsed method descriptor: parameter types and return type."""

    parameter_types: tuple[FieldDescriptor, ...]
    return_type: ReturnType


type FieldDescriptor = BaseType | ObjectType | ArrayType
type ReturnType = FieldDescriptor | VoidType


# ---------------------------------------------------------------------------
# Generic signature data model (JVM spec §4.7.9.1)
# ---------------------------------------------------------------------------


@dataclass(frozen=True, slots=True)
class TypeVariable:
    """Type variable reference, e.g. ``TT;`` → ``TypeVariable("T")``."""

    name: str


@dataclass(frozen=True, slots=True)
class TypeArgument:
    """A single type argument inside angle brackets.

    *wildcard* is ``"+"`` (extends), ``"-"`` (super), or ``None`` (exact).
    *signature* is ``None`` only for the unbounded wildcard ``*``.
    """

    wildcard: Literal["+", "-"] | None
    signature: ReferenceTypeSignature | None


@dataclass(frozen=True, slots=True)
class InnerClassType:
    """An inner-class suffix inside a ``ClassTypeSignature`` (after ``.``)."""

    name: str
    type_arguments: tuple[TypeArgument, ...]


@dataclass(frozen=True, slots=True)
class ClassTypeSignature:
    """Fully-qualified generic class type, e.g.
    ``Ljava/util/Map<TK;TV;>.Entry<TK;TV;>;``
    """

    package: str
    name: str
    type_arguments: tuple[TypeArgument, ...]
    inner: tuple[InnerClassType, ...]


@dataclass(frozen=True, slots=True)
class ArrayTypeSignature:
    """Generic array type signature, e.g. ``[TT;``."""

    component: JavaTypeSignature


type ReferenceTypeSignature = ClassTypeSignature | TypeVariable | ArrayTypeSignature
type JavaTypeSignature = BaseType | ReferenceTypeSignature


@dataclass(frozen=True, slots=True)
class TypeParameter:
    """A formal type parameter declaration, e.g. ``T:Ljava/lang/Object;``."""

    name: str
    class_bound: ReferenceTypeSignature | None
    interface_bounds: tuple[ReferenceTypeSignature, ...]


@dataclass(frozen=True, slots=True)
class ClassSignature:
    """Parsed generic class signature (``Signature`` attribute on a class)."""

    type_parameters: tuple[TypeParameter, ...]
    super_class: ClassTypeSignature
    super_interfaces: tuple[ClassTypeSignature, ...]


@dataclass(frozen=True, slots=True)
class MethodSignature:
    """Parsed generic method signature (``Signature`` attribute on a method)."""

    type_parameters: tuple[TypeParameter, ...]
    parameter_types: tuple[JavaTypeSignature, ...]
    return_type: JavaTypeSignature | VoidType
    throws: tuple[ClassTypeSignature | TypeVariable, ...]


type FieldSignature = ReferenceTypeSignature


# ---------------------------------------------------------------------------
# Internal parser helpers
# ---------------------------------------------------------------------------


class _Reader:
    """Minimal cursor over a string for recursive-descent parsing."""

    __slots__ = ("_s", "_pos")

    def __init__(self, s: str) -> None:
        self._s = s
        self._pos = 0

    @property
    def pos(self) -> int:
        return self._pos

    def at_end(self) -> bool:
        return self._pos >= len(self._s)

    def peek(self) -> str:
        if self._pos >= len(self._s):
            self._fail("unexpected end of string")
        return self._s[self._pos]

    def advance(self) -> str:
        ch = self.peek()
        self._pos += 1
        return ch

    def expect(self, ch: str) -> None:
        actual = self.advance()
        if actual != ch:
            self._fail(f"expected '{ch}', got '{actual}'", self._pos - 1)

    def remaining(self) -> str:
        return self._s[self._pos :]

    def _fail(self, msg: str, pos: int | None = None) -> Never:
        p = pos if pos is not None else self._pos
        raise ValueError(f"{msg} at position {p} in {self._s!r}")


def _read_field_descriptor(r: _Reader) -> FieldDescriptor:
    ch = r.peek()
    bt = _BASE_TYPE_BY_CHAR.get(ch)
    if bt is not None:
        r.advance()
        return bt
    if ch == "L":
        return _read_object_type(r)
    if ch == "[":
        r.advance()
        return ArrayType(_read_field_descriptor(r))
    r._fail(f"invalid descriptor character '{ch}'")


def _read_object_type(r: _Reader) -> ObjectType:
    r.expect("L")
    start = r.pos
    while r.peek() != ";":
        r.advance()
    class_name = r._s[start : r.pos]
    r.expect(";")
    if not class_name:
        r._fail("empty class name in object type", start)
    return ObjectType(class_name)


def _read_return_type(r: _Reader) -> ReturnType:
    if r.peek() == "V":
        r.advance()
        return VOID
    return _read_field_descriptor(r)


# ---------------------------------------------------------------------------
# Generic signature parser helpers
# ---------------------------------------------------------------------------


def _read_reference_type_signature(r: _Reader) -> ReferenceTypeSignature:
    ch = r.peek()
    if ch == "L":
        return _read_class_type_signature(r)
    if ch == "T":
        return _read_type_variable(r)
    if ch == "[":
        return _read_array_type_signature(r)
    r._fail(f"expected reference type signature, got '{ch}'")


def _read_java_type_signature(r: _Reader) -> JavaTypeSignature:
    ch = r.peek()
    bt = _BASE_TYPE_BY_CHAR.get(ch)
    if bt is not None:
        r.advance()
        return bt
    return _read_reference_type_signature(r)


def _read_type_variable(r: _Reader) -> TypeVariable:
    r.expect("T")
    start = r.pos
    while r.peek() != ";":
        r.advance()
    name = r._s[start : r.pos]
    r.expect(";")
    if not name:
        r._fail("empty type variable name", start)
    return TypeVariable(name)


def _read_type_arguments(r: _Reader) -> tuple[TypeArgument, ...]:
    r.expect("<")
    args: list[TypeArgument] = []
    while r.peek() != ">":
        args.append(_read_type_argument(r))
    r.expect(">")
    return tuple(args)


def _read_type_argument(r: _Reader) -> TypeArgument:
    ch = r.peek()
    if ch == "*":
        r.advance()
        return TypeArgument(wildcard=None, signature=None)
    wildcard: Literal["+", "-"] | None = None
    if ch in ("+", "-"):
        wildcard = ch  # pyright: ignore[reportAssignmentType]
        r.advance()
    sig = _read_reference_type_signature(r)
    return TypeArgument(wildcard=wildcard, signature=sig)


def _read_class_type_signature(r: _Reader) -> ClassTypeSignature:
    r.expect("L")

    # Collect the full identifier path (package + simple name) first.
    ident_chars: list[str] = []
    while r.peek() not in ("<", ".", ";"):
        ident_chars.append(r.advance())

    full_ident = "".join(ident_chars)

    # Split into package and simple name at the last '/'.
    last_slash = full_ident.rfind("/")
    if last_slash == -1:
        package = ""
        name = full_ident
    else:
        package = full_ident[: last_slash + 1]
        name = full_ident[last_slash + 1 :]

    # Optional type arguments.
    type_arguments: tuple[TypeArgument, ...] = ()
    if not r.at_end() and r.peek() == "<":
        type_arguments = _read_type_arguments(r)

    # Inner class suffixes.
    inner: list[InnerClassType] = []
    while not r.at_end() and r.peek() == ".":
        r.advance()  # consume '.'
        inner_name_chars: list[str] = []
        while r.peek() not in ("<", ".", ";"):
            inner_name_chars.append(r.advance())
        inner_name = "".join(inner_name_chars)
        inner_type_args: tuple[TypeArgument, ...] = ()
        if not r.at_end() and r.peek() == "<":
            inner_type_args = _read_type_arguments(r)
        inner.append(InnerClassType(inner_name, inner_type_args))

    r.expect(";")
    return ClassTypeSignature(package, name, type_arguments, tuple(inner))


def _read_array_type_signature(r: _Reader) -> ArrayTypeSignature:
    r.expect("[")
    component = _read_java_type_signature(r)
    return ArrayTypeSignature(component)


def _read_type_parameters(r: _Reader) -> tuple[TypeParameter, ...]:
    r.expect("<")
    params: list[TypeParameter] = []
    while r.peek() != ">":
        params.append(_read_type_parameter(r))
    r.expect(">")
    return tuple(params)


def _read_type_parameter(r: _Reader) -> TypeParameter:
    # Identifier ':' [ClassBound] {':' InterfaceBound}
    start = r.pos
    name_chars: list[str] = []
    while r.peek() != ":":
        name_chars.append(r.advance())
    name = "".join(name_chars)
    if not name:
        r._fail("empty type parameter name", start)

    r.expect(":")

    # Class bound — may be empty (just ':' followed by another ':' or next param or '>').
    class_bound: ReferenceTypeSignature | None = None
    if not r.at_end() and r.peek() not in (":", ">"):
        class_bound = _read_reference_type_signature(r)

    # Interface bounds (each prefixed by ':').
    interface_bounds: list[ReferenceTypeSignature] = []
    while not r.at_end() and r.peek() == ":":
        r.advance()  # consume ':'
        interface_bounds.append(_read_reference_type_signature(r))

    return TypeParameter(name, class_bound, tuple(interface_bounds))


def _read_return_type_signature(r: _Reader) -> JavaTypeSignature | VoidType:
    if r.peek() == "V":
        r.advance()
        return VOID
    return _read_java_type_signature(r)


def _read_throws_signature(r: _Reader) -> ClassTypeSignature | TypeVariable:
    r.expect("^")
    ch = r.peek()
    if ch == "L":
        return _read_class_type_signature(r)
    if ch == "T":
        return _read_type_variable(r)
    r._fail(f"expected class type or type variable after '^', got '{ch}'")


# ---------------------------------------------------------------------------
# Public parsing API
# ---------------------------------------------------------------------------


def parse_field_descriptor(s: str) -> FieldDescriptor:
    """Parse a JVM field descriptor string into a structured type.

    >>> parse_field_descriptor("Ljava/lang/String;")
    ObjectType(class_name='java/lang/String')
    >>> parse_field_descriptor("[[I")
    ArrayType(component_type=ArrayType(component_type=<BaseType.INT: 'I'>))
    """
    r = _Reader(s)
    result = _read_field_descriptor(r)
    if not r.at_end():
        r._fail("trailing characters after field descriptor")
    return result


def parse_method_descriptor(s: str) -> MethodDescriptor:
    """Parse a JVM method descriptor string into parameter and return types.

    >>> parse_method_descriptor("(IDLjava/lang/Thread;)Ljava/lang/Object;")
    MethodDescriptor(parameter_types=(...), return_type=ObjectType(...))
    """
    r = _Reader(s)
    r.expect("(")
    params: list[FieldDescriptor] = []
    while r.peek() != ")":
        params.append(_read_field_descriptor(r))
    r.expect(")")
    ret = _read_return_type(r)
    if not r.at_end():
        r._fail("trailing characters after method descriptor")
    return MethodDescriptor(tuple(params), ret)


def parse_class_signature(s: str) -> ClassSignature:
    """Parse a generic class signature (``Signature`` attribute on a class).

    >>> parse_class_signature("<T:Ljava/lang/Object;>Ljava/lang/Object;")
    ClassSignature(...)
    """
    r = _Reader(s)
    type_params: tuple[TypeParameter, ...] = ()
    if r.peek() == "<":
        type_params = _read_type_parameters(r)
    super_class = _read_class_type_signature(r)
    super_interfaces: list[ClassTypeSignature] = []
    while not r.at_end():
        super_interfaces.append(_read_class_type_signature(r))
    return ClassSignature(type_params, super_class, tuple(super_interfaces))


def parse_method_signature(s: str) -> MethodSignature:
    """Parse a generic method signature (``Signature`` attribute on a method).

    >>> parse_method_signature("<T:Ljava/lang/Object;>(TT;)TT;")
    MethodSignature(...)
    """
    r = _Reader(s)
    type_params: tuple[TypeParameter, ...] = ()
    if r.peek() == "<":
        type_params = _read_type_parameters(r)
    r.expect("(")
    param_types: list[JavaTypeSignature] = []
    while r.peek() != ")":
        param_types.append(_read_java_type_signature(r))
    r.expect(")")
    ret = _read_return_type_signature(r)
    throws: list[ClassTypeSignature | TypeVariable] = []
    while not r.at_end():
        throws.append(_read_throws_signature(r))
    return MethodSignature(type_params, tuple(param_types), ret, tuple(throws))


def parse_field_signature(s: str) -> FieldSignature:
    """Parse a generic field type signature (``Signature`` attribute on a field).

    >>> parse_field_signature("Ljava/util/List<Ljava/lang/String;>;")
    ClassTypeSignature(...)
    """
    r = _Reader(s)
    result = _read_reference_type_signature(r)
    if not r.at_end():
        r._fail("trailing characters after field signature")
    return result


# ---------------------------------------------------------------------------
# Descriptor string construction (round-trip)
# ---------------------------------------------------------------------------


def _field_descriptor_to_str(t: FieldDescriptor) -> str:
    if isinstance(t, BaseType):
        return t.value
    if isinstance(t, ObjectType):
        return f"L{t.class_name};"
    if isinstance(t, ArrayType):  # pyright: ignore[reportUnnecessaryIsInstance]
        return f"[{_field_descriptor_to_str(t.component_type)}"
    raise TypeError(f"unexpected descriptor type: {type(t)}")  # pyright: ignore[reportUnreachable]


def to_descriptor(t: FieldDescriptor | MethodDescriptor) -> str:
    """Convert a structured descriptor back into its JVM string form.

    >>> to_descriptor(BaseType.INT)
    'I'
    >>> to_descriptor(ObjectType("java/lang/String"))
    'Ljava/lang/String;'
    >>> to_descriptor(MethodDescriptor((BaseType.INT,), VOID))
    '(I)V'
    """
    if isinstance(t, MethodDescriptor):
        params = "".join(_field_descriptor_to_str(p) for p in t.parameter_types)
        ret = "V" if isinstance(t.return_type, VoidType) else _field_descriptor_to_str(t.return_type)
        return f"({params}){ret}"
    return _field_descriptor_to_str(t)


# ---------------------------------------------------------------------------
# Slot helpers
# ---------------------------------------------------------------------------


def slot_size(t: FieldDescriptor) -> int:
    """Return the number of JVM local/stack slots a type occupies (2 for long/double, 1 otherwise)."""
    if isinstance(t, BaseType) and t in _TWO_SLOT_TYPES:
        return 2
    return 1


def parameter_slot_count(d: MethodDescriptor) -> int:
    """Return the total number of JVM parameter slots for a method descriptor.

    Does **not** include the implicit ``this`` slot for instance methods.
    """
    return sum(slot_size(p) for p in d.parameter_types)


# ---------------------------------------------------------------------------
# Validation helpers
# ---------------------------------------------------------------------------


def is_valid_field_descriptor(s: str) -> bool:
    """Return ``True`` if *s* is a well-formed JVM field descriptor."""
    try:
        parse_field_descriptor(s)
        return True
    except ValueError:
        return False


def is_valid_method_descriptor(s: str) -> bool:
    """Return ``True`` if *s* is a well-formed JVM method descriptor."""
    try:
        parse_method_descriptor(s)
        return True
    except ValueError:
        return False
