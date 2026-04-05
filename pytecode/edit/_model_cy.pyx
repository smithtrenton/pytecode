# cython: boundscheck=False, wraparound=False, cdivision=True
"""Mutable editing models for JVM class files.

Provides Cython extension-type wrappers (``ClassModel``, ``MethodModel``,
``FieldModel``, ``CodeModel``) that resolve raw constant-pool indexes
into symbolic string references, making class-file content easy to
inspect and transform programmatically.
"""

import copy
from typing import TYPE_CHECKING

__all__ = ["ClassModel", "CodeModel", "FieldModel", "MethodModel"]

from ..classfile._instructions_cy cimport (
    Branch as CBranch,
    BranchW as CBranchW,
    ConstPoolIndex as CConstPoolIndex,
    IInc as CIInc,
    IIncW as CIIncW,
    InvokeDynamic as CInvokeDynamic,
    InvokeInterface as CInvokeInterface,
    LocalIndex as CLocalIndex,
    LocalIndexW as CLocalIndexW,
    LookupSwitch as CLookupSwitch,
    MatchOffsetPair as CMatchOffsetPair,
    MultiANewArray as CMultiANewArray,
    TableSwitch as CTableSwitch,
)
from ._labels_cy cimport Label as CLabel
from ..classfile.attributes import (
    AttributeInfo,
    CodeAttr,
    LineNumberTableAttr,
    LocalVariableTableAttr,
    LocalVariableTypeTableAttr,
)
from ..classfile.constant_pool import (
    ClassInfo,
    DoubleInfo,
    DynamicInfo,
    FieldrefInfo,
    FloatInfo,
    IntegerInfo,
    InterfaceMethodrefInfo,
    InvokeDynamicInfo,
    LongInfo,
    MethodHandleInfo,
    MethodrefInfo,
    MethodTypeInfo,
    NameAndTypeInfo,
    StringInfo,
)
from ..classfile.constants import MAGIC, ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from ..classfile.info import ClassFile, FieldInfo, MethodInfo
from ..classfile.instructions import (
    Branch,
    BranchW,
    ConstPoolIndex,
    IInc,
    IIncW,
    InsnInfo,
    InsnInfoType,
    InvokeDynamic,
    InvokeInterface,
    LocalIndex,
    LocalIndexW,
    LookupSwitch,
    MultiANewArray,
    TableSwitch,
)
from ..classfile.reader import ClassReader
from ..classfile.writer import ClassWriter
from ._attribute_clone import clone_attribute, clone_attributes
from .constant_pool_builder import ConstantPoolBuilder
from .debug_info import (
    DebugInfoPolicy,
    DebugInfoState,
    is_class_debug_info_stale,
    normalize_debug_info_policy,
    skip_debug_method_attributes,
    strip_class_debug_attributes,
)
from .labels import (
    BranchInsn,
    CodeItem,
    ExceptionHandler,
    Label,
    LineNumberEntry,
    LocalVariableEntry,
    LocalVariableTypeEntry,
    LookupSwitchInsn,
    TableSwitchInsn,
    clone_raw_instruction,
    lower_code,
    resolve_catch_type,
)
from .operands import (
    _IMPLICIT_VAR_SLOTS,
    _WIDE_TO_BASE,
    FieldInsn,
    IIncInsn,
    InterfaceMethodInsn,
    InvokeDynamicInsn,
    LdcClass,
    LdcDouble,
    LdcDynamic,
    LdcFloat,
    LdcInsn,
    LdcInt,
    LdcLong,
    LdcMethodHandle,
    LdcMethodType,
    LdcString,
    LdcValue,
    MethodInsn,
    MultiANewArrayInsn,
    TypeInsn,
    VarInsn,
)

if TYPE_CHECKING:
    from ..analysis.hierarchy import ClassResolver


_BRANCH_TARGET_CONTEXTS = {
    insn_type: f"{insn_type.name} target"
    for insn_type in InsnInfoType
    if insn_type.instinfo in (Branch, BranchW)
}
_trusted_branch_insn = BranchInsn._trusted
_trusted_lookup_switch_insn = LookupSwitchInsn._trusted
_trusted_table_switch_insn = TableSwitchInsn._trusted
_trusted_field_insn = FieldInsn._trusted
_trusted_method_insn = MethodInsn._trusted
_trusted_interface_method_insn = InterfaceMethodInsn._trusted
_trusted_invoke_dynamic_insn = InvokeDynamicInsn._trusted
_trusted_ldc_insn = LdcInsn._trusted
_trusted_multi_anew_array_insn = MultiANewArrayInsn._trusted
_trusted_type_insn = TypeInsn._trusted
_trusted_var_insn = VarInsn._trusted
_trusted_iinc_insn = IIncInsn._trusted
_MISSING = object()


def _repr_fields(str class_name, tuple fields):
    return f"{class_name}(" + ", ".join(f"{name}={value!r}" for name, value in fields) + ")"


cdef class _ModelBase:
    def _init_values(self):
        raise NotImplementedError

    def _compare_values(self):
        return self._init_values()

    def _repr_items(self):
        raise NotImplementedError

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._repr_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._compare_values() == other._compare_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._init_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._init_values(), memo))

    def __reduce__(self):
        return type(self), self._init_values()


