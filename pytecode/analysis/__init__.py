"""Control-flow graph construction and stack/local simulation.

Provides analysis infrastructure for JVM bytecode in the editing model:

- **Verification type system** (``VType``) mirroring JVM spec §4.10.1.2
- **Control-flow graph** construction from ``CodeModel`` instructions
- **Stack and local variable simulation** with forward dataflow analysis
- **Frame recomputation** for ``max_stack``, ``max_locals``, and ``StackMapTable``
- **Type merging** at control-flow join points using the class hierarchy

All result types are frozen dataclasses — safe to share across threads.
The module operates on the symbolic editing model (``CodeModel``) so it
benefits from label-based branch targets, symbolic operands, and
exception handlers already bound to labels.

References:
    JVM spec §4.7.4 — StackMapTable attribute format.
    JVM spec §4.10.1 — Verification by type checking.
    JVM spec §4.10.1.2 — Verification type system and type merging rules.
    JVM spec §6.5 — Individual opcode definitions (stack effects).
"""

from __future__ import annotations

from collections import deque
from collections.abc import Sequence
from dataclasses import dataclass
from typing import TYPE_CHECKING

from ..classfile.attributes import (
    AppendFrameInfo,
    ChopFrameInfo,
    DoubleVariableInfo,
    FloatVariableInfo,
    FullFrameInfo,
    IntegerVariableInfo,
    LongVariableInfo,
    NullVariableInfo,
    ObjectVariableInfo,
    SameFrameExtendedInfo,
    SameFrameInfo,
    SameLocals1StackItemFrameExtendedInfo,
    SameLocals1StackItemFrameInfo,
    StackMapFrameInfo,
    StackMapTableAttr,
    TopVariableInfo,
    UninitializedThisVariableInfo,
    UninitializedVariableInfo,
    VerificationTypeInfo,
)
from ..classfile.constants import VerificationType
from ..classfile.descriptors import (
    ArrayType as DescArrayType,
)
from ..classfile.descriptors import (
    BaseType,
    FieldDescriptor,
    ObjectType,
    VoidType,
    parse_field_descriptor,
    parse_method_descriptor,
)
from ..classfile.instructions import (
    ArrayType as InsnArrayType,
)
from ..classfile.instructions import (
    InsnInfo,
    InsnInfoType,
)
from ..edit.labels import (
    BranchInsn,
    CodeItem,
    ExceptionHandler,
    Label,
    LookupSwitchInsn,
    TableSwitchInsn,
)
from ..edit.operands import (
    FieldInsn,
    IIncInsn,
    InterfaceMethodInsn,
    InvokeDynamicInsn,
    LdcClass,
    LdcDouble,
    LdcFloat,
    LdcInsn,
    LdcInt,
    LdcLong,
    LdcMethodHandle,
    LdcMethodType,
    LdcString,
    MethodInsn,
    MultiANewArrayInsn,
    TypeInsn,
    VarInsn,
)
from .hierarchy import JAVA_LANG_OBJECT, common_superclass

if TYPE_CHECKING:
    from ..edit.constant_pool_builder import ConstantPoolBuilder
    from ..edit.model import CodeModel, MethodModel
    from .hierarchy import ClassResolver

# ===================================================================
# Analysis errors
# ===================================================================


class AnalysisError(Exception):
    """Base class for control-flow and simulation errors."""


class StackUnderflowError(AnalysisError):
    """Popped from empty or insufficiently deep stack."""


class InvalidLocalError(AnalysisError):
    """Read from an uninitialized or out-of-bounds local variable slot."""


class TypeMergeError(AnalysisError):
    """Incompatible types at a control-flow join point."""


# ===================================================================
# Verification type system  (JVM spec §4.10.1.2)
# ===================================================================


@dataclass(frozen=True, slots=True)
class VTop:
    """Top type — undefined or second slot of a category-2 value."""


@dataclass(frozen=True, slots=True)
class VInteger:
    """Verification type for int, short, byte, char, boolean."""


@dataclass(frozen=True, slots=True)
class VFloat:
    """Verification type for float."""


@dataclass(frozen=True, slots=True)
class VLong:
    """Verification type for long (occupies 2 slots; second is TOP)."""


@dataclass(frozen=True, slots=True)
class VDouble:
    """Verification type for double (occupies 2 slots; second is TOP)."""


@dataclass(frozen=True, slots=True)
class VNull:
    """Verification type for null — assignable to any reference type."""


@dataclass(frozen=True, slots=True)
class VObject:
    """Verification type for a reference to a class, interface, or array.

    Attributes:
        class_name: JVM internal name (e.g. ``"java/lang/String"`` or ``"[I"``).
    """

    class_name: str


@dataclass(frozen=True, slots=True)
class VUninitializedThis:
    """Verification type for ``this`` before the super/this ``<init>`` call."""


@dataclass(frozen=True, slots=True)
class VUninitialized:
    """Verification type for an object created by NEW before ``<init>``.

    Analysis inserts synthetic labels for unlabeled ``NEW`` instructions so
    edited code can still refer to allocation sites precisely.

    Attributes:
        new_label: Label identifying the NEW instruction that created this value.
    """

    new_label: Label


type VType = VTop | VInteger | VFloat | VLong | VDouble | VNull | VObject | VUninitializedThis | VUninitialized


# --- Singletons for stateless types ---

_TOP = VTop()
_INTEGER = VInteger()
_FLOAT = VFloat()
_LONG = VLong()
_DOUBLE = VDouble()
_NULL = VNull()
_UNINIT_THIS = VUninitializedThis()
_OBJECT_OBJECT = VObject(JAVA_LANG_OBJECT)
_OBJECT_STRING = VObject("java/lang/String")
_OBJECT_CLASS = VObject("java/lang/Class")
_OBJECT_METHOD_TYPE = VObject("java/lang/invoke/MethodType")
_OBJECT_METHOD_HANDLE = VObject("java/lang/invoke/MethodHandle")
_OBJECT_THROWABLE = VObject("java/lang/Throwable")

# ---------------------------------------------------------------------------
# VType helpers
# ---------------------------------------------------------------------------

# Map from NEWARRAY atype codes to the resulting array element descriptor.
_NEWARRAY_TYPE_MAP: dict[InsnArrayType, str] = {
    InsnArrayType.BOOLEAN: "[Z",
    InsnArrayType.CHAR: "[C",
    InsnArrayType.FLOAT: "[F",
    InsnArrayType.DOUBLE: "[D",
    InsnArrayType.BYTE: "[B",
    InsnArrayType.SHORT: "[S",
    InsnArrayType.INT: "[I",
    InsnArrayType.LONG: "[J",
}


def vtype_from_descriptor(fd: FieldDescriptor) -> VType:
    """Convert a parsed field descriptor to a verification type."""
    if isinstance(fd, BaseType):
        if fd in {BaseType.INT, BaseType.SHORT, BaseType.BYTE, BaseType.CHAR, BaseType.BOOLEAN}:
            return _INTEGER
        if fd is BaseType.FLOAT:
            return _FLOAT
        if fd is BaseType.LONG:
            return _LONG
        if fd is BaseType.DOUBLE:
            return _DOUBLE
    if isinstance(fd, ObjectType):
        return VObject(fd.class_name)
    if isinstance(fd, DescArrayType):
        return VObject(_descriptor_to_internal(fd))
    raise ValueError(f"Unexpected descriptor type: {fd!r}")  # pragma: no cover


def vtype_from_field_descriptor_str(desc: str) -> VType:
    """Convert a raw field descriptor string to a verification type."""
    return vtype_from_descriptor(parse_field_descriptor(desc))


def _descriptor_to_internal(fd: FieldDescriptor) -> str:
    """Convert a FieldDescriptor to the JVM internal form used in VObject.class_name."""
    if isinstance(fd, BaseType):
        return fd.value
    if isinstance(fd, ObjectType):
        return fd.class_name
    # fd must be DescArrayType at this point.
    return "[" + _descriptor_to_component_string(fd.component_type)


def _descriptor_to_component_string(fd: FieldDescriptor) -> str:
    """Return the JVM descriptor string for a component type."""
    if isinstance(fd, BaseType):
        return fd.value
    if isinstance(fd, ObjectType):
        return f"L{fd.class_name};"
    # fd must be DescArrayType at this point.
    return "[" + _descriptor_to_component_string(fd.component_type)


def is_category2(vt: VType) -> bool:
    """Return ``True`` for long and double (category-2 computational types)."""
    return isinstance(vt, VLong | VDouble)


def is_reference(vt: VType) -> bool:
    """Return ``True`` for reference verification types (null, object, uninitialized)."""
    return isinstance(vt, VNull | VObject | VUninitializedThis | VUninitialized)


def merge_vtypes(a: VType, b: VType, resolver: ClassResolver | None = None) -> VType:
    """Merge two verification types at a control-flow join point.

    Follows JVM spec §4.10.1.2 type merging rules:

    - Identical types → same type
    - Two ``VObject`` → ``VObject(common_superclass(...))``
    - ``VNull`` + reference → the reference type
    - Incompatible types → ``VTop``

    Args:
        a: First verification type.
        b: Second verification type.
        resolver: Optional class hierarchy resolver for precise object merging.

    Returns:
        The merged verification type.
    """
    if a == b:
        return a

    # VNull merges with any reference to yield the reference type.
    if isinstance(a, VNull) and is_reference(b):
        return b
    if isinstance(b, VNull) and is_reference(a):
        return a

    # Two VObject references → common superclass.
    if isinstance(a, VObject) and isinstance(b, VObject):
        if resolver is not None:
            try:
                return VObject(common_superclass(resolver, a.class_name, b.class_name))
            except Exception:
                return _OBJECT_OBJECT
        return _OBJECT_OBJECT

    # Everything else is incompatible.
    return _TOP


# ===================================================================
# Frame state
# ===================================================================


