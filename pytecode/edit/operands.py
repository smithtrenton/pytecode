"""Symbolic instruction operand wrappers for the editing model.

Provides editing-model instruction types that replace raw constant-pool
indexes and local-variable slot encodings with resolved symbolic values.
These types are used inside ``CodeModel.instructions`` and are lifted from
raw ``InsnInfo`` records during ``ClassModel.from_classfile()``, then
lowered back to spec-faithful ``InsnInfo`` records during
``to_classfile()``.

All wrapper types inherit from ``InsnInfo`` so the existing
``type CodeItem = InsnInfo | Label`` alias and ``_instruction_byte_size``
dispatch remain valid without changes to their signatures.

Covered instruction families:
    Constant-pool-backed: field access, method invocation, type operations,
    constant loading, invokedynamic, multianewarray.

    Local-variable-backed: all load/store families (including implicit
    ``_0``–``_3`` variants and ``WIDE`` forms), ``RET``, ``IINC``.

Out of scope (remain raw ``InsnInfo`` records):
    ``BIPUSH`` / ``SIPUSH`` — immediate integer values, no CP or slot
    reference.  ``NEWARRAY`` — primitive-type enum, no CP reference.
    No-operand instructions — nothing to symbolise.  Branch / switch
    instructions — already symbolic via ``labels.py``.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from ..classfile.instructions import (
    InsnInfo,
    InsnInfoType,
)

if TYPE_CHECKING:
    pass

__all__ = [
    "FieldInsn",
    "IIncInsn",
    "InterfaceMethodInsn",
    "InvokeDynamicInsn",
    "LdcClass",
    "LdcDouble",
    "LdcDynamic",
    "LdcFloat",
    "LdcInsn",
    "LdcInt",
    "LdcLong",
    "LdcMethodHandle",
    "LdcMethodType",
    "LdcString",
    "LdcValue",
    "MethodInsn",
    "MultiANewArrayInsn",
    "TypeInsn",
    "VarInsn",
]

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
    """Integer constant for ``ldc`` / ``ldc_w`` (CONSTANT_Integer, §4.4.4).

    Attributes:
        value: The raw unsigned 32-bit value (JVMS u4 ``bytes`` field).
    """

    value: int


@dataclass(frozen=True)
class LdcFloat:
    """Float constant for ``ldc`` / ``ldc_w`` (CONSTANT_Float, §4.4.4).

    Stored as a raw IEEE 754 bit pattern rather than a Python ``float`` to
    preserve NaN bit patterns and signed zeros exactly.

    Attributes:
        raw_bits: IEEE 754 single-precision bit pattern as an unsigned
            32-bit integer.
    """

    raw_bits: int


@dataclass(frozen=True)
class LdcLong:
    """Long constant for ``ldc2_w`` (CONSTANT_Long, §4.4.5).

    Attributes:
        value: The raw unsigned 64-bit value (JVMS u4 ``high_bytes`` << 32 | u4 ``low_bytes``).
    """

    value: int


@dataclass(frozen=True)
class LdcDouble:
    """Double constant for ``ldc2_w`` (CONSTANT_Double, §4.4.5).

    Stored as split high/low 32-bit words to preserve exact bit patterns.

    Attributes:
        high_bytes: Upper 32 bits of the IEEE 754 double-precision bit
            pattern.
        low_bytes: Lower 32 bits of the IEEE 754 double-precision bit
            pattern.
    """

    high_bytes: int
    low_bytes: int


@dataclass(frozen=True)
class LdcString:
    """String constant for ``ldc`` / ``ldc_w`` (CONSTANT_String, §4.4.3).

    Attributes:
        value: The string constant value.
    """

    value: str


@dataclass(frozen=True)
class LdcClass:
    """Class literal for ``ldc`` / ``ldc_w`` (CONSTANT_Class, §4.4.1).

    Attributes:
        name: JVM internal class name (e.g. ``java/lang/Object``).
    """

    name: str


@dataclass(frozen=True)
class LdcMethodType:
    """MethodType constant for ``ldc`` / ``ldc_w`` (CONSTANT_MethodType, §4.4.9).

    Attributes:
        descriptor: JVM method descriptor (e.g. ``(II)V``).
    """

    descriptor: str


@dataclass(frozen=True)
class LdcMethodHandle:
    """MethodHandle constant for ``ldc`` / ``ldc_w`` (CONSTANT_MethodHandle, §4.4.8).

    Attributes:
        reference_kind: Method handle behaviour kind (1–9, per
            JVMS Table 5.4.3.5-A).  Kinds 1–4 reference a Fieldref;
            5–8 reference a Methodref or InterfaceMethodref; 9 references
            an InterfaceMethodref only.
        owner: JVM internal name of the class owning the referenced member.
        name: Name of the referenced field or method.
        descriptor: JVM field or method descriptor of the referenced member.
        is_interface: Whether the owner is an interface type.  Controls
            emission of CONSTANT_InterfaceMethodref vs CONSTANT_Methodref.

    Raises:
        ValueError: If ``reference_kind`` is outside the [1, 9] range.
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
    """Dynamic constant for ``ldc`` / ``ldc_w`` (CONSTANT_Dynamic / condy, §4.4.10).

    Attributes:
        bootstrap_method_attr_index: Index into the ``BootstrapMethods``
            attribute (must fit the JVM ``u2`` range).
        name: Symbolic name of the dynamic constant.
        descriptor: JVM field descriptor of the produced value.

    Raises:
        ValueError: If ``bootstrap_method_attr_index`` exceeds the ``u2``
            range.
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
    """Symbolic instruction for field access (§6.5.getfield, §6.5.putfield, etc.).

    Wraps GETFIELD, PUTFIELD, GETSTATIC, and PUTSTATIC with resolved
    symbolic references instead of raw constant-pool indices.

    Attributes:
        owner: JVM internal name of the field's declaring class
            (e.g. ``java/lang/System``).
        name: Field name.
        descriptor: JVM field descriptor (e.g. ``I``,
            ``Ljava/lang/String;``).

    Raises:
        ValueError: If ``insn_type`` is not a field access opcode.
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
    """Symbolic instruction for method invocation (§6.5.invokevirtual, etc.).

    Wraps INVOKEVIRTUAL, INVOKESPECIAL, and INVOKESTATIC with resolved
    symbolic references.  Use ``InterfaceMethodInsn`` for INVOKEINTERFACE.

    Attributes:
        owner: JVM internal name of the method's declaring class or
            interface.
        name: Method name.
        descriptor: JVM method descriptor (e.g. ``(II)I``).
        is_interface: Whether ``owner`` is an interface type.  Controls
            emission of CONSTANT_InterfaceMethodref vs CONSTANT_Methodref
            (relevant for INVOKESTATIC / INVOKESPECIAL on interface
            methods since Java 8+).

    Raises:
        ValueError: If ``insn_type`` is not a supported method invocation
            opcode.
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
    """Symbolic instruction for INVOKEINTERFACE (§6.5.invokeinterface).

    The ``count`` operand (argument word count) is computed automatically
    during lowering from the method descriptor; callers do not set it.

    Attributes:
        owner: JVM internal name of the interface declaring the method.
        name: Method name.
        descriptor: JVM method descriptor.
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
    """Symbolic instruction for type operations (§6.5.new, §6.5.checkcast, etc.).

    Wraps NEW, CHECKCAST, INSTANCEOF, and ANEWARRAY with resolved symbolic
    class references.

    Attributes:
        class_name: JVM internal class name
            (e.g. ``java/lang/StringBuilder``).  For ANEWARRAY, the
            element type's internal name or descriptor.

    Raises:
        ValueError: If ``insn_type`` is not a type instruction opcode.
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
    """Symbolic instruction for local variable access (§6.5.iload, §6.5.astore, etc.).

    Normalises all implicit slot-encoded opcodes (``ILOAD_0``–``ASTORE_3``),
    standard explicit forms (``ILOAD`` through ``ASTORE``, ``RET``), and WIDE
    variants (``ILOADW``–``RETW``) into a single representation.

    The ``type`` field always holds the *canonical* non-WIDE base opcode
    (e.g. ``ILOAD``, not ``ILOAD_0`` or ``ILOADW``).

    Lowering selects the optimal encoding automatically:

    - slot 0–3 with a matching implicit form → implicit 1-byte opcode
    - slot 0–255 → explicit 2-byte form (opcode + u1)
    - slot 256–65535 → WIDE 4-byte form (WIDE prefix + opcode + u2)

    RET has no implicit forms, but supports WIDE encoding for slots > 255.

    Attributes:
        slot: Local variable table index (``u2`` range, 0–65535).

    Raises:
        ValueError: If ``insn_type`` is not a local variable opcode or
            ``slot`` exceeds the ``u2`` range.
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
    """Symbolic instruction for IINC / IINCW (§6.5.iinc).

    Normalises both the standard and WIDE forms.  Lowering selects the
    appropriate encoding:

    - slot 0–255 and increment fits in i1 (–128..127) → standard 3-byte form
    - slot > 255 or increment outside i1 range → WIDE 6-byte form

    Attributes:
        slot: Local variable table index (``u2`` range, 0–65535).
        increment: Signed increment value (``i2`` range, –32768..32767).

    Raises:
        ValueError: If ``slot`` exceeds the ``u2`` range or ``increment``
            exceeds the ``i2`` range.
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
    """Symbolic instruction for constant loading (§6.5.ldc, §6.5.ldc_w, §6.5.ldc2_w).

    Lowering selects the minimal encoding: ``ldc`` (2 bytes) when the
    constant-pool index fits in one byte (≤ 255), ``ldc_w`` (3 bytes)
    otherwise, for single-slot constants (int, float, string, class,
    method-type, method-handle, dynamic).  Double-slot constants (long,
    double) always use ``ldc2_w`` (3 bytes).

    Attributes:
        value: Tagged constant determining the constant-pool entry type.
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
    """Symbolic instruction for INVOKEDYNAMIC (§6.5.invokedynamic).

    Attributes:
        bootstrap_method_attr_index: Index into the ``BootstrapMethods``
            attribute (must fit the JVM ``u2`` range).
        name: Symbolic method name resolved via the bootstrap method.
        descriptor: JVM method descriptor of the call site.

    Raises:
        ValueError: If ``bootstrap_method_attr_index`` exceeds the ``u2``
            range.
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
    """Symbolic instruction for MULTIANEWARRAY (§6.5.multianewarray).

    Attributes:
        class_name: JVM internal name of the array type
            (e.g. ``[[Ljava/lang/String;`` for ``String[][]``).
        dimensions: Number of dimensions to allocate (``u1`` range, 1–255).

    Raises:
        ValueError: If ``dimensions`` is outside the [1, 255] range.
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