cdef class CodeModel(_ModelBase):
    """Mutable wrapper around a method's Code attribute (JVMS §4.7.3).

    Carries a mixed instruction stream of raw opcodes and ``Label``
    pseudo-instructions, symbolic exception/debug metadata, and
    stack/local limits lifted from the parsed ``CodeAttr``.

    Attributes:
        max_stack: Maximum operand-stack depth for this method.
        max_locals: Maximum number of local-variable slots.
        instructions: Interleaved opcodes and ``Label`` pseudo-instructions.
        exception_handlers: Symbolic exception-table entries.
        line_numbers: Line-number debug entries (may be empty).
        local_variables: LocalVariableTable debug entries (may be empty).
        local_variable_types: LocalVariableTypeTable debug entries
            (may be empty).
        attributes: Non-lifted Code sub-attributes (e.g. StackMapTable).
        debug_info_state: Tracks whether debug tables are fresh or stale.
    """

    cdef public Py_ssize_t max_stack
    cdef public Py_ssize_t max_locals
    cdef public list instructions
    cdef public list exception_handlers
    cdef public list line_numbers
    cdef public list local_variables
    cdef public list local_variable_types
    cdef public list attributes
    cdef public object _nested_attribute_layout
    cdef public object debug_info_state

    def __init__(
        self,
        Py_ssize_t max_stack,
        Py_ssize_t max_locals,
        object instructions=_MISSING,
        object exception_handlers=_MISSING,
        object line_numbers=_MISSING,
        object local_variables=_MISSING,
        object local_variable_types=_MISSING,
        object attributes=_MISSING,
        object _nested_attribute_layout=(),
        object debug_info_state=DebugInfoState.FRESH,
    ):
        self.max_stack = max_stack
        self.max_locals = max_locals
        self.instructions = [] if instructions is _MISSING else instructions
        self.exception_handlers = [] if exception_handlers is _MISSING else exception_handlers
        self.line_numbers = [] if line_numbers is _MISSING else line_numbers
        self.local_variables = [] if local_variables is _MISSING else local_variables
        self.local_variable_types = [] if local_variable_types is _MISSING else local_variable_types
        self.attributes = [] if attributes is _MISSING else attributes
        self._nested_attribute_layout = _nested_attribute_layout
        self.debug_info_state = debug_info_state

    def _init_values(self):
        return (
            self.max_stack,
            self.max_locals,
            self.instructions,
            self.exception_handlers,
            self.line_numbers,
            self.local_variables,
            self.local_variable_types,
            self.attributes,
            self._nested_attribute_layout,
            self.debug_info_state,
        )

    def _compare_values(self):
        return (
            self.max_stack,
            self.max_locals,
            self.instructions,
            self.exception_handlers,
            self.line_numbers,
            self.local_variables,
            self.local_variable_types,
            self.attributes,
            self.debug_info_state,
        )

    def _repr_items(self):
        return (
            ("max_stack", self.max_stack),
            ("max_locals", self.max_locals),
            ("instructions", self.instructions),
            ("exception_handlers", self.exception_handlers),
            ("line_numbers", self.line_numbers),
            ("local_variables", self.local_variables),
            ("local_variable_types", self.local_variable_types),
            ("attributes", self.attributes),
            ("debug_info_state", self.debug_info_state),
        )


cdef class FieldModel(_ModelBase):
    """Mutable wrapper around a class field (JVMS §4.5).

    Attributes:
        access_flags: Field access and property flags.
        name: Unqualified field name.
        descriptor: Field descriptor (e.g. ``I``, ``Ljava/lang/String;``).
        attributes: Raw field-level attributes (e.g. ConstantValue).
    """

    cdef public object access_flags
    cdef public object name
    cdef public object descriptor
    cdef public list attributes

    def __init__(self, object access_flags, object name, object descriptor, object attributes):
        self.access_flags = access_flags
        self.name = name
        self.descriptor = descriptor
        self.attributes = attributes

    def _init_values(self):
        return (self.access_flags, self.name, self.descriptor, self.attributes)

    def _repr_items(self):
        return (
            ("access_flags", self.access_flags),
            ("name", self.name),
            ("descriptor", self.descriptor),
            ("attributes", self.attributes),
        )


cdef class MethodModel(_ModelBase):
    """Mutable wrapper around a class method (JVMS §4.6).

    The ``code`` field is ``None`` for abstract and native methods.
    The ``attributes`` list holds non-Code attributes only; the Code
    attribute is lifted into the ``code`` field.

    Attributes:
        access_flags: Method access and property flags.
        name: Unqualified method name (or ``<init>``/``<clinit>``).
        descriptor: Method descriptor (e.g. ``(II)V``).
        code: Lifted Code attribute, or ``None`` for abstract/native methods.
        attributes: Non-Code method-level attributes (e.g. Exceptions).
    """

    cdef public object access_flags
    cdef public object name
    cdef public object descriptor
    cdef public object code
    cdef public list attributes

    def __init__(self, object access_flags, object name, object descriptor, object code, object attributes):
        self.access_flags = access_flags
        self.name = name
        self.descriptor = descriptor
        self.code = code
        self.attributes = attributes

    def _init_values(self):
        return (self.access_flags, self.name, self.descriptor, self.code, self.attributes)

    def _repr_items(self):
        return (
            ("access_flags", self.access_flags),
            ("name", self.name),
            ("descriptor", self.descriptor),
            ("code", self.code),
            ("attributes", self.attributes),
        )