@dataclass(frozen=True, slots=True)
class FrameState:
    """Immutable snapshot of the operand stack and local variable slots.

    Category-2 values (long, double) occupy two consecutive slots — the
    value itself followed by ``VTop``.

    Attributes:
        stack: Operand stack, ordered bottom-to-top.
        locals: Local variable slots indexed by slot number; unset slots
            are ``VTop``.
    """

    stack: tuple[VType, ...]
    locals: tuple[VType, ...]

    # -- Stack operations --

    def push(self, *types: VType) -> FrameState:
        """Push one or more types onto the stack (category-2 aware)."""
        new_stack = list(self.stack)
        for vt in types:
            new_stack.append(vt)
            if is_category2(vt):
                new_stack.append(_TOP)
        return FrameState(tuple(new_stack), self.locals)

    def pop(self, n: int = 1) -> tuple[FrameState, tuple[VType, ...]]:
        """Pop *n* stack slots and return ``(new_state, popped_values)``.

        Args:
            n: Number of stack slots to pop.

        Returns:
            A ``(new_state, popped_values)`` tuple where *popped_values* is
            ordered from topmost to deepest.

        Raises:
            StackUnderflowError: If the stack has fewer than *n* slots.
        """
        if len(self.stack) < n:
            raise StackUnderflowError(f"Need {n} slots but stack has {len(self.stack)}")
        if n == 0:
            return self, ()
        remaining = self.stack[:-n]
        popped = tuple(reversed(self.stack[-n:]))
        return FrameState(remaining, self.locals), popped

    def peek(self, depth: int = 0) -> VType:
        """Return the type at *depth* slots from the top (0 = top).

        Raises:
            StackUnderflowError: If *depth* exceeds the current stack size.
        """
        idx = len(self.stack) - 1 - depth
        if idx < 0:
            raise StackUnderflowError(f"Cannot peek at depth {depth} with stack size {len(self.stack)}")
        return self.stack[idx]

    # -- Local operations --

    def set_local(self, index: int, vtype: VType) -> FrameState:
        """Set a local variable slot (category-2 aware)."""
        needed = index + (2 if is_category2(vtype) else 1)
        locals_list = list(self.locals)
        while len(locals_list) < needed:
            locals_list.append(_TOP)
        locals_list[index] = vtype
        if is_category2(vtype):
            locals_list[index + 1] = _TOP
        return FrameState(self.stack, tuple(locals_list))

    def get_local(self, index: int) -> VType:
        """Read a local variable slot.

        Raises:
            InvalidLocalError: If *index* is out of range or the slot is
                uninitialized.
        """
        if index < 0 or index >= len(self.locals):
            raise InvalidLocalError(f"Local variable slot {index} is out of range (max {len(self.locals) - 1})")
        vt = self.locals[index]
        if isinstance(vt, VTop):
            raise InvalidLocalError(f"Local variable slot {index} is not initialized")
        return vt

    @property
    def stack_depth(self) -> int:
        """Number of stack slots currently occupied."""
        return len(self.stack)

    @property
    def max_local_index(self) -> int:
        """Highest local slot index in use (or -1 if no locals)."""
        return len(self.locals) - 1


_EMPTY_FRAME = FrameState((), ())


def initial_frame(method: MethodModel, class_name: str) -> FrameState:
    """Build the entry ``FrameState`` for a method.

    Slot 0 is ``VObject(class_name)`` for instance methods, or
    ``VUninitializedThis`` for ``<init>``.  Parameter types follow,
    with category-2 values spanning two slots.  Stack is empty.

    Args:
        method: The method whose initial frame to build.
        class_name: JVM internal name of the enclosing class.

    Returns:
        A ``FrameState`` representing the method entry point.
    """
    from ..classfile.constants import MethodAccessFlag

    md = parse_method_descriptor(method.descriptor)
    locals_list: list[VType] = []

    if not (method.access_flags & MethodAccessFlag.STATIC):
        if method.name == "<init>":
            locals_list.append(_UNINIT_THIS)
        else:
            locals_list.append(VObject(class_name))

    for param in md.parameter_types:
        vt = vtype_from_descriptor(param)
        locals_list.append(vt)
        if is_category2(vt):
            locals_list.append(_TOP)

    return FrameState((), tuple(locals_list))


# ===================================================================
# Merging frame states
# ===================================================================


def _merge_frames(a: FrameState, b: FrameState, resolver: ClassResolver | None) -> FrameState:
    """Merge two frame states at a control-flow join point.

    Stacks must be the same depth (JVM spec requirement).  Locals are
    merged slot-by-slot; the shorter locals tuple is padded with ``VTop``.
    """
    if len(a.stack) != len(b.stack):
        raise TypeMergeError(f"Stack depths differ at join point: {len(a.stack)} vs {len(b.stack)}")

    merged_stack = tuple(merge_vtypes(sa, sb, resolver) for sa, sb in zip(a.stack, b.stack))

    max_locals = max(len(a.locals), len(b.locals))
    merged_locals: list[VType] = []
    for i in range(max_locals):
        la = a.locals[i] if i < len(a.locals) else _TOP
        lb = b.locals[i] if i < len(b.locals) else _TOP
        merged_locals.append(merge_vtypes(la, lb, resolver))

    return FrameState(merged_stack, tuple(merged_locals))


# ===================================================================
# Opcode metadata
# ===================================================================


@dataclass(frozen=True, slots=True)
class OpcodeEffect:
    """Static stack effect and control-flow metadata for an opcode.

    ``pops`` and ``pushes`` are ``-1`` for opcodes whose stack effects depend on
    the operand (invoke, field access, LDC, multianewarray).  Those are
    computed dynamically during simulation from the instruction's symbolic
    operand metadata.

    Attributes:
        pops: Number of stack slots consumed (``-1`` if variable).
        pushes: Number of stack slots produced (``-1`` if variable).
        is_branch: ``True`` for branch instructions.
        is_unconditional: ``True`` for unconditional transfers (goto, switch,
            athrow).
        is_switch: ``True`` for tableswitch/lookupswitch.
        is_return: ``True`` for return instructions.
    """

    pops: int
    pushes: int
    is_branch: bool = False
    is_unconditional: bool = False
    is_switch: bool = False
    is_return: bool = False


_T = InsnInfoType

# Shorthand constructors
_simple = OpcodeEffect


def _branch(p: int, u: bool) -> OpcodeEffect:
    return OpcodeEffect(p, 0, is_branch=True, is_unconditional=u)


def _ret(p: int) -> OpcodeEffect:
    return OpcodeEffect(p, 0, is_return=True)


_switch = OpcodeEffect(1, 0, is_branch=True, is_unconditional=True, is_switch=True)
_var = OpcodeEffect(-1, -1)  # variable — resolved during simulation

