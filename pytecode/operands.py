"""Symbolic instruction operand wrappers for the editing model.

Provides editing-model instruction types that replace raw constant-pool indexes
and local-variable slot encodings with resolved symbolic values.  These types
are used inside ``CodeModel.instructions`` and are lifted from raw
``InsnInfo`` records during ``ClassModel.from_classfile()`` then lowered back
to spec-faithful ``InsnInfo`` records during ``to_classfile()``.

All wrapper types inherit from ``InsnInfo`` so the existing
``type CodeItem = InsnInfo | Label`` alias and ``_instruction_byte_size``
dispatch remain valid without changes to their signatures.

Scope
-----
- Constant-pool-backed instructions: field access, method invocation, type
  operations, constant loading, invokedynamic, multianewarray.
- Local-variable-backed instructions: all load/store families (including
  implicit ``_0``–``_3`` variants and ``WIDE`` forms), ``RET``, ``IINC``.

Out of scope (remain raw ``InsnInfo`` records)
----------------------------------------------
- ``BIPUSH`` / ``SIPUSH`` — immediate integer values, no CP or slot reference.
- ``NEWARRAY`` — primitive-type enum, no CP reference.
- No-operand instructions — nothing to symbolise.
- Branch / switch instructions — already symbolic via ``labels.py``.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from .instructions import (
    InsnInfo,
    InsnInfoType,
)

if TYPE_CHECKING:
    pass

# ---------------------------------------------------------------------------
# Opcode classification sets (used for validation in __init__)
# ---------------------------------------------------------------------------

_FIELD_OPCODES: frozenset[InsnInfoType] = frozenset(
    {
        InsnInfoType.GETFIELD,
        InsnInfoType.PUTFIELD,
        InsnInfoType.GETSTATIC,
        InsnInfoType.PUTSTATIC,
    }
)

_METHOD_OPCODES: frozenset[InsnInfoType] = frozenset(
    {
        InsnInfoType.INVOKEVIRTUAL,
        InsnInfoType.INVOKESPECIAL,
        InsnInfoType.INVOKESTATIC,
    }
)

_TYPE_OPCODES: frozenset[InsnInfoType] = frozenset(
    {
        InsnInfoType.NEW,
        InsnInfoType.CHECKCAST,
        InsnInfoType.INSTANCEOF,
        InsnInfoType.ANEWARRAY,
    }
)

# All opcodes that normalize into VarInsn, keyed for fast lookup.
# Explicit-index forms: ILOAD, LLOAD, FLOAD, DLOAD, ALOAD, ISTORE, LSTORE,
#   FSTORE, DSTORE, ASTORE, RET (LocalIndex); and their WIDE counterparts
#   (LocalIndexW).
_VAR_EXPLICIT_OPCODES: frozenset[InsnInfoType] = frozenset(
    {
        InsnInfoType.ILOAD,
        InsnInfoType.LLOAD,
        InsnInfoType.FLOAD,
        InsnInfoType.DLOAD,
        InsnInfoType.ALOAD,
        InsnInfoType.ISTORE,
        InsnInfoType.LSTORE,
        InsnInfoType.FSTORE,
        InsnInfoType.DSTORE,
        InsnInfoType.ASTORE,
        InsnInfoType.RET,
    }
)

# Base opcodes that are valid canonical types for VarInsn (accepted in __init__)
_VAR_BASE_OPCODES: frozenset[InsnInfoType] = _VAR_EXPLICIT_OPCODES

# ---------------------------------------------------------------------------
# Implicit-slot variant mapping
# Maps each _N opcode → (canonical_opcode, slot)
# ---------------------------------------------------------------------------

_IMPLICIT_VAR_SLOTS: dict[InsnInfoType, tuple[InsnInfoType, int]] = {
    # ILOAD_0..3
    InsnInfoType.ILOAD_0: (InsnInfoType.ILOAD, 0),
    InsnInfoType.ILOAD_1: (InsnInfoType.ILOAD, 1),
    InsnInfoType.ILOAD_2: (InsnInfoType.ILOAD, 2),
    InsnInfoType.ILOAD_3: (InsnInfoType.ILOAD, 3),
    # LLOAD_0..3
    InsnInfoType.LLOAD_0: (InsnInfoType.LLOAD, 0),
    InsnInfoType.LLOAD_1: (InsnInfoType.LLOAD, 1),
    InsnInfoType.LLOAD_2: (InsnInfoType.LLOAD, 2),
    InsnInfoType.LLOAD_3: (InsnInfoType.LLOAD, 3),
    # FLOAD_0..3
    InsnInfoType.FLOAD_0: (InsnInfoType.FLOAD, 0),
    InsnInfoType.FLOAD_1: (InsnInfoType.FLOAD, 1),
    InsnInfoType.FLOAD_2: (InsnInfoType.FLOAD, 2),
    InsnInfoType.FLOAD_3: (InsnInfoType.FLOAD, 3),
    # DLOAD_0..3
    InsnInfoType.DLOAD_0: (InsnInfoType.DLOAD, 0),
    InsnInfoType.DLOAD_1: (InsnInfoType.DLOAD, 1),
    InsnInfoType.DLOAD_2: (InsnInfoType.DLOAD, 2),
    InsnInfoType.DLOAD_3: (InsnInfoType.DLOAD, 3),
    # ALOAD_0..3
    InsnInfoType.ALOAD_0: (InsnInfoType.ALOAD, 0),
    InsnInfoType.ALOAD_1: (InsnInfoType.ALOAD, 1),
    InsnInfoType.ALOAD_2: (InsnInfoType.ALOAD, 2),
    InsnInfoType.ALOAD_3: (InsnInfoType.ALOAD, 3),
    # ISTORE_0..3
    InsnInfoType.ISTORE_0: (InsnInfoType.ISTORE, 0),
    InsnInfoType.ISTORE_1: (InsnInfoType.ISTORE, 1),
    InsnInfoType.ISTORE_2: (InsnInfoType.ISTORE, 2),
    InsnInfoType.ISTORE_3: (InsnInfoType.ISTORE, 3),
    # LSTORE_0..3
    InsnInfoType.LSTORE_0: (InsnInfoType.LSTORE, 0),
    InsnInfoType.LSTORE_1: (InsnInfoType.LSTORE, 1),
    InsnInfoType.LSTORE_2: (InsnInfoType.LSTORE, 2),
    InsnInfoType.LSTORE_3: (InsnInfoType.LSTORE, 3),
    # FSTORE_0..3
    InsnInfoType.FSTORE_0: (InsnInfoType.FSTORE, 0),
    InsnInfoType.FSTORE_1: (InsnInfoType.FSTORE, 1),
    InsnInfoType.FSTORE_2: (InsnInfoType.FSTORE, 2),
    InsnInfoType.FSTORE_3: (InsnInfoType.FSTORE, 3),
    # DSTORE_0..3
    InsnInfoType.DSTORE_0: (InsnInfoType.DSTORE, 0),
    InsnInfoType.DSTORE_1: (InsnInfoType.DSTORE, 1),
    InsnInfoType.DSTORE_2: (InsnInfoType.DSTORE, 2),
    InsnInfoType.DSTORE_3: (InsnInfoType.DSTORE, 3),
    # ASTORE_0..3
    InsnInfoType.ASTORE_0: (InsnInfoType.ASTORE, 0),
    InsnInfoType.ASTORE_1: (InsnInfoType.ASTORE, 1),
    InsnInfoType.ASTORE_2: (InsnInfoType.ASTORE, 2),
    InsnInfoType.ASTORE_3: (InsnInfoType.ASTORE, 3),
}

# Reverse: (canonical_opcode, slot) → implicit opcode (for lowering)
_VAR_SHORTCUTS: dict[tuple[InsnInfoType, int], InsnInfoType] = {v: k for k, v in _IMPLICIT_VAR_SLOTS.items()}

# WIDE opcode → canonical base opcode
_WIDE_TO_BASE: dict[InsnInfoType, InsnInfoType] = {
    InsnInfoType.ILOADW: InsnInfoType.ILOAD,
    InsnInfoType.LLOADW: InsnInfoType.LLOAD,
    InsnInfoType.FLOADW: InsnInfoType.FLOAD,
    InsnInfoType.DLOADW: InsnInfoType.DLOAD,
    InsnInfoType.ALOADW: InsnInfoType.ALOAD,
    InsnInfoType.ISTOREW: InsnInfoType.ISTORE,
    InsnInfoType.LSTOREW: InsnInfoType.LSTORE,
    InsnInfoType.FSTOREW: InsnInfoType.FSTORE,
    InsnInfoType.DSTOREW: InsnInfoType.DSTORE,
    InsnInfoType.ASTOREW: InsnInfoType.ASTORE,
    InsnInfoType.RETW: InsnInfoType.RET,
}

# Canonical base opcode → WIDE opcode (for lowering)
_BASE_TO_WIDE: dict[InsnInfoType, InsnInfoType] = {v: k for k, v in _WIDE_TO_BASE.items()}

_U1_MAX = 0xFF
_U2_MAX = 0xFFFF
_I2_MIN = -(1 << 15)
_I2_MAX = (1 << 15) - 1


def _require_u2(value: int, *, context: str) -> int:
    if not 0 <= value <= _U2_MAX:
        raise ValueError(f"{context} must be in range [0, {_U2_MAX}], got {value}")
    return value


def _require_i2(value: int, *, context: str) -> int:
    if not _I2_MIN <= value <= _I2_MAX:
        raise ValueError(f"{context} must be in range [{_I2_MIN}, {_I2_MAX}], got {value}")
    return value


def _require_u1(value: int, *, context: str, minimum: int = 0) -> int:
    if not minimum <= value <= _U1_MAX:
        raise ValueError(f"{context} must be in range [{minimum}, {_U1_MAX}], got {value}")
    return value


# ---------------------------------------------------------------------------
# LDC value types (frozen dataclasses)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class LdcInt:
    """Integer constant for LDC (CONSTANT_Integer)."""

    value: int


@dataclass(frozen=True)
class LdcFloat:
    """Float constant for LDC (CONSTANT_Float, raw IEEE 754 bit pattern)."""

    raw_bits: int


@dataclass(frozen=True)
class LdcLong:
    """Long constant for LDC2_W (CONSTANT_Long)."""

    value: int


@dataclass(frozen=True)
class LdcDouble:
    """Double constant for LDC2_W (CONSTANT_Double, high/low 32-bit words)."""

    high_bytes: int
    low_bytes: int


@dataclass(frozen=True)
class LdcString:
    """String constant for LDC (CONSTANT_String)."""

    value: str


@dataclass(frozen=True)
class LdcClass:
    """Class literal for LDC (CONSTANT_Class, JVM internal name)."""

    name: str


@dataclass(frozen=True)
class LdcMethodType:
    """MethodType constant for LDC (CONSTANT_MethodType)."""

    descriptor: str


@dataclass(frozen=True)
class LdcMethodHandle:
    """MethodHandle constant for LDC (CONSTANT_MethodHandle).

    ``owner``, ``name``, and ``descriptor`` describe the referenced member.
    ``is_interface`` distinguishes CONSTANT_InterfaceMethodref targets from
    CONSTANT_Methodref targets (reference kinds 1–4 use Fieldref; 5–8 use
    Methodref or InterfaceMethodref; 9 uses InterfaceMethodref only).
    """

    reference_kind: int
    owner: str
    name: str
    descriptor: str
    is_interface: bool = False

    def __post_init__(self) -> None:
        if not 1 <= self.reference_kind <= 9:
            raise ValueError(f"reference_kind must be in range [1, 9], got {self.reference_kind}")


@dataclass(frozen=True)
class LdcDynamic:
    """Dynamic constant for LDC (CONSTANT_Dynamic / condy).

    ``bootstrap_method_attr_index`` must fit the JVM ``u2`` range.
    """

    bootstrap_method_attr_index: int
    name: str
    descriptor: str

    def __post_init__(self) -> None:
        _require_u2(
            self.bootstrap_method_attr_index,
            context="bootstrap_method_attr_index",
        )


type LdcValue = (
    LdcInt | LdcFloat | LdcLong | LdcDouble | LdcString | LdcClass | LdcMethodType | LdcMethodHandle | LdcDynamic
)

# ---------------------------------------------------------------------------
# Symbolic instruction wrapper types
# ---------------------------------------------------------------------------


@dataclass(init=False)
class FieldInsn(InsnInfo):
    """Editing-model instruction for field access (GET/PUT FIELD/STATIC).

    ``owner`` is the JVM internal class name (e.g. ``java/lang/System``).
    ``descriptor`` is the JVM field descriptor (e.g. ``I``, ``Ljava/lang/String;``).
    """

    owner: str
    name: str
    descriptor: str

    def __init__(
        self,
        insn_type: InsnInfoType,
        owner: str,
        name: str,
        descriptor: str,
        bytecode_offset: int = -1,
    ) -> None:
        if insn_type not in _FIELD_OPCODES:
            raise ValueError(f"{insn_type.name} is not a field access opcode")
        super().__init__(insn_type, bytecode_offset)
        self.owner = owner
        self.name = name
        self.descriptor = descriptor


@dataclass(init=False)
class MethodInsn(InsnInfo):
    """Editing-model instruction for method invocation (INVOKE{VIRTUAL,SPECIAL,STATIC}).

    ``owner`` is the JVM internal class or interface name.
    ``descriptor`` is the JVM method descriptor.
    ``is_interface`` must be ``True`` when ``owner`` is an interface; this
    controls whether a ``CONSTANT_InterfaceMethodref`` or ``CONSTANT_Methodref``
    entry is emitted in the constant pool (relevant for INVOKESTATIC and
    INVOKESPECIAL on interface methods since Java 8+).
    """

    owner: str
    name: str
    descriptor: str
    is_interface: bool

    def __init__(
        self,
        insn_type: InsnInfoType,
        owner: str,
        name: str,
        descriptor: str,
        is_interface: bool = False,
        bytecode_offset: int = -1,
    ) -> None:
        if insn_type not in _METHOD_OPCODES:
            raise ValueError(
                f"{insn_type.name} is not a method invocation opcode (use InterfaceMethodInsn for INVOKEINTERFACE)"
            )
        super().__init__(insn_type, bytecode_offset)
        self.owner = owner
        self.name = name
        self.descriptor = descriptor
        self.is_interface = is_interface


@dataclass(init=False)
class InterfaceMethodInsn(InsnInfo):
    """Editing-model instruction for INVOKEINTERFACE.

    The ``count`` (argument word count) is computed automatically during
    lowering from the method descriptor; callers do not set it.
    """

    owner: str
    name: str
    descriptor: str

    def __init__(
        self,
        owner: str,
        name: str,
        descriptor: str,
        bytecode_offset: int = -1,
    ) -> None:
        super().__init__(InsnInfoType.INVOKEINTERFACE, bytecode_offset)
        self.owner = owner
        self.name = name
        self.descriptor = descriptor


@dataclass(init=False)
class TypeInsn(InsnInfo):
    """Editing-model instruction for type-based operations (NEW, CHECKCAST, INSTANCEOF, ANEWARRAY).

    ``class_name`` is the JVM internal class name (e.g. ``java/lang/StringBuilder``).
    For ANEWARRAY, it is the element type's internal name or descriptor
    (e.g. ``java/lang/String`` for ``String[]``).
    """

    class_name: str

    def __init__(
        self,
        insn_type: InsnInfoType,
        class_name: str,
        bytecode_offset: int = -1,
    ) -> None:
        if insn_type not in _TYPE_OPCODES:
            raise ValueError(f"{insn_type.name} is not a type instruction opcode")
        super().__init__(insn_type, bytecode_offset)
        self.class_name = class_name


@dataclass(init=False)
class VarInsn(InsnInfo):
    """Editing-model instruction for local variable access.

    Normalises all implicit slot-encoded opcodes (``ILOAD_0``–``ASTORE_3``),
    standard explicit forms (``ILOAD`` through ``ASTORE``, ``RET``), and WIDE
    variants (``ILOADW``–``RETW``) into a single representation.

    The ``type`` field always holds the *canonical* non-WIDE base opcode
    (e.g. ``ILOAD``, not ``ILOAD_0`` or ``ILOADW``).

    Lowering selects the optimal encoding automatically:
    - slot 0–3 with a matching implicit form → implicit 1-byte opcode
    - slot 0–255 → explicit 2-byte form (opcode + u1)
    - slot 256–65535 → WIDE 4-byte form (WIDE prefix + opcode + u2)

    RET has no implicit forms, but it does support WIDE encoding for slots > 255.
    """

    slot: int

    def __init__(
        self,
        insn_type: InsnInfoType,
        slot: int,
        bytecode_offset: int = -1,
    ) -> None:
        if insn_type not in _VAR_BASE_OPCODES:
            raise ValueError(f"{insn_type.name} is not a local variable instruction opcode")
        super().__init__(insn_type, bytecode_offset)
        self.slot = _require_u2(slot, context="local variable slot")


@dataclass(init=False)
class IIncInsn(InsnInfo):
    """Editing-model instruction for IINC / IINCW.

    Normalises both the standard and WIDE forms.  Lowering selects the
    appropriate encoding:
    - slot 0–255 and increment fits in i1 (–128..127) → standard 3-byte form
    - slot > 255 or increment outside i1 range → WIDE 6-byte form

    ``slot`` must fit the JVM ``u2`` range and ``increment`` must fit ``i2``.
    """

    slot: int
    increment: int

    def __init__(
        self,
        slot: int,
        increment: int,
        bytecode_offset: int = -1,
    ) -> None:
        super().__init__(InsnInfoType.IINC, bytecode_offset)
        self.slot = _require_u2(slot, context="local variable slot")
        self.increment = _require_i2(increment, context="iinc increment")


@dataclass(init=False)
class LdcInsn(InsnInfo):
    """Editing-model instruction for loading a constant (LDC / LDC_W / LDC2_W).

    ``value`` is a tagged union over all supported constant types.  Lowering
    selects the minimal encoding: ``LDC`` (2 bytes) when the constant-pool index
    fits in one byte (≤ 255), ``LDC_W`` (3 bytes) otherwise, for single-category
    constants (int, float, string, class, method-type, method-handle, dynamic).
    Double-category constants (long, double) always use ``LDC2_W`` (3 bytes).
    """

    value: LdcValue

    def __init__(
        self,
        value: LdcValue,
        bytecode_offset: int = -1,
    ) -> None:
        super().__init__(InsnInfoType.LDC_W, bytecode_offset)
        self.value = value


@dataclass(init=False)
class InvokeDynamicInsn(InsnInfo):
    """Editing-model instruction for INVOKEDYNAMIC.

    ``bootstrap_method_attr_index`` references an entry in the
    ``BootstrapMethods`` attribute and must fit the JVM ``u2`` range.
    """

    bootstrap_method_attr_index: int
    name: str
    descriptor: str

    def __init__(
        self,
        bootstrap_method_attr_index: int,
        name: str,
        descriptor: str,
        bytecode_offset: int = -1,
    ) -> None:
        super().__init__(InsnInfoType.INVOKEDYNAMIC, bytecode_offset)
        self.bootstrap_method_attr_index = _require_u2(
            bootstrap_method_attr_index,
            context="bootstrap_method_attr_index",
        )
        self.name = name
        self.descriptor = descriptor


@dataclass(init=False)
class MultiANewArrayInsn(InsnInfo):
    """Editing-model instruction for MULTIANEWARRAY.

    ``class_name`` is the JVM internal name of the array type
    (e.g. ``[[Ljava/lang/String;`` for ``String[][]``). ``dimensions`` must be
    in the JVM ``u1`` range ``[1, 255]``.
    """

    class_name: str
    dimensions: int

    def __init__(
        self,
        class_name: str,
        dimensions: int,
        bytecode_offset: int = -1,
    ) -> None:
        super().__init__(InsnInfoType.MULTIANEWARRAY, bytecode_offset)
        self.class_name = class_name
        self.dimensions = _require_u1(
            dimensions,
            context="multianewarray dimensions",
            minimum=1,
        )