cdef class ClassModel(_ModelBase):
    """Mutable editing model for a JVM class file (JVMS §4.1).

    Fields use symbolic (resolved) references — plain strings for class
    names, field/method names, and descriptors — instead of raw
    constant-pool indexes.

    A ``ConstantPoolBuilder`` is carried so that raw attributes and
    instructions (which still contain CP indexes) remain valid.  The
    builder is seeded from the original constant pool during
    ``from_classfile`` and can be used to allocate new entries.

    Attributes:
        version: ``(major, minor)`` class-file version pair.
        access_flags: Class access and property flags.
        name: Internal class name (e.g. ``java/lang/Object``).
        super_name: Internal name of the superclass, or ``None`` for
            ``java/lang/Object`` itself.
        interfaces: Internal names of directly implemented interfaces.
        fields: Mutable field models.
        methods: Mutable method models.
        attributes: Class-level attributes (e.g. SourceFile, InnerClasses).
        constant_pool: Builder for allocating or resolving CP entries.
        debug_info_state: Tracks whether debug metadata is fresh or stale.
    """

    cdef public object version
    cdef public object access_flags
    cdef public object name
    cdef public object super_name
    cdef public list interfaces
    cdef public list fields
    cdef public list methods
    cdef public list attributes
    cdef public object constant_pool
    cdef public object debug_info_state

    def __init__(
        self,
        object version,
        object access_flags,
        object name,
        object super_name,
        object interfaces,
        object fields,
        object methods,
        object attributes,
        object constant_pool=_MISSING,
        object debug_info_state=DebugInfoState.FRESH,
    ):
        self.version = version
        self.access_flags = access_flags
        self.name = name
        self.super_name = super_name
        self.interfaces = interfaces
        self.fields = fields
        self.methods = methods
        self.attributes = attributes
        self.constant_pool = ConstantPoolBuilder() if constant_pool is _MISSING else constant_pool
        self.debug_info_state = debug_info_state

    def _init_values(self):
        return (
            self.version,
            self.access_flags,
            self.name,
            self.super_name,
            self.interfaces,
            self.fields,
            self.methods,
            self.attributes,
            self.constant_pool,
            self.debug_info_state,
        )

    def _repr_items(self):
        return (
            ("version", self.version),
            ("access_flags", self.access_flags),
            ("name", self.name),
            ("super_name", self.super_name),
            ("interfaces", self.interfaces),
            ("fields", self.fields),
            ("methods", self.methods),
            ("attributes", self.attributes),
            ("constant_pool", self.constant_pool),
            ("debug_info_state", self.debug_info_state),
        )

    # ------------------------------------------------------------------
    # Construction helpers
    # ------------------------------------------------------------------

    @classmethod
    def from_classfile(cls, object cf, *, bint skip_debug=False):
        """Build a ``ClassModel`` from a parsed ``ClassFile``.

        Resolves constant-pool indexes to symbolic string values and
        seeds the ``ConstantPoolBuilder`` from the original pool so that
        raw attributes and instructions remain valid.

        Args:
            cf: Parsed class-file structure to lift.
            skip_debug: If true, strip debug metadata (line numbers,
                local-variable tables, source-file attributes) during
                lifting.

        Returns:
            A fully resolved ``ClassModel``.

        Raises:
            ValueError: If a constant-pool entry has an unexpected type.
        """
        cp = ConstantPoolBuilder.from_pool(cf.constant_pool)

        # Resolve this_class → class name string.
        this_class_entry = cp.peek(cf.this_class)
        if type(this_class_entry) is not ClassInfo:
            raise ValueError(f"this_class CP index {cf.this_class} is not a CONSTANT_Class")
        name = cp.resolve_utf8(this_class_entry.name_index)

        # Resolve super_class → class name string or None.
        super_name = None
        if cf.super_class != 0:
            super_entry = cp.peek(cf.super_class)
            if type(super_entry) is not ClassInfo:
                raise ValueError(f"super_class CP index {cf.super_class} is not a CONSTANT_Class")
            super_name = cp.resolve_utf8(super_entry.name_index)

        # Resolve interfaces.
        interfaces = []
        for iface_index in cf.interfaces:
            iface_entry = cp.peek(iface_index)
            if type(iface_entry) is not ClassInfo:
                raise ValueError(f"interface CP index {iface_index} is not a CONSTANT_Class")
            interfaces.append(cp.resolve_utf8(iface_entry.name_index))

        # Convert fields.
        fields = [_field_from_info(fi, cp) for fi in cf.fields]

        # Reuse lifted CP-backed wrappers across methods in the same class.
        lifted_cp_item_cache = {}

        # Convert methods.
        methods = [
            _method_from_info(
                mi,
                cp,
                skip_debug=skip_debug,
                const_pool_item_cache=lifted_cp_item_cache,
            )
            for mi in cf.methods
        ]
        class_attributes = clone_attributes(cf.attributes)
        if skip_debug:
            class_attributes = strip_class_debug_attributes(class_attributes)

        return cls(
            version=(cf.major_version, cf.minor_version),
            access_flags=cf.access_flags,
            name=name,
            super_name=super_name,
            interfaces=interfaces,
            fields=fields,
            methods=methods,
            attributes=class_attributes,
            constant_pool=cp,
        )

    @classmethod
    def from_bytes(cls, object data, *, bint skip_debug=False):
        """Parse raw class-file bytes and build a ``ClassModel``.

        Args:
            data: Raw ``.class`` file content.
            skip_debug: If true, strip debug metadata during lifting.

        Returns:
            A fully resolved ``ClassModel``.
        """
        reader = ClassReader(data)
        return cls.from_classfile(reader.class_info, skip_debug=skip_debug)

    # ------------------------------------------------------------------
    # Lowering
    # ------------------------------------------------------------------

    def to_classfile(
        self,
        *,
        bint recompute_frames=False,
        object resolver=None,
        object debug_info=DebugInfoPolicy.PRESERVE,
    ):
        """Lower this model back to a spec-faithful ``ClassFile``.

        Uses the ``ConstantPoolBuilder`` to allocate (or find) constant-pool
        entries for every symbolic reference and reassembles the raw
        ``FieldInfo``/``MethodInfo``/``CodeAttr`` structures.

        Args:
            recompute_frames: When true, recompute ``max_stack``,
                ``max_locals``, and ``StackMapTable`` for every method
                that has code.
            resolver: Class hierarchy resolver required when
                *recompute_frames* is true.
            debug_info: Controls whether lifted debug tables and
                class-level source-debug attributes are preserved or
                stripped during lowering.

        Returns:
            A fully assembled ``ClassFile`` ready for serialization.
        """
        cp = self.constant_pool
        debug_policy = normalize_debug_info_policy(debug_info)

        # Allocate class-level CP entries.
        this_class_index = cp.add_class(self.name)
        super_class_index = cp.add_class(self.super_name) if self.super_name else 0
        interface_indexes = [cp.add_class(iface) for iface in self.interfaces]

        # Lower fields.
        raw_fields = [_field_to_info(fm, cp) for fm in self.fields]

        # Lower methods.
        raw_methods = [
            _method_to_info(
                mm,
                cp,
                class_name=self.name,
                recompute_frames=recompute_frames,
                resolver=resolver,
                debug_info=debug_policy,
            )
            for mm in self.methods
        ]

        pool = cp.build()
        class_attributes = self.attributes
        if debug_policy is DebugInfoPolicy.STRIP or is_class_debug_info_stale(self):
            class_attributes = strip_class_debug_attributes(class_attributes)

        return ClassFile(
            magic=MAGIC,
            minor_version=self.version[1],
            major_version=self.version[0],
            constant_pool_count=cp.count,
            constant_pool=pool,
            access_flags=self.access_flags,
            this_class=this_class_index,
            super_class=super_class_index,
            interfaces_count=len(interface_indexes),
            interfaces=interface_indexes,
            fields_count=len(raw_fields),
            fields=raw_fields,
            methods_count=len(raw_methods),
            methods=raw_methods,
            attributes_count=len(class_attributes),
            attributes=clone_attributes(class_attributes),
        )

    def to_bytes(
        self,
        *,
        bint recompute_frames=False,
        object resolver=None,
        object debug_info=DebugInfoPolicy.PRESERVE,
    ):
        """Lower this model and serialize the resulting ``ClassFile`` to bytes.

        Args:
            recompute_frames: When true, recompute stack-map frames
                for every method that has code.
            resolver: Class hierarchy resolver required when
                *recompute_frames* is true.
            debug_info: Controls whether debug metadata is preserved
                or stripped during lowering.

        Returns:
            Raw ``.class`` file content.
        """
        return ClassWriter.write(
            self.to_classfile(
                recompute_frames=recompute_frames,
                resolver=resolver,
                debug_info=debug_info,
            )
        )