OPCODE_EFFECTS: dict[InsnInfoType, OpcodeEffect] = {
    # --- Constants ---
    _T.NOP: _simple(0, 0),
    _T.ACONST_NULL: _simple(0, 1),
    _T.ICONST_M1: _simple(0, 1),
    _T.ICONST_0: _simple(0, 1),
    _T.ICONST_1: _simple(0, 1),
    _T.ICONST_2: _simple(0, 1),
    _T.ICONST_3: _simple(0, 1),
    _T.ICONST_4: _simple(0, 1),
    _T.ICONST_5: _simple(0, 1),
    _T.LCONST_0: _simple(0, 2),
    _T.LCONST_1: _simple(0, 2),
    _T.FCONST_0: _simple(0, 1),
    _T.FCONST_1: _simple(0, 1),
    _T.FCONST_2: _simple(0, 1),
    _T.DCONST_0: _simple(0, 2),
    _T.DCONST_1: _simple(0, 2),
    _T.BIPUSH: _simple(0, 1),
    _T.SIPUSH: _simple(0, 1),
    _T.LDC: _var,
    _T.LDC_W: _var,
    _T.LDC2_W: _var,
    # --- Loads (raw forms — in editing model these are VarInsn) ---
    _T.ILOAD: _simple(0, 1),
    _T.LLOAD: _simple(0, 2),
    _T.FLOAD: _simple(0, 1),
    _T.DLOAD: _simple(0, 2),
    _T.ALOAD: _simple(0, 1),
    _T.ILOAD_0: _simple(0, 1),
    _T.ILOAD_1: _simple(0, 1),
    _T.ILOAD_2: _simple(0, 1),
    _T.ILOAD_3: _simple(0, 1),
    _T.LLOAD_0: _simple(0, 2),
    _T.LLOAD_1: _simple(0, 2),
    _T.LLOAD_2: _simple(0, 2),
    _T.LLOAD_3: _simple(0, 2),
    _T.FLOAD_0: _simple(0, 1),
    _T.FLOAD_1: _simple(0, 1),
    _T.FLOAD_2: _simple(0, 1),
    _T.FLOAD_3: _simple(0, 1),
    _T.DLOAD_0: _simple(0, 2),
    _T.DLOAD_1: _simple(0, 2),
    _T.DLOAD_2: _simple(0, 2),
    _T.DLOAD_3: _simple(0, 2),
    _T.ALOAD_0: _simple(0, 1),
    _T.ALOAD_1: _simple(0, 1),
    _T.ALOAD_2: _simple(0, 1),
    _T.ALOAD_3: _simple(0, 1),
    # --- Array loads ---
    _T.IALOAD: _simple(2, 1),
    _T.LALOAD: _simple(2, 2),
    _T.FALOAD: _simple(2, 1),
    _T.DALOAD: _simple(2, 2),
    _T.AALOAD: _simple(2, 1),
    _T.BALOAD: _simple(2, 1),
    _T.CALOAD: _simple(2, 1),
    _T.SALOAD: _simple(2, 1),
    # --- Stores (raw forms) ---
    _T.ISTORE: _simple(1, 0),
    _T.LSTORE: _simple(2, 0),
    _T.FSTORE: _simple(1, 0),
    _T.DSTORE: _simple(2, 0),
    _T.ASTORE: _simple(1, 0),
    _T.ISTORE_0: _simple(1, 0),
    _T.ISTORE_1: _simple(1, 0),
    _T.ISTORE_2: _simple(1, 0),
    _T.ISTORE_3: _simple(1, 0),
    _T.LSTORE_0: _simple(2, 0),
    _T.LSTORE_1: _simple(2, 0),
    _T.LSTORE_2: _simple(2, 0),
    _T.LSTORE_3: _simple(2, 0),
    _T.FSTORE_0: _simple(1, 0),
    _T.FSTORE_1: _simple(1, 0),
    _T.FSTORE_2: _simple(1, 0),
    _T.FSTORE_3: _simple(1, 0),
    _T.DSTORE_0: _simple(2, 0),
    _T.DSTORE_1: _simple(2, 0),
    _T.DSTORE_2: _simple(2, 0),
    _T.DSTORE_3: _simple(2, 0),
    _T.ASTORE_0: _simple(1, 0),
    _T.ASTORE_1: _simple(1, 0),
    _T.ASTORE_2: _simple(1, 0),
    _T.ASTORE_3: _simple(1, 0),
    # --- Array stores ---
    _T.IASTORE: _simple(3, 0),
    _T.LASTORE: _simple(4, 0),
    _T.FASTORE: _simple(3, 0),
    _T.DASTORE: _simple(4, 0),
    _T.AASTORE: _simple(3, 0),
    _T.BASTORE: _simple(3, 0),
    _T.CASTORE: _simple(3, 0),
    _T.SASTORE: _simple(3, 0),
    # --- Stack manipulation ---
    _T.POP: _simple(1, 0),
    _T.POP2: _simple(2, 0),
    _T.DUP: _simple(1, 2),
    _T.DUP_X1: _simple(2, 3),
    _T.DUP_X2: _simple(3, 4),
    _T.DUP2: _simple(2, 4),
    _T.DUP2_X1: _simple(3, 5),
    _T.DUP2_X2: _simple(4, 6),
    _T.SWAP: _simple(2, 2),
    # --- Integer arithmetic ---
    _T.IADD: _simple(2, 1),
    _T.ISUB: _simple(2, 1),
    _T.IMUL: _simple(2, 1),
    _T.IDIV: _simple(2, 1),
    _T.IREM: _simple(2, 1),
    _T.INEG: _simple(1, 1),
    _T.ISHL: _simple(2, 1),
    _T.ISHR: _simple(2, 1),
    _T.IUSHR: _simple(2, 1),
    _T.IAND: _simple(2, 1),
    _T.IOR: _simple(2, 1),
    _T.IXOR: _simple(2, 1),
    # --- Long arithmetic ---
    _T.LADD: _simple(4, 2),
    _T.LSUB: _simple(4, 2),
    _T.LMUL: _simple(4, 2),
    _T.LDIV: _simple(4, 2),
    _T.LREM: _simple(4, 2),
    _T.LNEG: _simple(2, 2),
    _T.LSHL: _simple(3, 2),
    _T.LSHR: _simple(3, 2),
    _T.LUSHR: _simple(3, 2),
    _T.LAND: _simple(4, 2),
    _T.LOR: _simple(4, 2),
    _T.LXOR: _simple(4, 2),
    # --- Float arithmetic ---
    _T.FADD: _simple(2, 1),
    _T.FSUB: _simple(2, 1),
    _T.FMUL: _simple(2, 1),
    _T.FDIV: _simple(2, 1),
    _T.FREM: _simple(2, 1),
    _T.FNEG: _simple(1, 1),
    # --- Double arithmetic ---
    _T.DADD: _simple(4, 2),
    _T.DSUB: _simple(4, 2),
    _T.DMUL: _simple(4, 2),
    _T.DDIV: _simple(4, 2),
    _T.DREM: _simple(4, 2),
    _T.DNEG: _simple(2, 2),
    # --- Conversions ---
    _T.I2L: _simple(1, 2),
    _T.I2F: _simple(1, 1),
    _T.I2D: _simple(1, 2),
    _T.L2I: _simple(2, 1),
    _T.L2F: _simple(2, 1),
    _T.L2D: _simple(2, 2),
    _T.F2I: _simple(1, 1),
    _T.F2L: _simple(1, 2),
    _T.F2D: _simple(1, 2),
    _T.D2I: _simple(2, 1),
    _T.D2L: _simple(2, 2),
    _T.D2F: _simple(2, 1),
    _T.I2B: _simple(1, 1),
    _T.I2C: _simple(1, 1),
    _T.I2S: _simple(1, 1),
    # --- Comparisons ---
    _T.LCMP: _simple(4, 1),
    _T.FCMPL: _simple(2, 1),
    _T.FCMPG: _simple(2, 1),
    _T.DCMPL: _simple(4, 1),
    _T.DCMPG: _simple(4, 1),
    # --- Conditional branches (pop 1 int) ---
    _T.IFEQ: _branch(1, False),
    _T.IFNE: _branch(1, False),
    _T.IFLT: _branch(1, False),
    _T.IFGE: _branch(1, False),
    _T.IFGT: _branch(1, False),
    _T.IFLE: _branch(1, False),
    # --- Conditional branches (pop 2 ints) ---
    _T.IF_ICMPEQ: _branch(2, False),
    _T.IF_ICMPNE: _branch(2, False),
    _T.IF_ICMPLT: _branch(2, False),
    _T.IF_ICMPGE: _branch(2, False),
    _T.IF_ICMPGT: _branch(2, False),
    _T.IF_ICMPLE: _branch(2, False),
    # --- Reference conditional branches ---
    _T.IF_ACMPEQ: _branch(2, False),
    _T.IF_ACMPNE: _branch(2, False),
    _T.IFNULL: _branch(1, False),
    _T.IFNONNULL: _branch(1, False),
    # --- Unconditional branches ---
    _T.GOTO: _branch(0, True),
    _T.GOTO_W: _branch(0, True),
    # --- Subroutine (legacy, pre-Java 6) ---
    _T.JSR: OpcodeEffect(0, 1, is_branch=True, is_unconditional=True),
    _T.JSR_W: OpcodeEffect(0, 1, is_branch=True, is_unconditional=True),
    _T.RET: OpcodeEffect(0, 0, is_branch=True, is_unconditional=True),
    # --- Switch ---
    _T.TABLESWITCH: _switch,
    _T.LOOKUPSWITCH: _switch,
    # --- Returns ---
    _T.IRETURN: _ret(1),
    _T.LRETURN: _ret(2),
    _T.FRETURN: _ret(1),
    _T.DRETURN: _ret(2),
    _T.ARETURN: _ret(1),
    _T.RETURN: _ret(0),
    # --- Field access (variable effect) ---
    _T.GETFIELD: _var,
    _T.PUTFIELD: _var,
    _T.GETSTATIC: _var,
    _T.PUTSTATIC: _var,
    # --- Method invocation (variable effect) ---
    _T.INVOKEVIRTUAL: _var,
    _T.INVOKESPECIAL: _var,
    _T.INVOKESTATIC: _var,
    _T.INVOKEINTERFACE: _var,
    _T.INVOKEDYNAMIC: _var,
    # --- Object creation ---
    _T.NEW: _simple(0, 1),
    _T.NEWARRAY: _simple(1, 1),
    _T.ANEWARRAY: _simple(1, 1),
    _T.MULTIANEWARRAY: _var,
    _T.ARRAYLENGTH: _simple(1, 1),
    # --- Type operations ---
    _T.CHECKCAST: _simple(1, 1),
    _T.INSTANCEOF: _simple(1, 1),
    # --- Throw ---
    _T.ATHROW: _ret(1),
    # --- Monitor ---
    _T.MONITORENTER: _simple(1, 0),
    _T.MONITOREXIT: _simple(1, 0),
    # --- IINC (no stack change) ---
    _T.IINC: _simple(0, 0),
    # --- WIDE variants (same effect as non-wide) ---
    _T.WIDE: _simple(0, 0),
    _T.ILOADW: _simple(0, 1),
    _T.LLOADW: _simple(0, 2),
    _T.FLOADW: _simple(0, 1),
    _T.DLOADW: _simple(0, 2),
    _T.ALOADW: _simple(0, 1),
    _T.ISTOREW: _simple(1, 0),
    _T.LSTOREW: _simple(2, 0),
    _T.FSTOREW: _simple(1, 0),
    _T.DSTOREW: _simple(2, 0),
    _T.ASTOREW: _simple(1, 0),
    _T.RETW: OpcodeEffect(0, 0, is_branch=True, is_unconditional=True),
    _T.IINCW: _simple(0, 0),
}


def _is_terminal(insn: InsnInfo) -> bool:
    """Return whether *insn* ends a basic block with no fall-through."""
    effect = OPCODE_EFFECTS.get(insn.type)
    if effect is None:
        return False
    return effect.is_unconditional or effect.is_return


def _is_branch_or_switch(insn: InsnInfo) -> bool:
    """Return whether *insn* is a branch or switch instruction."""
    effect = OPCODE_EFFECTS.get(insn.type)
    if effect is None:
        return False
    return effect.is_branch


# ===================================================================
# Control-flow graph
# ===================================================================


@dataclass(slots=True)
class BasicBlock:
    """A maximal straight-line sequence of instructions within a method.

    Mutable during construction, then frozen by ``build_cfg``.

    Attributes:
        id: Unique block index within the CFG.
        label: Label at the start of this block, if any.
        instructions: Ordered instructions in this block.
        successor_ids: Block ids of normal-flow successors.
        exception_handler_ids: ``(handler_block_id, catch_type)`` pairs for
            active exception handlers.
    """

    id: int
    label: Label | None
    instructions: list[InsnInfo]
    successor_ids: list[int]
    exception_handler_ids: list[tuple[int, str | None]]

    def __repr__(self) -> str:
        label_str = f" ({self.label!r})" if self.label is not None else ""
        return f"BasicBlock(id={self.id}{label_str}, insns={len(self.instructions)}, succs={self.successor_ids})"


@dataclass(frozen=True, slots=True)
class ExceptionEdge:
    """An exception edge from a protected block to a handler block.

    Attributes:
        handler_block_id: Block id of the exception handler.
        catch_type: Internal name of the caught exception type, or ``None``
            for a catch-all (``finally``).
    """

    handler_block_id: int
    catch_type: str | None


@dataclass(frozen=True, slots=True)
class ControlFlowGraph:
    """Control-flow graph for a method's code body.

    Attributes:
        entry: The entry basic block.
        blocks: All blocks, ordered to match the original instruction sequence.
        exception_handlers: Exception handler declarations from the code.
        label_to_block: Mapping from labels to the block they start.
    """

    entry: BasicBlock
    blocks: tuple[BasicBlock, ...]
    exception_handlers: tuple[ExceptionHandler, ...]
    label_to_block: dict[Label, BasicBlock]


def build_cfg(code: CodeModel) -> ControlFlowGraph:
    """Construct a control-flow graph from a ``CodeModel``.

    Partitions the instruction stream into basic blocks and builds edges
    for branches, fall-through, and exception handlers.

    Args:
        code: The code model to partition into basic blocks.

    Returns:
        A ``ControlFlowGraph`` with edges for all control-flow paths.
    """
    items = code.instructions
    if not items:
        empty_block = BasicBlock(id=0, label=None, instructions=[], successor_ids=[], exception_handler_ids=[])
        return ControlFlowGraph(
            entry=empty_block,
            blocks=(empty_block,),
            exception_handlers=tuple(code.exception_handlers),
            label_to_block={},
        )

    # Step 1: Identify block leaders.
    # A leader is an instruction (not a Label) that starts a new block.
    # We track leaders by their index in the items list.
    leader_indices: set[int] = set()

    # Collect all labels that are branch targets or exception handler boundaries.
    target_labels: set[int] = set()  # id() of labels that start blocks

    # Labels used as branch targets
    for item in items:
        if isinstance(item, BranchInsn):
            target_labels.add(id(item.target))
        elif isinstance(item, LookupSwitchInsn):
            target_labels.add(id(item.default_target))
            for _, lbl in item.pairs:
                target_labels.add(id(lbl))
        elif isinstance(item, TableSwitchInsn):
            target_labels.add(id(item.default_target))
            for lbl in item.targets:
                target_labels.add(id(lbl))

    # Labels used in exception handlers
    for eh in code.exception_handlers:
        target_labels.add(id(eh.start))
        target_labels.add(id(eh.end))
        target_labels.add(id(eh.handler))

    # First real instruction is always a leader.
    first_insn_idx = _find_first_insn(items)
    if first_insn_idx is not None:
        leader_indices.add(first_insn_idx)

    # Scan for leaders.
    prev_was_terminal = False
    for i, item in enumerate(items):
        if isinstance(item, Label):
            if id(item) in target_labels:
                # The next real instruction after this label is a leader.
                next_insn = _find_next_insn(items, i + 1)
                if next_insn is not None:
                    leader_indices.add(next_insn)
                else:
                    leader_indices.add(i)
            continue

        # item is an InsnInfo
        if prev_was_terminal:
            leader_indices.add(i)

        prev_was_terminal = _is_terminal(item) or (
            _is_branch_or_switch(item) and not OPCODE_EFFECTS[item.type].is_unconditional
        )
        # For conditional branches, the fall-through is the next insn, which
        # is implicitly a leader only if control can reach it from multiple paths.
        # But we still need to split after any branch for clean block boundaries.
        if _is_branch_or_switch(item):
            prev_was_terminal = True

    if not leader_indices:
        # All labels, no real instructions — create a single empty block.
        empty_block = BasicBlock(id=0, label=None, instructions=[], successor_ids=[], exception_handler_ids=[])
        lbl_map: dict[Label, BasicBlock] = {}
        for item in items:
            if isinstance(item, Label):
                lbl_map[item] = empty_block
        return ControlFlowGraph(
            entry=empty_block,
            blocks=(empty_block,),
            exception_handlers=tuple(code.exception_handlers),
            label_to_block=lbl_map,
        )

    # Step 2: Build blocks.
    sorted_leaders = sorted(leader_indices)
    leader_set = set(sorted_leaders)

    blocks: list[BasicBlock] = []
    block_for_index: dict[int, int] = {}  # items index → block id
    label_to_block_map: dict[Label, BasicBlock] = {}

    current_block_id = 0
    current_block: BasicBlock | None = None
    pending_labels: list[Label] = []

    for i, item in enumerate(items):
        if isinstance(item, Label):
            next_insn = _find_next_insn(items, i + 1)
            if current_block is not None and (next_insn is None or next_insn not in leader_set):
                label_to_block_map[item] = current_block
            else:
                pending_labels.append(item)
            continue

        # item is an InsnInfo
        if i in leader_set:
            # Start a new block.
            block_label = pending_labels[0] if pending_labels else None
            current_block = BasicBlock(
                id=current_block_id,
                label=block_label,
                instructions=[],
                successor_ids=[],
                exception_handler_ids=[],
            )
            # Map all pending labels to this block.
            for lbl in pending_labels:
                label_to_block_map[lbl] = current_block
            pending_labels = []
            blocks.append(current_block)
            current_block_id += 1

        if current_block is not None:
            current_block.instructions.append(item)
            block_for_index[i] = current_block.id

    # Map any trailing labels to the last block.
    if pending_labels and blocks:
        for lbl in pending_labels:
            label_to_block_map[lbl] = blocks[-1]

    # Also map labels that precede leader instructions to their block.
    # Walk items again to pick up labels immediately before leader instructions.
    pending_labels_2: list[Label] = []
    for i, item in enumerate(items):
        if isinstance(item, Label):
            pending_labels_2.append(item)
        else:
            if pending_labels_2:
                if i in leader_set:
                    for lbl in pending_labels_2:
                        if lbl not in label_to_block_map:
                            # Find the block for this leader
                            for blk in blocks:
                                if blk.instructions and blk.instructions[0] is item:
                                    label_to_block_map[lbl] = blk
                                    break
                pending_labels_2 = []

    # Step 3: Build edges.
    for idx, block in enumerate(blocks):
        if not block.instructions:
            # Empty block falls through to next block.
            if idx + 1 < len(blocks):
                block.successor_ids.append(blocks[idx + 1].id)
            continue

        last_insn = block.instructions[-1]
        effect = OPCODE_EFFECTS.get(last_insn.type)

        # Branch targets
        if isinstance(last_insn, BranchInsn):
            target_block = label_to_block_map.get(last_insn.target)
            if target_block is not None:
                block.successor_ids.append(target_block.id)
        elif isinstance(last_insn, LookupSwitchInsn):
            default_block = label_to_block_map.get(last_insn.default_target)
            if default_block is not None:
                block.successor_ids.append(default_block.id)
            for _, lbl in last_insn.pairs:
                target_block = label_to_block_map.get(lbl)
                if target_block is not None and target_block.id not in block.successor_ids:
                    block.successor_ids.append(target_block.id)
        elif isinstance(last_insn, TableSwitchInsn):
            default_block = label_to_block_map.get(last_insn.default_target)
            if default_block is not None:
                block.successor_ids.append(default_block.id)
            for lbl in last_insn.targets:
                target_block = label_to_block_map.get(lbl)
                if target_block is not None and target_block.id not in block.successor_ids:
                    block.successor_ids.append(target_block.id)

        # Fall-through edge (only if not unconditional/terminal)
        is_terminal_insn = effect is not None and (effect.is_unconditional or effect.is_return)
        if not is_terminal_insn and idx + 1 < len(blocks):
            block.successor_ids.append(blocks[idx + 1].id)

    # Step 4: Build exception handler edges.
    # For each exception handler, find blocks in the protected range and add edges.
    for eh in code.exception_handlers:
        start_block = label_to_block_map.get(eh.start)
        end_block = label_to_block_map.get(eh.end)
        handler_block = label_to_block_map.get(eh.handler)

        if start_block is None or handler_block is None:
            continue

        start_id = start_block.id
        end_id = end_block.id if end_block is not None else len(blocks)

        for block in blocks:
            if start_id <= block.id < end_id:
                edge = (handler_block.id, eh.catch_type)
                if edge not in block.exception_handler_ids:
                    block.exception_handler_ids.append(edge)

    return ControlFlowGraph(
        entry=blocks[0],
        blocks=tuple(blocks),
        exception_handlers=tuple(code.exception_handlers),
        label_to_block=label_to_block_map,
    )


def _find_first_insn(items: list[CodeItem]) -> int | None:
    """Return the index of the first real instruction in *items*."""
    for i, item in enumerate(items):
        if isinstance(item, InsnInfo):
            return i
    return None


def _find_next_insn(items: list[CodeItem], start: int) -> int | None:
    """Return the index of the next real instruction at or after *start*."""
    for i in range(start, len(items)):
        if isinstance(items[i], InsnInfo):
            return i
    return None


# ===================================================================
# Stack and local simulation
# ===================================================================


@dataclass(frozen=True, slots=True)
class SimulationResult:
    """Results of forward dataflow stack/local simulation.

    Attributes:
        entry_states: Mapping from block id to the frame state on entry.
        exit_states: Mapping from block id to the frame state on exit.
        max_stack: Maximum operand stack depth observed.
        max_locals: Maximum local variable slot count observed.
    """

    entry_states: dict[int, FrameState]
    exit_states: dict[int, FrameState]
    max_stack: int
    max_locals: int


def simulate(
    cfg: ControlFlowGraph,
    code: CodeModel,
    method: MethodModel,
    class_name: str,
    resolver: ClassResolver | None = None,
) -> SimulationResult:
    """Run forward dataflow analysis over a control-flow graph.

    Propagates ``FrameState`` through each basic block, merging at join
    points using a worklist algorithm.

    Args:
        cfg: Control-flow graph to analyze.
        code: Code model providing the instruction stream.
        method: Method model (used to derive the initial frame).
        class_name: JVM internal name of the enclosing class.
        resolver: Optional class hierarchy resolver for precise type merging.

    Returns:
        A ``SimulationResult`` with per-block entry/exit states and
        computed max_stack/max_locals.
    """
    if not cfg.blocks:
        entry = initial_frame(method, class_name)
        return SimulationResult(
            entry_states={},
            exit_states={},
            max_stack=0,
            max_locals=len(entry.locals),
        )

    analysis_code = _prepare_analysis_code(code)
    entry_frame = initial_frame(method, class_name)

    entry_states: dict[int, FrameState] = {cfg.entry.id: entry_frame}
    exit_states: dict[int, FrameState] = {}

    # Worklist: queue of block ids to process.
    worklist: deque[int] = deque([cfg.entry.id])
    in_worklist: set[int] = {cfg.entry.id}

    max_stack = 0
    max_locals = len(entry_frame.locals)

    # Build a quick successor lookup including exception handler targets.
    block_by_id = {b.id: b for b in cfg.blocks}

    while worklist:
        block_id = worklist.popleft()
        in_worklist.discard(block_id)

        block = block_by_id[block_id]
        if block_id not in entry_states:
            continue

        state = entry_states[block_id]
        if state.stack_depth > max_stack:
            max_stack = state.stack_depth
        if len(state.locals) > max_locals:
            max_locals = len(state.locals)

        # Simulate all instructions in this block.
        for item in block.instructions:
            if block.exception_handler_ids and _instruction_may_throw(item):
                _propagate_exception_handlers(
                    block.exception_handler_ids,
                    state,
                    entry_states,
                    worklist,
                    in_worklist,
                    resolver,
                )
            state = _simulate_insn(item, state, analysis_code, class_name)
            if state.stack_depth > max_stack:
                max_stack = state.stack_depth
            if len(state.locals) > max_locals:
                max_locals = len(state.locals)

        exit_states[block_id] = state

        # Propagate to successors.
        for succ_id in block.successor_ids:
            _propagate(succ_id, state, entry_states, worklist, in_worklist, resolver)

    return SimulationResult(
        entry_states=entry_states,
        exit_states=exit_states,
        max_stack=max_stack,
        max_locals=max_locals,
    )