# ------------------------------------------------------------------
# Internal helpers
# ------------------------------------------------------------------


def _field_from_info(object fi, object cp):
    return FieldModel(
        access_flags=fi.access_flags,
        name=cp.resolve_utf8(fi.name_index),
        descriptor=cp.resolve_utf8(fi.descriptor_index),
        attributes=clone_attributes(fi.attributes),
    )


def _method_from_info(
    object mi,
    object cp,
    *,
    bint skip_debug=False,
    dict const_pool_item_cache=None,
):
    code = None
    cdef list non_code_attrs = []

    for attr in mi.attributes:
        if type(attr) is CodeAttr:
            code = _code_from_attr(
                attr,
                cp,
                skip_debug=skip_debug,
                const_pool_item_cache=const_pool_item_cache,
            )
        else:
            non_code_attrs.append(clone_attribute(attr))
    if skip_debug:
        non_code_attrs = skip_debug_method_attributes(non_code_attrs)

    return MethodModel(
        access_flags=mi.access_flags,
        name=cp.resolve_utf8(mi.name_index),
        descriptor=cp.resolve_utf8(mi.descriptor_index),
        code=code,
        attributes=non_code_attrs,
    )


def _field_to_info(object fm, object cp):
    return FieldInfo(
        access_flags=fm.access_flags,
        name_index=cp.add_utf8(fm.name),
        descriptor_index=cp.add_utf8(fm.descriptor),
        attributes_count=len(fm.attributes),
        attributes=clone_attributes(fm.attributes),
    )


def _method_to_info(
    object mm,
    object cp,
    *,
    object class_name=None,
    bint recompute_frames=False,
    object resolver=None,
    object debug_info=DebugInfoPolicy.PRESERVE,
):
    cdef list attrs = clone_attributes(mm.attributes)

    if mm.code is not None:
        code_attr = lower_code(
            mm.code,
            cp,
            method=mm if recompute_frames else None,
            class_name=class_name if recompute_frames else None,
            resolver=resolver,
            recompute_frames=recompute_frames,
            debug_info=debug_info,
        )
        attrs.insert(0, code_attr)

    return MethodInfo(
        access_flags=mm.access_flags,
        name_index=cp.add_utf8(mm.name),
        descriptor_index=cp.add_utf8(mm.descriptor),
        attributes_count=len(attrs),
        attributes=attrs,
    )


cdef inline int _validate_code_offset(int offset, int code_length, str context):
    if not 0 <= offset <= code_length:
        raise ValueError(f"{context} offset {offset} is outside code range [0, {code_length}]")
    return offset


cdef inline CLabel _label_for_offset(dict labels_by_offset, int offset):
    return labels_by_offset[offset]


def _collect_labels(object code_attr, *, bint skip_debug=False):
    cdef int code_length = code_attr.code_length
    cdef set label_offsets = set()
    add_label_offset = label_offsets.add

    for insn in code_attr.code:
        insn_type = type(insn)
        if insn_type is Branch or insn_type is BranchW:
            add_label_offset(
                _validate_code_offset(
                    insn.bytecode_offset + insn.offset,
                    code_length,
                    _BRANCH_TARGET_CONTEXTS[insn.type],
                )
            )
        elif insn_type is LookupSwitch:
            add_label_offset(
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_length,
                    "lookupswitch default target",
                )
            )
            for pair in insn.pairs:
                add_label_offset(
                    _validate_code_offset(
                        insn.bytecode_offset + pair.offset,
                        code_length,
                        "lookupswitch case target",
                    )
                )
        elif insn_type is TableSwitch:
            add_label_offset(
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_length,
                    "tableswitch default target",
                )
            )
            for relative in insn.offsets:
                add_label_offset(
                    _validate_code_offset(
                        insn.bytecode_offset + relative,
                        code_length,
                        "tableswitch case target",
                    )
                )

    for exception in code_attr.exception_table:
        add_label_offset(_validate_code_offset(exception.start_pc, code_length, "exception handler start"))
        add_label_offset(_validate_code_offset(exception.end_pc, code_length, "exception handler end"))
        add_label_offset(_validate_code_offset(exception.handler_pc, code_length, "exception handler target"))

    if skip_debug:
        labels_by_offset = {}
        for offset in label_offsets:
            labels_by_offset[offset] = Label(f"L{offset}")
        return labels_by_offset

    for attribute in code_attr.attributes:
        attr_type = type(attribute)
        if attr_type is LineNumberTableAttr:
            for entry in attribute.line_number_table:
                add_label_offset(_validate_code_offset(entry.start_pc, code_length, "line number entry"))
        elif attr_type is LocalVariableTableAttr:
            for entry in attribute.local_variable_table:
                add_label_offset(_validate_code_offset(entry.start_pc, code_length, "local variable start"))
                add_label_offset(
                    _validate_code_offset(entry.start_pc + entry.length, code_length, "local variable end")
                )
        elif attr_type is LocalVariableTypeTableAttr:
            for entry in attribute.local_variable_type_table:
                add_label_offset(
                    _validate_code_offset(entry.start_pc, code_length, "local variable type start")
                )
                add_label_offset(
                    _validate_code_offset(
                        entry.start_pc + entry.length,
                        code_length,
                        "local variable type end",
                    )
                )

    labels_by_offset = {}
    for offset in label_offsets:
        labels_by_offset[offset] = Label(f"L{offset}")
    return labels_by_offset