def _propagate(
    target_id: int,
    incoming: FrameState,
    entry_states: dict[int, FrameState],
    worklist: deque[int],
    in_worklist: set[int],
    resolver: ClassResolver | None,
) -> None:
    """Merge *incoming* into the entry state of *target_id* and enqueue if changed."""
    if target_id in entry_states:
        existing = entry_states[target_id]
        try:
            merged = _merge_frames(existing, incoming, resolver)
        except TypeMergeError as exc:
            raise TypeMergeError(f"Cannot merge incoming frame into block {target_id}: {exc}") from exc
        if merged == existing:
            return  # No change — don't re-process.
        entry_states[target_id] = merged
    else:
        entry_states[target_id] = incoming

    if target_id not in in_worklist:
        worklist.append(target_id)
        in_worklist.add(target_id)


_NON_THROWING_RAW_OPCODES: frozenset[InsnInfoType] = frozenset(
    {
        _T.NOP,
        _T.ACONST_NULL,
        _T.ICONST_M1,
        _T.ICONST_0,
        _T.ICONST_1,
        _T.ICONST_2,
        _T.ICONST_3,
        _T.ICONST_4,
        _T.ICONST_5,
        _T.LCONST_0,
        _T.LCONST_1,
        _T.FCONST_0,
        _T.FCONST_1,
        _T.FCONST_2,
        _T.DCONST_0,
        _T.DCONST_1,
        _T.BIPUSH,
        _T.SIPUSH,
        _T.ILOAD,
        _T.ILOAD_0,
        _T.ILOAD_1,
        _T.ILOAD_2,
        _T.ILOAD_3,
        _T.ILOADW,
        _T.LLOAD,
        _T.LLOAD_0,
        _T.LLOAD_1,
        _T.LLOAD_2,
        _T.LLOAD_3,
        _T.LLOADW,
        _T.FLOAD,
        _T.FLOAD_0,
        _T.FLOAD_1,
        _T.FLOAD_2,
        _T.FLOAD_3,
        _T.FLOADW,
        _T.DLOAD,
        _T.DLOAD_0,
        _T.DLOAD_1,
        _T.DLOAD_2,
        _T.DLOAD_3,
        _T.DLOADW,
        _T.ALOAD,
        _T.ALOAD_0,
        _T.ALOAD_1,
        _T.ALOAD_2,
        _T.ALOAD_3,
        _T.ALOADW,
        _T.ISTORE,
        _T.ISTORE_0,
        _T.ISTORE_1,
        _T.ISTORE_2,
        _T.ISTORE_3,
        _T.ISTOREW,
        _T.LSTORE,
        _T.LSTORE_0,
        _T.LSTORE_1,
        _T.LSTORE_2,
        _T.LSTORE_3,
        _T.LSTOREW,
        _T.FSTORE,
        _T.FSTORE_0,
        _T.FSTORE_1,
        _T.FSTORE_2,
        _T.FSTORE_3,
        _T.FSTOREW,
        _T.DSTORE,
        _T.DSTORE_0,
        _T.DSTORE_1,
        _T.DSTORE_2,
        _T.DSTORE_3,
        _T.DSTOREW,
        _T.ASTORE,
        _T.ASTORE_0,
        _T.ASTORE_1,
        _T.ASTORE_2,
        _T.ASTORE_3,
        _T.ASTOREW,
        _T.POP,
        _T.POP2,
        _T.DUP,
        _T.DUP_X1,
        _T.DUP_X2,
        _T.DUP2,
        _T.DUP2_X1,
        _T.DUP2_X2,
        _T.SWAP,
        _T.IADD,
        _T.ISUB,
        _T.IMUL,
        _T.INEG,
        _T.ISHL,
        _T.ISHR,
        _T.IUSHR,
        _T.IAND,
        _T.IOR,
        _T.IXOR,
        _T.LADD,
        _T.LSUB,
        _T.LMUL,
        _T.LNEG,
        _T.LSHL,
        _T.LSHR,
        _T.LUSHR,
        _T.LAND,
        _T.LOR,
        _T.LXOR,
        _T.FADD,
        _T.FSUB,
        _T.FMUL,
        _T.FDIV,
        _T.FREM,
        _T.FNEG,
        _T.DADD,
        _T.DSUB,
        _T.DMUL,
        _T.DDIV,
        _T.DREM,
        _T.DNEG,
        _T.I2L,
        _T.I2F,
        _T.I2D,
        _T.L2I,
        _T.L2F,
        _T.L2D,
        _T.F2I,
        _T.F2L,
        _T.F2D,
        _T.D2I,
        _T.D2L,
        _T.D2F,
        _T.I2B,
        _T.I2C,
        _T.I2S,
        _T.LCMP,
        _T.FCMPL,
        _T.FCMPG,
        _T.DCMPL,
        _T.DCMPG,
        _T.IFEQ,
        _T.IFNE,
        _T.IFLT,
        _T.IFGE,
        _T.IFGT,
        _T.IFLE,
        _T.IF_ICMPEQ,
        _T.IF_ICMPNE,
        _T.IF_ICMPLT,
        _T.IF_ICMPGE,
        _T.IF_ICMPGT,
        _T.IF_ICMPLE,
        _T.IF_ACMPEQ,
        _T.IF_ACMPNE,
        _T.GOTO,
        _T.GOTO_W,
        _T.JSR,
        _T.JSR_W,
        _T.RET,
        _T.RETW,
        _T.IFNULL,
        _T.IFNONNULL,
        _T.TABLESWITCH,
        _T.LOOKUPSWITCH,
        _T.IRETURN,
        _T.LRETURN,
        _T.FRETURN,
        _T.DRETURN,
        _T.ARETURN,
        _T.RETURN,
        _T.IINC,
        _T.IINCW,
        _T.WIDE,
    }
)


def _instruction_may_throw(insn: InsnInfo) -> bool:
    """Return whether an instruction may transfer control to an exception handler.

    The analysis stays conservative by treating any opcode outside the
    well-understood non-throwing set as potentially exceptional.
    """
    if isinstance(insn, VarInsn | IIncInsn | BranchInsn | LookupSwitchInsn | TableSwitchInsn):
        return False
    if isinstance(
        insn,
        FieldInsn | MethodInsn | InterfaceMethodInsn | InvokeDynamicInsn | TypeInsn | MultiANewArrayInsn,
    ):
        return True
    if isinstance(insn, LdcInsn):
        return False
    return insn.type not in _NON_THROWING_RAW_OPCODES


def _propagate_exception_handlers(
    handler_edges: list[tuple[int, str | None]],
    state: FrameState,
    entry_states: dict[int, FrameState],
    worklist: deque[int],
    in_worklist: set[int],
    resolver: ClassResolver | None,
) -> None:
    """Propagate the pre-instruction state to each active exception handler."""
    for handler_id, catch_type in handler_edges:
        if catch_type is not None:
            handler_stack = (VObject(catch_type),)
        else:
            handler_stack = (_OBJECT_THROWABLE,)
        handler_state = FrameState(handler_stack, state.locals)
        _propagate(handler_id, handler_state, entry_states, worklist, in_worklist, resolver)


# ===================================================================
# Per-instruction simulation
# ===================================================================


# VarInsn canonical opcode → type category for loads/stores.
_LOAD_TYPE_MAP: dict[InsnInfoType, VType] = {
    _T.ILOAD: _INTEGER,
    _T.LLOAD: _LONG,
    _T.FLOAD: _FLOAT,
    _T.DLOAD: _DOUBLE,
    _T.ALOAD: _NULL,  # placeholder — actual type comes from the local
}

_STORE_OPCODES: frozenset[InsnInfoType] = frozenset(
    {
        _T.ISTORE,
        _T.LSTORE,
        _T.FSTORE,
        _T.DSTORE,
        _T.ASTORE,
    }
)


def _simulate_insn(
    insn: InsnInfo,
    state: FrameState,
    code: CodeModel,
    class_name: str,
) -> FrameState:
    """Apply the effect of one instruction to the frame state."""

    # --- VarInsn (symbolic load/store) ---
    if isinstance(insn, VarInsn):
        return _simulate_var_insn(insn, state)

    # --- IIncInsn ---
    if isinstance(insn, IIncInsn):
        # No stack change; just verify the local is integer-typed.
        return state

    # --- FieldInsn ---
    if isinstance(insn, FieldInsn):
        return _simulate_field_insn(insn, state)

    # --- MethodInsn ---
    if isinstance(insn, MethodInsn):
        return _simulate_method_insn(insn, state, class_name)

    # --- InterfaceMethodInsn ---
    if isinstance(insn, InterfaceMethodInsn):
        return _simulate_interface_method_insn(insn, state)

    # --- InvokeDynamicInsn ---
    if isinstance(insn, InvokeDynamicInsn):
        return _simulate_invokedynamic_insn(insn, state)

    # --- TypeInsn ---
    if isinstance(insn, TypeInsn):
        return _simulate_type_insn(insn, state, code)

    # --- LdcInsn ---
    if isinstance(insn, LdcInsn):
        return _simulate_ldc_insn(insn, state)

    # --- MultiANewArrayInsn ---
    if isinstance(insn, MultiANewArrayInsn):
        state, _ = state.pop(insn.dimensions)
        return state.push(VObject(insn.class_name))

    # --- BranchInsn (conditional branches pop operands, GOTO does not) ---
    if isinstance(insn, BranchInsn):
        return _simulate_branch_insn(insn, state)

    # --- Switch ---
    if isinstance(insn, LookupSwitchInsn | TableSwitchInsn):
        state, _ = state.pop(1)  # pop the key
        return state

    # --- All other InsnInfo (raw opcodes with static effects) ---
    return _simulate_raw_insn(insn, state)