cdef object _lift_instruction(
    object insn,
    dict labels_by_offset,
    object cp,
):
    cdef CBranch branch_insn
    cdef CBranchW branch_w_insn
    cdef CLookupSwitch lookup_switch
    cdef CTableSwitch table_switch
    cdef CIInc iinc_insn
    cdef CIIncW iinc_w_insn
    cdef CInvokeDynamic invoke_dynamic
    cdef CInvokeInterface invoke_interface
    cdef CMultiANewArray multi_anew_array
    cdef CConstPoolIndex const_pool_insn
    cdef CLocalIndexW local_index_w
    cdef CLocalIndex local_index
    cdef CMatchOffsetPair pair
    cdef Py_ssize_t relative
    insn_type = type(insn)
    if insn_type is Branch:
        branch_insn = insn
        return _trusted_branch_insn(
            branch_insn.type,
            _label_for_offset(labels_by_offset, branch_insn.bytecode_offset + branch_insn.offset),
        )
    if insn_type is BranchW:
        branch_w_insn = insn
        return _trusted_branch_insn(
            branch_w_insn.type,
            _label_for_offset(labels_by_offset, branch_w_insn.bytecode_offset + branch_w_insn.offset),
        )
    if insn_type is LookupSwitch:
        lookup_switch = insn
        return _trusted_lookup_switch_insn(
            _label_for_offset(
                labels_by_offset,
                lookup_switch.bytecode_offset + lookup_switch.default,
            ),
            [
                (
                    pair.match,
                    _label_for_offset(
                        labels_by_offset,
                        lookup_switch.bytecode_offset + pair.offset,
                    ),
                )
                for pair in lookup_switch.pairs
            ],
        )
    if insn_type is TableSwitch:
        table_switch = insn
        return _trusted_table_switch_insn(
            _label_for_offset(
                labels_by_offset,
                table_switch.bytecode_offset + table_switch.default,
            ),
            table_switch.low,
            table_switch.high,
            [
                _label_for_offset(
                    labels_by_offset,
                    table_switch.bytecode_offset + relative,
                )
                for relative in table_switch.offsets
            ],
        )
    if insn_type is IInc:
        iinc_insn = insn
        return _trusted_iinc_insn(iinc_insn.index, iinc_insn.value)
    if insn_type is IIncW:
        iinc_w_insn = insn
        return _trusted_iinc_insn(iinc_w_insn.index, iinc_w_insn.value)
    if insn_type is InvokeDynamic:
        invoke_dynamic = insn
        return _lift_invoke_dynamic(invoke_dynamic, cp)
    if insn_type is InvokeInterface:
        invoke_interface = insn
        return _lift_invoke_interface(invoke_interface, cp)
    if insn_type is MultiANewArray:
        multi_anew_array = insn
        return _trusted_multi_anew_array_insn(
            _resolve_class_name(cp, multi_anew_array.index),
            multi_anew_array.dimensions,
        )
    if insn_type is ConstPoolIndex:
        const_pool_insn = insn
        return _lift_const_pool_index(const_pool_insn, cp)
    if insn_type is LocalIndexW:
        local_index_w = insn
        # WIDE var opcodes: normalize to VarInsn with canonical base opcode.
        base = _WIDE_TO_BASE[local_index_w.type]
        return _trusted_var_insn(base, local_index_w.index)
    if insn_type is LocalIndex:
        local_index = insn
        # LDC (0x12) uses a u1 CP index stored in LocalIndex.index.
        if local_index.type == InsnInfoType.LDC:
            return _trusted_ldc_insn(_resolve_ldc_value(cp, local_index.index))
        return _trusted_var_insn(local_index.type, local_index.index)
    # Implicit slot variants: ILOAD_0 through ASTORE_3.
    implicit = _IMPLICIT_VAR_SLOTS.get(insn.type)
    if implicit is not None:
        base_opcode, slot = implicit
        return _trusted_var_insn(base_opcode, slot)
    return clone_raw_instruction(insn)


def _lift_instructions(
    object code_attr,
    dict labels_by_offset,
    object cp,
    dict const_pool_item_cache=None,
):
    cdef list instructions = []
    cdef set inserted_offsets = set()
    if const_pool_item_cache is None:
        const_pool_item_cache = {}
    append = instructions.append
    inserted_add = inserted_offsets.add
    labels_get = labels_by_offset.get

    for insn in code_attr.code:
        label = labels_get(insn.bytecode_offset)
        if label is not None and insn.bytecode_offset not in inserted_offsets:
            append(label)
            inserted_add(insn.bytecode_offset)

        if type(insn) is ConstPoolIndex:
            const_pool_insn = insn
            cache_key = (const_pool_insn.type, const_pool_insn.index)
            cached = const_pool_item_cache.get(cache_key)
            if cached is None:
                cached = _lift_const_pool_index(const_pool_insn, cp)
                const_pool_item_cache[cache_key] = cached
            append(_clone_lifted_code_item(cached))
            continue

        append(_lift_instruction(insn, labels_by_offset, cp))

    cdef int code_length = code_attr.code_length
    end_label = labels_get(code_length)
    if end_label is not None and code_length not in inserted_offsets:
        append(end_label)
        inserted_add(code_length)

    missing_offsets = sorted(set(labels_by_offset) - inserted_offsets)
    if missing_offsets:
        raise ValueError(
            "labels refer to offsets that are not instruction boundaries: "
            + ", ".join(str(offset) for offset in missing_offsets)
        )

    return instructions


cdef object _clone_lifted_code_item(object item):
    item_type = type(item)
    if item_type is FieldInsn:
        field_item = item
        return _trusted_field_insn(field_item.type, field_item.owner, field_item.name, field_item.descriptor)
    if item_type is MethodInsn:
        method_item = item
        return _trusted_method_insn(
            method_item.type,
            method_item.owner,
            method_item.name,
            method_item.descriptor,
            method_item.is_interface,
        )
    if item_type is TypeInsn:
        type_item = item
        return _trusted_type_insn(type_item.type, type_item.class_name)
    if item_type is LdcInsn:
        ldc_item = item
        return _trusted_ldc_insn(ldc_item.value)
    return clone_raw_instruction(insn=item)