def _simulate_var_insn(insn: VarInsn, state: FrameState) -> FrameState:
    """Simulate a VarInsn (load/store)."""
    opcode = insn.type
    slot = insn.slot

    if opcode in _STORE_OPCODES:
        # Store: pop from stack, write to local.
        if opcode == _T.LSTORE:
            state, _ = state.pop(2)
            return state.set_local(slot, _LONG)
        elif opcode == _T.DSTORE:
            state, _ = state.pop(2)
            return state.set_local(slot, _DOUBLE)
        else:
            state, (val,) = state.pop(1)
            return state.set_local(slot, val)

    if opcode == _T.RET:
        return state

    # Load: read from local, push to stack.
    if opcode == _T.ALOAD:
        val = state.get_local(slot)
        return state.push(val)

    # For typed loads, we push the type from the load opcode.
    vt = _LOAD_TYPE_MAP.get(opcode, _INTEGER)
    state.get_local(slot)
    return state.push(vt)


def _simulate_field_insn(insn: FieldInsn, state: FrameState) -> FrameState:
    """Simulate a field access instruction."""
    field_type = vtype_from_field_descriptor_str(insn.descriptor)
    field_slots = 2 if is_category2(field_type) else 1

    if insn.type == _T.GETFIELD:
        state, _ = state.pop(1)  # pop objectref
        return state.push(field_type)
    elif insn.type == _T.PUTFIELD:
        state, _ = state.pop(field_slots)  # pop value
        state, _ = state.pop(1)  # pop objectref
        return state
    elif insn.type == _T.GETSTATIC:
        return state.push(field_type)
    else:  # PUTSTATIC
        state, _ = state.pop(field_slots)
        return state


def _simulate_method_insn(
    insn: MethodInsn,
    state: FrameState,
    class_name: str,
) -> FrameState:
    """Simulate INVOKEVIRTUAL, INVOKESPECIAL, INVOKESTATIC."""
    md = parse_method_descriptor(insn.descriptor)
    # Pop arguments (right to left in slots).
    arg_slots = sum(2 if is_category2(vtype_from_descriptor(p)) else 1 for p in md.parameter_types)
    state, _ = state.pop(arg_slots)

    # Pop objectref for non-static methods.
    if insn.type != _T.INVOKESTATIC:
        state, (receiver,) = state.pop(1)
        # Successful constructor calls initialize either ``this`` or the
        # freshly allocated object referenced by ``receiver``.
        if insn.name == "<init>" and isinstance(receiver, VUninitializedThis):
            state = _replace_uninitialized(state, receiver, VObject(class_name))
        elif insn.name == "<init>" and isinstance(receiver, VUninitialized):
            state = _replace_uninitialized(state, receiver, VObject(insn.owner))

    # Push return value.
    if not isinstance(md.return_type, VoidType):
        ret_type = vtype_from_descriptor(md.return_type)
        state = state.push(ret_type)

    return state


def _simulate_interface_method_insn(insn: InterfaceMethodInsn, state: FrameState) -> FrameState:
    """Simulate INVOKEINTERFACE."""
    md = parse_method_descriptor(insn.descriptor)
    arg_slots = sum(2 if is_category2(vtype_from_descriptor(p)) else 1 for p in md.parameter_types)
    state, _ = state.pop(arg_slots)
    state, _ = state.pop(1)  # pop objectref

    if not isinstance(md.return_type, VoidType):
        ret_type = vtype_from_descriptor(md.return_type)
        state = state.push(ret_type)
    return state


def _simulate_invokedynamic_insn(insn: InvokeDynamicInsn, state: FrameState) -> FrameState:
    """Simulate INVOKEDYNAMIC."""
    md = parse_method_descriptor(insn.descriptor)
    arg_slots = sum(2 if is_category2(vtype_from_descriptor(p)) else 1 for p in md.parameter_types)
    state, _ = state.pop(arg_slots)

    if not isinstance(md.return_type, VoidType):
        ret_type = vtype_from_descriptor(md.return_type)
        state = state.push(ret_type)
    return state


def _simulate_type_insn(
    insn: TypeInsn,
    state: FrameState,
    code: CodeModel,
) -> FrameState:
    """Simulate NEW, CHECKCAST, INSTANCEOF, ANEWARRAY."""
    if insn.type == _T.NEW:
        new_label = _find_label_for_insn(code, insn)
        if new_label is None:
            raise AnalysisError("NEW instruction is missing an analysis label")
        return state.push(VUninitialized(new_label))
    elif insn.type == _T.CHECKCAST:
        state, _ = state.pop(1)
        return state.push(VObject(insn.class_name))
    elif insn.type == _T.INSTANCEOF:
        state, _ = state.pop(1)
        return state.push(_INTEGER)
    else:  # ANEWARRAY
        state, _ = state.pop(1)  # pop count
        # ANEWARRAY creates an array of reference type.
        if insn.class_name.startswith("["):
            return state.push(VObject("[" + insn.class_name))
        else:
            return state.push(VObject("[L" + insn.class_name + ";"))


def _simulate_ldc_insn(insn: LdcInsn, state: FrameState) -> FrameState:
    """Simulate LDC/LDC_W/LDC2_W."""
    val = insn.value
    if isinstance(val, LdcInt):
        return state.push(_INTEGER)
    elif isinstance(val, LdcFloat):
        return state.push(_FLOAT)
    elif isinstance(val, LdcLong):
        return state.push(_LONG)
    elif isinstance(val, LdcDouble):
        return state.push(_DOUBLE)
    elif isinstance(val, LdcString):
        return state.push(_OBJECT_STRING)
    elif isinstance(val, LdcClass):
        return state.push(_OBJECT_CLASS)
    elif isinstance(val, LdcMethodType):
        return state.push(_OBJECT_METHOD_TYPE)
    elif isinstance(val, LdcMethodHandle):
        return state.push(_OBJECT_METHOD_HANDLE)
    else:
        # LdcDynamic — type determined by descriptor.
        vt = vtype_from_field_descriptor_str(val.descriptor)
        return state.push(vt)


def _simulate_branch_insn(insn: BranchInsn, state: FrameState) -> FrameState:
    """Simulate the stack effect of a branch instruction (pop condition operands)."""
    effect = OPCODE_EFFECTS.get(insn.type)
    if effect is not None and effect.pops > 0:
        state, _ = state.pop(effect.pops)
    # JSR/JSR_W pushes a return address onto the stack.
    if insn.type in {_T.JSR, _T.JSR_W}:
        return state.push(_INTEGER)
    return state