def _lift_exception_handlers(
    object code_attr,
    dict labels_by_offset,
    object cp,
):
    return [
        ExceptionHandler(
            start=_label_for_offset(labels_by_offset, exception.start_pc),
            end=_label_for_offset(labels_by_offset, exception.end_pc),
            handler=_label_for_offset(labels_by_offset, exception.handler_pc),
            catch_type=resolve_catch_type(cp, exception.catch_type),
        )
        for exception in code_attr.exception_table
    ]


def _lift_nested_code_attributes(
    object code_attr,
    dict labels_by_offset,
    object cp,
    *,
    bint skip_debug=False,
):
    cdef list line_numbers = []
    cdef list local_variables = []
    cdef list local_variable_types = []
    cdef list attributes = []
    cdef list layout = []

    for attribute in code_attr.attributes:
        attr_type = type(attribute)
        if attr_type is LineNumberTableAttr:
            layout.append("line_numbers")
            if skip_debug:
                continue
            line_numbers.extend(
                LineNumberEntry(
                    label=_label_for_offset(labels_by_offset, entry.start_pc),
                    line_number=entry.line_number,
                )
                for entry in attribute.line_number_table
            )
        elif attr_type is LocalVariableTableAttr:
            layout.append("local_variables")
            if skip_debug:
                continue
            local_variables.extend(
                LocalVariableEntry(
                    start=_label_for_offset(labels_by_offset, entry.start_pc),
                    end=_label_for_offset(labels_by_offset, entry.start_pc + entry.length),
                    name=cp.resolve_utf8(entry.name_index),
                    descriptor=cp.resolve_utf8(entry.descriptor_index),
                    slot=entry.index,
                )
                for entry in attribute.local_variable_table
            )
        elif attr_type is LocalVariableTypeTableAttr:
            layout.append("local_variable_types")
            if skip_debug:
                continue
            local_variable_types.extend(
                LocalVariableTypeEntry(
                    start=_label_for_offset(labels_by_offset, entry.start_pc),
                    end=_label_for_offset(labels_by_offset, entry.start_pc + entry.length),
                    name=cp.resolve_utf8(entry.name_index),
                    signature=cp.resolve_utf8(entry.signature_index),
                    slot=entry.index,
                )
                for entry in attribute.local_variable_type_table
            )
        else:
            layout.append("other")
            attributes.append(clone_attribute(attribute))

    return line_numbers, local_variables, local_variable_types, attributes, tuple(layout)


def _code_from_attr(
    object attr,
    object cp,
    *,
    bint skip_debug=False,
    dict const_pool_item_cache=None,
):
    labels_by_offset = _collect_labels(attr, skip_debug=skip_debug)
    line_numbers, local_variables, local_variable_types, attributes, layout = _lift_nested_code_attributes(
        attr,
        labels_by_offset,
        cp,
        skip_debug=skip_debug,
    )
    return CodeModel(
        max_stack=attr.max_stacks,
        max_locals=attr.max_locals,
        instructions=_lift_instructions(
            attr,
            labels_by_offset,
            cp,
            const_pool_item_cache,
        ),
        exception_handlers=_lift_exception_handlers(attr, labels_by_offset, cp),
        line_numbers=line_numbers,
        local_variables=local_variables,
        local_variable_types=local_variable_types,
        attributes=attributes,
        _nested_attribute_layout=layout,
    )


# ---------------------------------------------------------------------------
# CP resolution helpers (lifting direction)
# ---------------------------------------------------------------------------


cdef object _resolve_class_name(object cp, int index):
    """Resolve a CONSTANT_Class CP index to its internal name string."""
    entry = cp.peek(index)
    if type(entry) is not ClassInfo:
        raise ValueError(f"CP index {index} is not a CONSTANT_Class: {type(entry).__name__}")
    return cp.resolve_utf8(entry.name_index)


cdef _resolve_member_ref(
    object cp,
    int index,
):
    """Resolve a Fieldref/Methodref/InterfaceMethodref to (owner, name, descriptor, is_interface)."""
    entry = cp.peek(index)
    cdef type t = type(entry)
    if t is not FieldrefInfo and t is not MethodrefInfo and t is not InterfaceMethodrefInfo:
        raise ValueError(f"CP index {index} is not a member ref entry: {type(entry).__name__}")
    cdef bint is_interface = t is InterfaceMethodrefInfo
    owner = _resolve_class_name(cp, entry.class_index)
    nat = cp.peek(entry.name_and_type_index)
    if type(nat) is not NameAndTypeInfo:
        raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
    name = cp.resolve_utf8(nat.name_index)
    descriptor = cp.resolve_utf8(nat.descriptor_index)
    return owner, name, descriptor, is_interface