def _simulate_raw_insn(insn: InsnInfo, state: FrameState) -> FrameState:
    """Simulate a raw (non-symbolic) instruction using the opcode effect table."""
    opcode = insn.type

    # --- Stack manipulation (requires type-aware handling) ---
    if opcode == _T.DUP:
        val = state.peek(0)
        return FrameState(state.stack + (val,), state.locals)
    elif opcode == _T.DUP_X1:
        v1 = state.peek(0)
        v2 = state.peek(1)
        stack = state.stack[:-2] + (v1, v2, v1)
        return FrameState(stack, state.locals)
    elif opcode == _T.DUP_X2:
        v1 = state.peek(0)
        v2 = state.peek(1)
        v3 = state.peek(2)
        stack = state.stack[:-3] + (v1, v3, v2, v1)
        return FrameState(stack, state.locals)
    elif opcode == _T.DUP2:
        v1 = state.peek(0)
        v2 = state.peek(1)
        return FrameState(state.stack + (v2, v1), state.locals)
    elif opcode == _T.DUP2_X1:
        v1 = state.peek(0)
        v2 = state.peek(1)
        v3 = state.peek(2)
        stack = state.stack[:-3] + (v2, v1, v3, v2, v1)
        return FrameState(stack, state.locals)
    elif opcode == _T.DUP2_X2:
        v1 = state.peek(0)
        v2 = state.peek(1)
        v3 = state.peek(2)
        v4 = state.peek(3)
        stack = state.stack[:-4] + (v2, v1, v4, v3, v2, v1)
        return FrameState(stack, state.locals)
    elif opcode == _T.SWAP:
        v1 = state.peek(0)
        v2 = state.peek(1)
        stack = state.stack[:-2] + (v1, v2)
        return FrameState(stack, state.locals)
    elif opcode == _T.POP:
        state, _ = state.pop(1)
        return state
    elif opcode == _T.POP2:
        state, _ = state.pop(2)
        return state

    # --- Constants ---
    if opcode == _T.ACONST_NULL:
        return state.push(_NULL)
    if opcode in {_T.ICONST_M1, _T.ICONST_0, _T.ICONST_1, _T.ICONST_2, _T.ICONST_3, _T.ICONST_4, _T.ICONST_5}:
        return state.push(_INTEGER)
    if opcode in {_T.LCONST_0, _T.LCONST_1}:
        return state.push(_LONG)
    if opcode in {_T.FCONST_0, _T.FCONST_1, _T.FCONST_2}:
        return state.push(_FLOAT)
    if opcode in {_T.DCONST_0, _T.DCONST_1}:
        return state.push(_DOUBLE)
    if opcode == _T.BIPUSH:
        return state.push(_INTEGER)
    if opcode == _T.SIPUSH:
        return state.push(_INTEGER)

    # --- Arithmetic (result type by opcode prefix) ---
    if opcode in {_T.IADD, _T.ISUB, _T.IMUL, _T.IDIV, _T.IREM, _T.ISHL, _T.ISHR, _T.IUSHR, _T.IAND, _T.IOR, _T.IXOR}:
        state, _ = state.pop(2)
        return state.push(_INTEGER)
    if opcode == _T.INEG:
        state, _ = state.pop(1)
        return state.push(_INTEGER)

    if opcode in {_T.LADD, _T.LSUB, _T.LMUL, _T.LDIV, _T.LREM, _T.LAND, _T.LOR, _T.LXOR}:
        state, _ = state.pop(4)
        return state.push(_LONG)
    if opcode in {_T.LSHL, _T.LSHR, _T.LUSHR}:
        state, _ = state.pop(3)  # long + int shift amount
        return state.push(_LONG)
    if opcode == _T.LNEG:
        state, _ = state.pop(2)
        return state.push(_LONG)

    if opcode in {_T.FADD, _T.FSUB, _T.FMUL, _T.FDIV, _T.FREM}:
        state, _ = state.pop(2)
        return state.push(_FLOAT)
    if opcode == _T.FNEG:
        state, _ = state.pop(1)
        return state.push(_FLOAT)

    if opcode in {_T.DADD, _T.DSUB, _T.DMUL, _T.DDIV, _T.DREM}:
        state, _ = state.pop(4)
        return state.push(_DOUBLE)
    if opcode == _T.DNEG:
        state, _ = state.pop(2)
        return state.push(_DOUBLE)

    # --- Conversions ---
    if opcode == _T.I2L:
        state, _ = state.pop(1)

        return state.push(_LONG)
    if opcode == _T.I2F:
        state, _ = state.pop(1)

        return state.push(_FLOAT)
    if opcode == _T.I2D:
        state, _ = state.pop(1)

        return state.push(_DOUBLE)
    if opcode == _T.L2I:
        state, _ = state.pop(2)

        return state.push(_INTEGER)
    if opcode == _T.L2F:
        state, _ = state.pop(2)

        return state.push(_FLOAT)
    if opcode == _T.L2D:
        state, _ = state.pop(2)

        return state.push(_DOUBLE)
    if opcode == _T.F2I:
        state, _ = state.pop(1)

        return state.push(_INTEGER)
    if opcode == _T.F2L:
        state, _ = state.pop(1)

        return state.push(_LONG)
    if opcode == _T.F2D:
        state, _ = state.pop(1)

        return state.push(_DOUBLE)
    if opcode == _T.D2I:
        state, _ = state.pop(2)

        return state.push(_INTEGER)
    if opcode == _T.D2L:
        state, _ = state.pop(2)

        return state.push(_LONG)
    if opcode == _T.D2F:
        state, _ = state.pop(2)

        return state.push(_FLOAT)
    if opcode in {_T.I2B, _T.I2C, _T.I2S}:
        state, _ = state.pop(1)

        return state.push(_INTEGER)

    # --- Comparisons ---
    if opcode == _T.LCMP:
        state, _ = state.pop(4)

        return state.push(_INTEGER)
    if opcode in {_T.FCMPL, _T.FCMPG}:
        state, _ = state.pop(2)

        return state.push(_INTEGER)
    if opcode in {_T.DCMPL, _T.DCMPG}:
        state, _ = state.pop(4)

        return state.push(_INTEGER)

    # --- Array loads ---
    if opcode in {_T.IALOAD, _T.BALOAD, _T.CALOAD, _T.SALOAD}:
        state, _ = state.pop(2)

        return state.push(_INTEGER)
    if opcode == _T.LALOAD:
        state, _ = state.pop(2)

        return state.push(_LONG)
    if opcode == _T.FALOAD:
        state, _ = state.pop(2)

        return state.push(_FLOAT)
    if opcode == _T.DALOAD:
        state, _ = state.pop(2)

        return state.push(_DOUBLE)
    if opcode == _T.AALOAD:
        state, (_, arrayref) = state.pop(2)
        # Try to determine component type from array reference.
        if isinstance(arrayref, VObject) and arrayref.class_name.startswith("["):
            component = arrayref.class_name[1:]
            if component.startswith("L") and component.endswith(";"):
                return state.push(VObject(component[1:-1]))
            elif component.startswith("["):
                return state.push(VObject(component))
            # Primitive component (e.g. "[I") — invalid bytecode for AALOAD
            # (should use IALOAD/FALOAD/etc.), fall through to Object default.
        return state.push(_OBJECT_OBJECT)

    # --- Array stores ---
    if opcode in {_T.IASTORE, _T.BASTORE, _T.CASTORE, _T.SASTORE, _T.FASTORE, _T.AASTORE}:
        state, _ = state.pop(3)

        return state
    if opcode in {_T.LASTORE, _T.DASTORE}:
        state, _ = state.pop(4)

        return state

    # --- Returns ---
    if opcode in {_T.IRETURN, _T.FRETURN, _T.ARETURN}:
        state, _ = state.pop(1)

        return state
    if opcode in {_T.LRETURN, _T.DRETURN}:
        state, _ = state.pop(2)

        return state
    if opcode == _T.RETURN:
        return state

    # --- ATHROW ---
    if opcode == _T.ATHROW:
        state, _ = state.pop(1)

        return state

    # --- Monitor ---
    if opcode in {_T.MONITORENTER, _T.MONITOREXIT}:
        state, _ = state.pop(1)

        return state

    # --- Array length ---
    if opcode == _T.ARRAYLENGTH:
        state, _ = state.pop(1)

        return state.push(_INTEGER)

    # --- NEWARRAY ---
    if opcode == _T.NEWARRAY:
        state, _ = state.pop(1)  # pop count
        from ..classfile.instructions import NewArray as NewArrayInsn

        if isinstance(insn, NewArrayInsn):
            array_desc = _NEWARRAY_TYPE_MAP.get(insn.atype, "[I")
            return state.push(VObject(array_desc))
        return state.push(VObject("[I"))

    # --- NOP / IINC / WIDE ---
    if opcode in {_T.NOP, _T.IINC, _T.IINCW, _T.WIDE}:
        return state

    # --- JSR (pushes return address) ---
    if opcode in {_T.JSR, _T.JSR_W}:
        return state.push(_INTEGER)  # return address (treated as integer for simplicity)

    # --- RET ---
    if opcode in {_T.RET, _T.RETW}:
        return state

    # --- Raw load/store opcodes (when not lifted to VarInsn) ---
    # These shouldn't appear in editing model code, but handle gracefully.
    if opcode in {_T.ILOAD, _T.ILOAD_0, _T.ILOAD_1, _T.ILOAD_2, _T.ILOAD_3, _T.ILOADW}:
        return state.push(_INTEGER)
    if opcode in {_T.LLOAD, _T.LLOAD_0, _T.LLOAD_1, _T.LLOAD_2, _T.LLOAD_3, _T.LLOADW}:
        return state.push(_LONG)
    if opcode in {_T.FLOAD, _T.FLOAD_0, _T.FLOAD_1, _T.FLOAD_2, _T.FLOAD_3, _T.FLOADW}:
        return state.push(_FLOAT)
    if opcode in {_T.DLOAD, _T.DLOAD_0, _T.DLOAD_1, _T.DLOAD_2, _T.DLOAD_3, _T.DLOADW}:
        return state.push(_DOUBLE)
    if opcode in {_T.ALOAD, _T.ALOAD_0, _T.ALOAD_1, _T.ALOAD_2, _T.ALOAD_3, _T.ALOADW}:
        return state.push(_OBJECT_OBJECT)
    if opcode in {_T.ISTORE, _T.ISTORE_0, _T.ISTORE_1, _T.ISTORE_2, _T.ISTORE_3, _T.ISTOREW}:
        state, _ = state.pop(1)

        return state
    if opcode in {_T.LSTORE, _T.LSTORE_0, _T.LSTORE_1, _T.LSTORE_2, _T.LSTORE_3, _T.LSTOREW}:
        state, _ = state.pop(2)

        return state
    if opcode in {_T.FSTORE, _T.FSTORE_0, _T.FSTORE_1, _T.FSTORE_2, _T.FSTORE_3, _T.FSTOREW}:
        state, _ = state.pop(1)

        return state
    if opcode in {_T.DSTORE, _T.DSTORE_0, _T.DSTORE_1, _T.DSTORE_2, _T.DSTORE_3, _T.DSTOREW}:
        state, _ = state.pop(2)

        return state
    if opcode in {_T.ASTORE, _T.ASTORE_0, _T.ASTORE_1, _T.ASTORE_2, _T.ASTORE_3, _T.ASTOREW}:
        state, _ = state.pop(1)

        return state

    # Unrecognized opcode — conservative no-op.
    return state


def _replace_uninitialized(
    state: FrameState,
    uninit: VUninitialized | VUninitializedThis,
    replacement: VObject,
) -> FrameState:
    """Replace all occurrences of *uninit* in the frame with *replacement*.

    After a successful ``<init>`` call, all references to the uninitialized
    object (on the stack and in locals) must be replaced with the initialized
    type (JVM spec §4.10.1.4).
    """
    new_stack = tuple(replacement if v == uninit else v for v in state.stack)
    new_locals = tuple(replacement if v == uninit else v for v in state.locals)
    return FrameState(new_stack, new_locals)


def _prepare_analysis_code(code: CodeModel) -> CodeModel:
    """Insert transient labels before unlabeled ``NEW`` instructions."""
    prepared_items: list[CodeItem] = []
    inserted = False
    prev_was_label = False

    for item in code.instructions:
        if isinstance(item, Label):
            prepared_items.append(item)
            prev_was_label = True
            continue

        if isinstance(item, TypeInsn) and item.type == _T.NEW and not prev_was_label:
            prepared_items.append(Label())
            inserted = True
        prepared_items.append(item)
        prev_was_label = False

    if not inserted:
        return code

    return type(code)(
        max_stack=code.max_stack,
        max_locals=code.max_locals,
        instructions=prepared_items,
        exception_handlers=code.exception_handlers,
        line_numbers=code.line_numbers,
        local_variables=code.local_variables,
        local_variable_types=code.local_variable_types,
        attributes=code.attributes,
    )


def _find_label_for_insn(code: CodeModel, target_insn: InsnInfo) -> Label | None:
    """Find the Label immediately preceding *target_insn* in the code.

    Returns ``None`` if no label precedes the instruction.  Simulation calls
    this on the analysis-prepared instruction stream, where unlabeled
    ``NEW`` instructions have already been given a synthetic label.
    """
    prev_label: Label | None = None
    for item in code.instructions:
        if item is target_insn:
            return prev_label
        if isinstance(item, Label):
            prev_label = item
        else:
            prev_label = None
    return None


# ===================================================================
# VType → VerificationTypeInfo conversion
# ===================================================================


def _vtype_to_vti(
    vtype: VType,
    cp: ConstantPoolBuilder,
    label_offsets: dict[Label, int],
) -> VerificationTypeInfo:
    """Convert a verification type to a raw ``VerificationTypeInfo``."""
    if isinstance(vtype, VTop):
        return TopVariableInfo(VerificationType.TOP)
    if isinstance(vtype, VInteger):
        return IntegerVariableInfo(VerificationType.INTEGER)
    if isinstance(vtype, VFloat):
        return FloatVariableInfo(VerificationType.FLOAT)
    if isinstance(vtype, VLong):
        return LongVariableInfo(VerificationType.LONG)
    if isinstance(vtype, VDouble):
        return DoubleVariableInfo(VerificationType.DOUBLE)
    if isinstance(vtype, VNull):
        return NullVariableInfo(VerificationType.NULL)
    if isinstance(vtype, VUninitializedThis):
        return UninitializedThisVariableInfo(VerificationType.UNINITIALIZED_THIS)
    if isinstance(vtype, VObject):
        return ObjectVariableInfo(VerificationType.OBJECT, cp.add_class(vtype.class_name))
    # VUninitialized
    offset = label_offsets.get(vtype.new_label)
    if offset is None:
        raise ValueError(f"missing bytecode offset for uninitialized NEW site {vtype.new_label!r}")
    return UninitializedVariableInfo(VerificationType.UNINITIALIZED, offset)