cdef object _resolve_ldc_value(
    object cp,
    int index,
):
    """Resolve an LDC/LDC_W/LDC2_W CP index to a typed ``LdcValue``."""
    entry = cp.peek(index)
    cdef type t = type(entry)
    if t is IntegerInfo:
        value = LdcInt(entry.value_bytes)
    elif t is FloatInfo:
        value = LdcFloat(entry.value_bytes)
    elif t is LongInfo:
        unsigned = (entry.high_bytes << 32) | (entry.low_bytes & 0xFFFFFFFF)
        value = unsigned - (1 << 64) if unsigned >= (1 << 63) else unsigned
        value = LdcLong(value)
    elif t is DoubleInfo:
        value = LdcDouble(entry.high_bytes, entry.low_bytes)
    elif t is StringInfo:
        value = LdcString(cp.resolve_utf8(entry.string_index))
    elif t is ClassInfo:
        value = LdcClass(cp.resolve_utf8(entry.name_index))
    elif t is MethodTypeInfo:
        value = LdcMethodType(cp.resolve_utf8(entry.descriptor_index))
    elif t is MethodHandleInfo:
        value = _resolve_ldc_method_handle(cp, entry)
    elif t is DynamicInfo:
        nat = cp.peek(entry.name_and_type_index)
        if type(nat) is not NameAndTypeInfo:
            raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
        value = LdcDynamic(
            entry.bootstrap_method_attr_index,
            cp.resolve_utf8(nat.name_index),
            cp.resolve_utf8(nat.descriptor_index),
        )
    else:
        raise ValueError(f"CP index {index} has unsupported type for LDC: {type(entry).__name__}")
    return value


cdef object _resolve_ldc_method_handle(
    object cp,
    object entry,
):
    """Resolve a CONSTANT_MethodHandle entry to an ``LdcMethodHandle``."""
    ref_entry = cp.peek(entry.reference_index)
    cdef type t = type(ref_entry)
    if t is not FieldrefInfo and t is not MethodrefInfo and t is not InterfaceMethodrefInfo:
        raise ValueError(
            f"MethodHandle reference index {entry.reference_index} has unexpected type: {type(ref_entry).__name__}"
        )
    cdef bint is_interface = t is InterfaceMethodrefInfo
    owner = _resolve_class_name(cp, ref_entry.class_index)
    nat = cp.peek(ref_entry.name_and_type_index)
    if type(nat) is not NameAndTypeInfo:
        raise ValueError(f"CP index {ref_entry.name_and_type_index} is not a CONSTANT_NameAndType")
    name = cp.resolve_utf8(nat.name_index)
    descriptor = cp.resolve_utf8(nat.descriptor_index)
    return LdcMethodHandle(entry.reference_kind, owner, name, descriptor, is_interface)


cdef object _lift_const_pool_index(
    object insn,
    object cp,
):
    """Lift a raw ``ConstPoolIndex`` instruction to a symbolic wrapper."""
    opcode = insn.type
    if opcode in (
        InsnInfoType.GETFIELD,
        InsnInfoType.PUTFIELD,
        InsnInfoType.GETSTATIC,
        InsnInfoType.PUTSTATIC,
    ):
        owner, name, descriptor, _ = _resolve_member_ref(cp, insn.index)
        return _trusted_field_insn(opcode, owner, name, descriptor)
    if opcode in (
        InsnInfoType.INVOKEVIRTUAL,
        InsnInfoType.INVOKESPECIAL,
        InsnInfoType.INVOKESTATIC,
    ):
        owner, name, descriptor, is_interface = _resolve_member_ref(cp, insn.index)
        return _trusted_method_insn(opcode, owner, name, descriptor, is_interface)
    if opcode in (
        InsnInfoType.NEW,
        InsnInfoType.CHECKCAST,
        InsnInfoType.INSTANCEOF,
        InsnInfoType.ANEWARRAY,
    ):
        return _trusted_type_insn(opcode, _resolve_class_name(cp, insn.index))
    if opcode in (InsnInfoType.LDC_W, InsnInfoType.LDC2_W):
        return _trusted_ldc_insn(_resolve_ldc_value(cp, insn.index))
    # Unknown ConstPoolIndex opcode: pass through unchanged (e.g. future opcodes).
    return clone_raw_instruction(insn)


cdef object _lift_invoke_dynamic(object insn, object cp):
    """Lift a raw ``InvokeDynamic`` instruction to an ``InvokeDynamicInsn``."""
    entry = cp.peek(insn.index)
    if type(entry) is not InvokeDynamicInfo:
        raise ValueError(f"CP index {insn.index} is not a CONSTANT_InvokeDynamic: {type(entry).__name__}")
    nat = cp.peek(entry.name_and_type_index)
    if type(nat) is not NameAndTypeInfo:
        raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
    return _trusted_invoke_dynamic_insn(
        entry.bootstrap_method_attr_index,
        cp.resolve_utf8(nat.name_index),
        cp.resolve_utf8(nat.descriptor_index),
    )


cdef object _lift_invoke_interface(
    object insn,
    object cp,
):
    """Lift a raw ``InvokeInterface`` instruction to an ``InterfaceMethodInsn``."""
    owner, name, descriptor, _ = _resolve_member_ref(cp, insn.index)
    return _trusted_interface_method_insn(owner, name, descriptor)