def _vtypes_to_vtis(
    vtypes: tuple[VType, ...],
    cp: ConstantPoolBuilder,
    label_offsets: dict[Label, int],
) -> list[VerificationTypeInfo]:
    """Convert a tuple of verification types to raw ``VerificationTypeInfo`` list."""
    return [_vtype_to_vti(vt, cp, label_offsets) for vt in vtypes]


def _verification_type_info_size(vti: VerificationTypeInfo) -> int:
    """Return the serialized size of a ``verification_type_info``."""
    if isinstance(vti, ObjectVariableInfo | UninitializedVariableInfo):
        return 3
    return 1


def _stack_map_frame_size(frame: StackMapFrameInfo) -> int:
    """Return the serialized size of a ``stack_map_frame``."""
    if isinstance(frame, SameFrameInfo):
        return 1
    if isinstance(frame, SameLocals1StackItemFrameInfo):
        return 1 + _verification_type_info_size(frame.stack)
    if isinstance(frame, SameLocals1StackItemFrameExtendedInfo):
        return 3 + _verification_type_info_size(frame.stack)
    if isinstance(frame, ChopFrameInfo | SameFrameExtendedInfo):
        return 3
    if isinstance(frame, AppendFrameInfo):
        return 3 + sum(_verification_type_info_size(vti) for vti in frame.locals)
    if isinstance(frame, FullFrameInfo):
        return (
            7
            + sum(_verification_type_info_size(vti) for vti in frame.locals)
            + sum(_verification_type_info_size(vti) for vti in frame.stack)
        )
    raise TypeError(f"unsupported stack map frame type: {type(frame).__name__}")


def _stack_map_table_attribute_length(frames: Sequence[StackMapFrameInfo]) -> int:
    """Return the serialized ``attribute_length`` for a ``StackMapTable``."""
    return 2 + sum(_stack_map_frame_size(frame) for frame in frames)


# ===================================================================
# Compact frame encoding selection
# ===================================================================


def _select_frame(
    offset_delta: int,
    prev_locals: Sequence[VerificationTypeInfo],
    curr_locals: Sequence[VerificationTypeInfo],
    curr_stack: Sequence[VerificationTypeInfo],
) -> StackMapFrameInfo:
    """Select the most compact StackMapTable frame encoding.

    Follows JVM spec §4.7.4 frame type selection rules.
    """
    locals_same = prev_locals == curr_locals

    if locals_same and not curr_stack:
        # same_frame or same_frame_extended
        if offset_delta <= 63:
            return SameFrameInfo(frame_type=offset_delta)
        return SameFrameExtendedInfo(frame_type=251, offset_delta=offset_delta)

    if locals_same and len(curr_stack) == 1:
        # same_locals_1_stack_item or extended variant
        if offset_delta <= 63:
            return SameLocals1StackItemFrameInfo(
                frame_type=64 + offset_delta,
                stack=curr_stack[0],
            )
        return SameLocals1StackItemFrameExtendedInfo(
            frame_type=247,
            offset_delta=offset_delta,
            stack=curr_stack[0],
        )

    if not curr_stack:
        diff = len(curr_locals) - len(prev_locals)

        # chop_frame: 1–3 fewer locals
        if -3 <= diff < 0 and curr_locals == prev_locals[: len(curr_locals)]:
            return ChopFrameInfo(
                frame_type=251 + diff,  # 248, 249, or 250
                offset_delta=offset_delta,
            )

        # append_frame: 1–3 more locals
        if 0 < diff <= 3 and curr_locals[: len(prev_locals)] == prev_locals:
            return AppendFrameInfo(
                frame_type=251 + diff,  # 252, 253, or 254
                offset_delta=offset_delta,
                locals=list(curr_locals[len(prev_locals) :]),
            )

    # full_frame
    return FullFrameInfo(
        frame_type=255,
        offset_delta=offset_delta,
        number_of_locals=len(curr_locals),
        locals=list(curr_locals),
        number_of_stack_items=len(curr_stack),
        stack=list(curr_stack),
    )


# ===================================================================
# compute_maxs / compute_frames — public API
# ===================================================================


def compute_maxs(
    code: CodeModel,
    method: MethodModel,
    class_name: str,
    resolver: ClassResolver | None = None,
) -> tuple[int, int]:
    """Recompute ``max_stack`` and ``max_locals`` for a method's code.

    Builds a control-flow graph, runs forward dataflow simulation, and
    returns ``(max_stack, max_locals)``.

    Args:
        code: The code model to analyze.
        method: The method model (used for initial frame).
        class_name: JVM internal name of the enclosing class.
        resolver: Optional class hierarchy resolver for precise type merging.

    Returns:
        A ``(max_stack, max_locals)`` tuple.
    """
    cfg = build_cfg(code)
    result = simulate(cfg, code, method, class_name, resolver)
    return result.max_stack, result.max_locals


@dataclass(frozen=True, slots=True)
class FrameComputationResult:
    """Results of frame computation: limits and StackMapTable.

    Attributes:
        max_stack: Recomputed maximum operand stack depth.
        max_locals: Recomputed maximum local variable slot count.
        stack_map_table: Generated ``StackMapTable`` attribute, or ``None``
            when no frames are required (e.g. a linear method with no
            branches or exception handlers).
    """

    max_stack: int
    max_locals: int
    stack_map_table: StackMapTableAttr | None


def compute_frames(
    code: CodeModel,
    method: MethodModel,
    class_name: str,
    cp: ConstantPoolBuilder,
    label_offsets: dict[Label, int],
    resolver: ClassResolver | None = None,
) -> FrameComputationResult:
    """Recompute ``max_stack``, ``max_locals``, and ``StackMapTable`` frames.

    Builds a CFG, simulates stack/local states, then generates compact
    StackMapTable entries at every branch/exception-handler target
    (JVM spec §4.7.4).

    Args:
        code: The ``CodeModel`` whose frames to compute.
        method: The ``MethodModel`` owning this code (used for initial frame).
        class_name: Internal name of the enclosing class
            (e.g. ``"com/example/Foo"``).
        cp: ``ConstantPoolBuilder`` for allocating ``CONSTANT_Class`` entries
            referenced by ``ObjectVariableInfo``.
        label_offsets: Mapping from ``Label`` to resolved bytecode offset,
            as produced by ``resolve_labels()``.
        resolver: Optional class hierarchy resolver for precise type merging.

    Returns:
        A ``FrameComputationResult`` with ``max_stack``, ``max_locals``, and
        an optional ``StackMapTableAttr`` (``None`` if no frames are needed).
    """
    analysis_code = _prepare_analysis_code(code)
    analysis_label_offsets = label_offsets
    if analysis_code is not code:
        from ..edit.labels import resolve_labels

        analysis_label_offsets = resolve_labels(list(analysis_code.instructions), cp).label_offsets

    cfg = build_cfg(code)
    sim = simulate(cfg, analysis_code, method, class_name, resolver)

    if not cfg.blocks:
        return FrameComputationResult(
            max_stack=sim.max_stack,
            max_locals=sim.max_locals,
            stack_map_table=None,
        )

    # Identify blocks that need frames: every block except the entry block
    # that has an entry state (i.e., is reachable).
    entry_block_id = cfg.entry.id
    frame_targets: list[tuple[int, int]] = []  # (bytecode_offset, block_id)
    for block in cfg.blocks:
        if block.id == entry_block_id:
            continue
        if block.id not in sim.entry_states:
            continue
        if block.label is None:
            continue
        offset = label_offsets.get(block.label)
        if offset is None:
            continue
        frame_targets.append((offset, block.id))

    frame_targets.sort(key=lambda t: t[0])

    if not frame_targets:
        return FrameComputationResult(
            max_stack=sim.max_stack,
            max_locals=sim.max_locals,
            stack_map_table=None,
        )

    # Build the initial frame locals as the "previous" frame for delta computation.
    entry_frame = initial_frame(method, class_name)
    prev_locals = _vtypes_to_vtis(entry_frame.locals, cp, analysis_label_offsets)
    prev_offset = -1  # offset_delta for the first frame is (offset - 0)

    frames: list[StackMapFrameInfo] = []
    for offset, block_id in frame_targets:
        state = sim.entry_states[block_id]
        curr_locals = _vtypes_to_vtis(state.locals, cp, analysis_label_offsets)
        curr_stack = _vtypes_to_vtis(state.stack, cp, analysis_label_offsets)

        # offset_delta = offset - prev_offset - 1 for the first frame,
        # and offset - prev_offset - 1 for subsequent frames.
        offset_delta = offset - prev_offset - 1

        frame = _select_frame(offset_delta, prev_locals, curr_locals, curr_stack)
        frames.append(frame)

        prev_locals = curr_locals
        prev_offset = offset

    stack_map_table = StackMapTableAttr(
        attribute_name_index=cp.add_utf8("StackMapTable"),
        attribute_length=_stack_map_table_attribute_length(frames),
        number_of_entries=len(frames),
        entries=frames,
    )

    return FrameComputationResult(
        max_stack=sim.max_stack,
        max_locals=sim.max_locals,
        stack_map_table=stack_map_table,
    )


# ===================================================================
# Public API
# ===================================================================


__all__ = [
    # Errors
    "AnalysisError",
    "InvalidLocalError",
    "StackUnderflowError",
    "TypeMergeError",
    # Verification types
    "VDouble",
    "VFloat",
    "VInteger",
    "VLong",
    "VNull",
    "VObject",
    "VTop",
    "VType",
    "VUninitialized",
    "VUninitializedThis",
    # VType helpers
    "is_category2",
    "is_reference",
    "merge_vtypes",
    "vtype_from_descriptor",
    "vtype_from_field_descriptor_str",
    # Frame state
    "FrameState",
    "initial_frame",
    # Opcode metadata
    "OPCODE_EFFECTS",
    "OpcodeEffect",
    # CFG
    "BasicBlock",
    "ControlFlowGraph",
    "ExceptionEdge",
    "build_cfg",
    # Simulation
    "SimulationResult",
    "simulate",
    # Frame computation
    "FrameComputationResult",
    "compute_frames",
    "compute_maxs",
]
