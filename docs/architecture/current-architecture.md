# Current architecture

The codebase is currently organized as a parser pipeline with a strongly typed read model.

## Runtime and packaging

- Requires Python 3.14+ (`pyproject.toml`)
- Ships a `py.typed` marker so downstream type checkers can consume package types
- Keeps the core library runtime dependency-free
- Uses Ruff, basedpyright, and `pytest`-based validation in development (see `README.md`)

## Public entry points

- `pytecode.ClassReader`
  - Constructed directly from classfile bytes, or via `ClassReader.from_bytes()` / `ClassReader.from_file()`
  - Parses eagerly during initialization
  - Produces `class_info`, an `info.ClassFile` dataclass tree
- `pytecode.JarFile`
  - Reads the contents of a JAR
  - Separates `.class` entries from non-class resources
  - Parses classes via `ClassReader`

- `pytecode.ClassModel`
  - Mutable editing model for JVM class files
  - Uses symbolic (resolved) references instead of raw constant-pool indexes
  - Constructed from a parsed `ClassFile` via `ClassModel.from_classfile()` or from raw bytes via `ClassModel.from_bytes()`
  - Produces a spec-faithful `ClassFile` via `to_classfile()` for lowering and future emission

At the moment, these are the only public exports in `pytecode.__init__`.

## Module responsibilities

### `pytecode\bytes_utils.py`

Low-level big-endian binary I/O primitives for both reading and writing. This is the I/O foundation for class parsing and future classfile emission.

**Read side**: Standalone `_read_u1/i1/u2/i2/u4/i4/_read_bytes` helper functions and a stateful `BytesReader` that tracks a cursor offset.

**Write side**: Standalone `_write_u1/i1/u2/i2/u4/i4/_write_bytes` helper functions and a stateful `BytesWriter` that appends to an internal buffer. `BytesWriter` provides `write_u1/i1/u2/i2/u4/i4/bytes` methods, `align(n)` for opcode-alignment padding, and a full set of `reserve_u1/i1/u2/i2/u4/i4` and `patch_u1/i1/u2/i2/u4/i4` methods for deferred length-prefixed structures.

### `pytecode\class_reader.py`

The central parser. `ClassReader` walks the classfile format in spec order:

1. header and version
2. constant pool
3. class metadata
4. interfaces
5. fields
6. methods
7. attributes

It also contains specialized parsing routines for:

- instructions
- `Code` attributes
- stack map frames
- annotations and type annotations
- module-related attributes

This module is the current orchestration layer for nearly all parsing logic.

When constant-pool-backed names are interpreted (for example, attribute names),
they are decoded using the shared JVM Modified UTF-8 helpers rather than plain
UTF-8.

### `pytecode\constant_pool.py`

Typed dataclasses for all 17 constant-pool entry types plus the `ConstantPoolInfoType` enum mapping tags to dataclasses. The enum embeds both the numeric tag and the corresponding dataclass, so new constant types only require adding an enum member.

### `pytecode\modified_utf8.py`

Shared JVM Modified UTF-8 codec helpers for `CONSTANT_Utf8` values. This module
centralizes spec-correct encoding and decoding of:

- embedded NUL (`U+0000`) using the two-byte modified form
- supplementary characters via UTF-16 surrogate pairs
- malformed byte-sequence rejection (for example, illegal four-byte UTF-8 forms)

It is used by `ConstantPoolBuilder`, `ClassReader`, and test helpers so
constant-pool string handling stays consistent across parsing, editing, and
fixtures.

### `pytecode\attributes.py`

Over 80 typed dataclasses for classfile attributes and nested structures (verification types, stack map frames, annotations, type annotations, module info, record components, etc.). It also defines `AttributeInfoType`, which maps attribute names to concrete dataclasses via an enum with a `_missing_` fallback. Unknown attribute names are routed to `UnimplementedAttr`, allowing parse-time preservation of vendor or future attributes.

### `pytecode\instructions.py`

Typed dataclasses for decoded bytecode instructions and operand shapes (12 operand-specific subclasses covering local indexes, constant pool indexes, branches, switches, etc.), plus the `InsnInfoType` opcode enum (205 standard opcodes plus WIDE variants) that maps byte values to instruction record types, and an `ArrayType` enum for `newarray`.

### `pytecode\info.py`

Top-level dataclasses representing the parsed classfile structure:

- `ClassFile`
- `FieldInfo`
- `MethodInfo`

These dataclasses hold references to attribute and constant-pool structures defined elsewhere.

### `pytecode\constants.py`

Enums and flags representing JVM constants, access flags, verification types, and target-type metadata. (The former `FieldType` enum was removed here and superseded by `BaseType` in `descriptors.py`.)

### `pytecode\descriptors.py`

Descriptor and generic signature utilities. Provides:

- **Data model**: `BaseType` enum (8 JVM primitives), `VoidType`, `ObjectType`, `ArrayType`, `MethodDescriptor` frozen dataclasses, and a full generic signature type hierarchy (`ClassTypeSignature`, `TypeVariable`, `ArrayTypeSignature`, `TypeArgument`, `TypeParameter`, `ClassSignature`, `MethodSignature`)
- **Parsing**: `parse_field_descriptor()`, `parse_method_descriptor()`, `parse_class_signature()`, `parse_method_signature()`, `parse_field_signature()` — all recursive-descent, raising `ValueError` with position context on malformed input, including malformed internal names, empty path segments, and empty inner-class suffixes
- **Construction**: `to_descriptor()` — converts structured types back to JVM descriptor strings (round-trip)
- **Slot helpers**: `slot_size()` and `parameter_slot_count()` — category-aware (long/double occupy 2 slots)
- **Validation**: `is_valid_field_descriptor()` and `is_valid_method_descriptor()` with spec-aware internal-name checks

All types are imported directly from `pytecode.descriptors`.

### `pytecode\constant_pool_builder.py`

Constant-pool management utilities for building and editing JVM constant pools. Provides `ConstantPoolBuilder`, a mutable accumulator with:

- **Deduplication on insertion** — identical entries return the existing index rather than growing the pool; both structural (raw-index-based via `add_entry`) and semantic (value-based via convenience methods) deduplication are supported
- **High-level convenience methods** — `add_utf8`, `add_class`, `add_methodref`, `add_fieldref`, etc. automatically create and deduplicate all prerequisite entries (e.g. `add_class("Foo")` creates the `CONSTANT_Utf8` name entry first), and `add_utf8()` uses JVM Modified UTF-8 with the JVM `u2` byte-length limit
- **Symbol-table lookups** — `find_utf8`, `find_class`, `find_name_and_type`, and `resolve_utf8` enable index-free lookups by value using spec-correct Modified UTF-8
- **Double-slot handling** — Long and Double entries automatically occupy two consecutive slots, consistent with the JVM spec
- **Spec hardening** — `add_method_handle()` validates reference kind, target entry type, and special-method rules; direct/imported `Utf8Info` entries are validated for length and Modified UTF-8 correctness
- **Deterministic ordering** — entries are assigned indexes in insertion order; deduplication never reorders existing entries
- **Import from parsed pools** — `ConstantPoolBuilder.from_pool(list)` seeds the builder from an existing `ClassFile.constant_pool`, preserving all original indexes so existing CP references remain valid; it now also validates index-0 placeholder rules, Long/Double gap slots, Utf8 payload consistency, and MethodHandle constraints before accepting the imported pool
- **Export to spec format** — `build()` returns a defensive-copy `list[ConstantPoolInfo | None]` identical in structure to `ClassFile.constant_pool`, and `get()` also returns defensive copies so caller mutation cannot corrupt builder state
- **Pool size guard** — raises `ValueError` if an allocation would exceed the JVM's u2 maximum (65 534 single-slot or 65 533 double-slot)

### `pytecode\model.py`

Mutable editing model for safe classfile manipulation. This module provides the higher-level object model described in issue [#6](https://github.com/smithtrenton/pytecode/issues/6), implementing Design A (Mutable Dataclasses). Four core types form the user-facing editing layer, with label-specific helpers delegated to `pytecode\labels.py`:

- **`ClassModel`** — top-level mutable representation of a class file. Fields use symbolic (resolved) references: `name: str`, `super_name: str | None`, `interfaces: list[str]`, along with `access_flags`, `version: tuple[int, int]`, lists of `FieldModel` and `MethodModel`, class-level attributes, and a `ConstantPoolBuilder`. Provides `from_classfile()` and `from_bytes()` factory methods for construction, and `to_classfile()` for lowering back to a spec-faithful `ClassFile`.
- **`MethodModel`** — mutable representation of a method with resolved `name: str` and `descriptor: str`, `access_flags`, an optional `CodeModel` (`None` for abstract/native methods), and non-Code attributes. The raw `Code` attribute is lifted out of the attribute list into the dedicated `code` field.
- **`FieldModel`** — mutable representation of a field with resolved `name: str` and `descriptor: str`, `access_flags`, and attributes.
- **`CodeModel`** — wraps a mixed instruction stream (`InsnInfo` plus `Label` pseudo-instructions), symbolic exception handlers, lifted line/local-variable debug tables, `max_stack`, `max_locals`, and residual nested Code attributes. During `from_classfile()`, all supported instruction families are lifted to symbolic wrappers: branch/switch instructions become `BranchInsn`/`LookupSwitchInsn`/`TableSwitchInsn`; field/method/type/LDC/invoke-dynamic/multianewarray constant-pool instructions become their corresponding operand wrappers from `pytecode.operands`; local-variable slot instructions (including all implicit `ILOAD_0`–`ASTORE_3` variants and WIDE forms) become `VarInsn`. All symbolic wrappers lower back to spec-shaped raw instructions during `to_classfile()`.

The model carries a `ConstantPoolBuilder` seeded from the original constant pool so that raw attributes and any still-raw instruction operands remain valid through editing. Symbolic references are resolved during `from_classfile()` and re-allocated during `to_classfile()`. Both conversion directions use deep copies for all mutable raw structures they retain (attribute lists, instruction records, and constant-pool-backed payloads) so the `ClassModel` owns its data independently from the source `ClassFile` — consistent with the defensive-copy convention already used by `ConstantPoolBuilder`.

For the design rationale behind this editing model, see [editing model design rationale](../design/editing-model.md).

### `pytecode\hierarchy.py`

Hierarchy-resolution helpers introduced for issue [#8](https://github.com/smithtrenton/pytecode/issues/8). This module provides:

- **Resolved snapshots** — `ResolvedClass` and `ResolvedMethod` frozen dataclasses for hierarchy-relevant class metadata, plus `InheritedMethod` for reporting matching inherited declarations
- **Pluggable interface** — `ClassResolver`, a minimal protocol that resolves an internal class name to a `ResolvedClass | None`
- **Built-in implementation** — `MappingClassResolver` for in-memory hierarchy graphs, with `from_classfiles()` and `from_models()` convenience constructors
- **Query helpers** — `iter_superclasses()`, `iter_supertypes()`, `is_subtype()`, `common_superclass()`, and `find_overridden_methods()`
- **Explicit failure modes** — `UnresolvedClassError` for missing classes and `HierarchyCycleError` for malformed ancestry loops

To keep ordinary project graphs ergonomic, the module treats `java/lang/Object` as an implicit root if a resolver does not provide it explicitly; otherwise missing hierarchy data is surfaced as an error rather than guessed.

### `pytecode\operands.py`

Symbolic editing-model wrappers for non-control-flow instructions, introduced for issue [#16](https://github.com/smithtrenton/pytecode/issues/16). All wrappers inherit from `InsnInfo` so that the existing `type CodeItem = InsnInfo | Label` alias requires no changes.

**Instruction wrappers:**

- **`FieldInsn`** — `GETFIELD`, `PUTFIELD`, `GETSTATIC`, `PUTSTATIC`; fields `owner: str`, `name: str`, `descriptor: str`
- **`MethodInsn`** — `INVOKEVIRTUAL`, `INVOKESPECIAL`, `INVOKESTATIC`; fields `owner`, `name`, `descriptor`, `is_interface: bool` (needed for interface-targeted INVOKESTATIC/INVOKESPECIAL since Java 8+)
- **`InterfaceMethodInsn`** — `INVOKEINTERFACE`; fields `owner`, `name`, `descriptor`; `count` is auto-computed from the descriptor during lowering
- **`TypeInsn`** — `NEW`, `CHECKCAST`, `INSTANCEOF`, `ANEWARRAY`; field `class_name: str`
- **`VarInsn`** — all local-variable load/store/RET opcodes including implicit `ILOAD_0`–`ASTORE_3` (40 variants) and WIDE forms, normalized to a canonical `(base_opcode, slot)` pair; slots are validated to fit the JVM `u2` range, and lowering selects the optimal encoding (implicit → explicit → WIDE, including `RETW`)
- **`IIncInsn`** — `IINC` and `IINCW`, normalized to `(slot, increment)`; `slot` is validated to fit `u2`, `increment` is validated to fit `i2`, and lowering picks narrow or wide form based on operand range
- **`LdcInsn`** — `LDC`, `LDC_W`, `LDC2_W`; field `value: LdcValue`; lowering selects the minimal encoding: `LDC` (2 bytes) when the CP index fits in one byte (≤ 255), `LDC_W` (3 bytes) otherwise; double-category constants always use `LDC2_W` (3 bytes)
- **`InvokeDynamicInsn`** — `INVOKEDYNAMIC`; fields `bootstrap_method_attr_index: int`, `name: str`, `descriptor: str`; bootstrap indexes are validated to fit JVM `u2`
- **`MultiANewArrayInsn`** — `MULTIANEWARRAY`; fields `class_name: str`, `dimensions: int`; dimensions are validated to fit JVM `u1` range `1..255`

**LDC value type hierarchy** (`LdcValue` union):
`LdcInt`, `LdcFloat`, `LdcLong`, `LdcDouble`, `LdcString`, `LdcClass`, `LdcMethodType`, `LdcMethodHandle`, `LdcDynamic` — frozen dataclasses carrying the typed constant payload.

**Mapping tables:** `_IMPLICIT_VAR_SLOTS` / `_VAR_SHORTCUTS` (40 implicit-slot ↔ base-opcode/slot pairs), `_WIDE_TO_BASE` / `_BASE_TO_WIDE` (11 WIDE variant ↔ base opcode pairs).

### `pytecode\labels.py`

Label-based bytecode editing helpers and lowering utilities introduced for issue [#7](https://github.com/smithtrenton/pytecode/issues/7). This module owns the symbolic control-flow layer:

- **`Label`** — identity-based pseudo-instruction marker for bytecode positions
- **`BranchInsn` / `LookupSwitchInsn` / `TableSwitchInsn`** — editing-model control-flow instructions that target labels instead of raw offsets
- **`ExceptionHandler` / `LineNumberEntry` / `LocalVariableEntry` / `LocalVariableTypeEntry`** — lifted exception/debug metadata bound to labels rather than byte offsets
- **`resolve_labels()`** — computes byte offsets for labels and instructions in a mixed `InsnInfo | Label` stream; for single-slot `LdcInsn` values, exact sizing uses a provided `ConstantPoolBuilder` context without mutating the live pool
- **`lower_code()`** — lowers symbolic code back to a raw `CodeAttr`, recalculating offsets and switch padding, promoting `GOTO`/`JSR` to wide forms, inverting overflowing conditional branches, reconstructing lifted debug attributes, and lowering all operand wrappers from `pytecode.operands` to spec-shaped raw instructions with correct CP index allocation

For the broader design rationale, trade-offs, and future phases behind this editing model, see [editing model design rationale](../design/editing-model.md).

### `pytecode\jar.py`

JAR container support. This is currently a convenience layer around archive reading plus class parsing: it separates `.class` entries from non-class resources and parses each class via `ClassReader`. JAR rewrite support remains future work ([#15](https://github.com/smithtrenton/pytecode/issues/15)).

### `run.py`

A repository smoke-test script that parses a JAR and writes pretty-printed class output alongside copied resource files. This is primarily a manual validation utility for current parser behavior against the checked-in `225.jar` sample.

### `tools\parse_wiki_instructions.py`

A support tool used to generate or verify instruction enum data from a JVM instruction reference table. This reduces manual drift in opcode metadata.

## Current data flow

### Class parsing

1. Raw bytes are wrapped in `BytesReader`.
2. `ClassReader.read_class()` validates the header and version.
3. Constant-pool entries are decoded into typed objects.
4. Field, method, and class attributes are parsed recursively.
5. `Code` attributes are decoded into typed instruction objects.
6. The final parsed result is stored as an `info.ClassFile` object on `ClassReader.class_info`.

### JAR parsing

1. `JarFile` reads each ZIP entry into memory.
2. Entries ending in `.class` are parsed with `ClassReader`.
3. Non-class resources are preserved as raw bytes.
4. Callers receive `(JarInfo, ClassReader)` pairs for classes and `JarInfo` records for other files.

## Design characteristics

### Strengths

- Spec-shaped datamodels make the parser output explicit and inspectable
- The parser retains structural detail rather than flattening everything into dictionaries
- Unknown attributes can still be carried as bytes
- Full opcode coverage with no gaps in the standard instruction set
- JAR-level parsing already gives the project a practical integration point
- The codebase is dependency-light and easy to run locally
- Enum-driven dispatch in both `ConstantPoolInfoType` and `AttributeInfoType` makes adding new types mechanical

### Current constraints

- Parsing is eager and tightly coupled to `ClassReader`
- Most parser behavior lives in one large module (`class_reader.py`, ~653 lines), which will become harder to evolve once emission is added
- The editing model now uses labels for control flow, exception ranges, and debug scopes, and symbolic operand wrappers for all major non-control-flow instruction families; only raw pass-through instructions (`BIPUSH`, `SIPUSH`, `NEWARRAY`, and zero-operand `InsnInfo`) remain in their spec-shaped form
- There is no classfile emission layer yet — `ClassModel.to_classfile()` produces a spec-model `ClassFile` but binary serialization requires issue [#12](https://github.com/smithtrenton/pytecode/issues/12)

## Test coverage

The test suite provides both integration-level and unit-level coverage (831 tests total):

**Unit tests** ([#2](https://github.com/smithtrenton/pytecode/issues/2) — done):

- `test_attributes.py` — per-attribute-type parsing for all 30 standard attribute types, stack map frame variants, verification types, annotation element values, type annotations, and the `UnimplementedAttr` fallback.
- `test_instructions.py` — instruction decoding for all operand shapes including no-operand, local variable index, constant-pool index, bipush/sipush, branch16/branch32, iinc, invokedynamic, invokeinterface, newarray, multianewarray, lookupswitch, tableswitch, and wide variants.
- `test_constant_pool.py` — all 17 constant-pool entry types, Long/Double double-slot handling, Modified UTF-8 `CONSTANT_Utf8` cases, mixed pool parsing, and unknown tag errors.
- `test_class_reader.py` — classfile parsing including magic number validation, version field validation, constant-pool indexing, access flags, interfaces, fields, methods, Code attributes, and error paths for invalid/truncated classfiles.
- `test_bytes_utils.py` — all primitive byte readers and writers (`_read_*`/`_write_*`), `BytesReader` stateful cursor, rewind, and buffer overrun, `BytesWriter` sequential writes, alignment padding, reserve/patch, and round-trip read↔write.
- `test_jar.py` — JAR reading, class/non-class separation, path normalization, and compiled JAR class count.
- `test_descriptors.py` — all 8 base types, object and array types, method descriptors, slot counting (long/double = 2 slots), round-trip parse → construct → parse, malformed descriptor error handling, generic class/method/field signatures with type parameters, wildcards (`+`/`-`/`*`), inner classes, type variables, throws clauses, and invalid internal-name edge cases.
- `test_constant_pool_builder.py` — builder deduplication, Modified UTF-8 handling, MethodHandle validation, import/export behavior, overflow guards, and defensive-copy semantics.
- `test_modified_utf8.py` — direct Modified UTF-8 codec coverage for NUL, supplementary characters, round-tripping, and malformed byte rejection.
- `test_model.py` — mutable editing model: from-scratch creation of `ClassModel`/`MethodModel`/`FieldModel`/`CodeModel`, `from_classfile()` symbolic resolution with error handling for malformed constant-pool references, `from_bytes()` convenience, round-trip `ClassFile → ClassModel → to_classfile()` equivalence across every compiled Java source fixture under `tests/resources/` (including multi-class outputs such as `Outer$Inner.class` and the helper/interface classes generated from `HierarchyFixture.java`), in-place mutation (add/remove fields and methods, rename class, change access flags), and ownership-boundary tests confirming the model does not share mutable state with the source or lowered `ClassFile`.
- `test_labels.py` — label/layout lowering coverage: offset resolution for linear, forward, backward, and multi-target branches; adjacent/terminal/dangling labels; duplicate label rejection; byte-size verification for every instruction subclass (including switch padding at offsets 0–3); automatic `GOTO_W`/`JSR_W` promotion for both forward and backward overflow; cascading promotion; all 16 conditional-branch inversions (parametrized); editing workflows showing offset recalculation after instruction insertion and removal; dynamic addition of exception handlers and debug entries; code-length boundary enforcement (65535 passes, 65536 raises); lifted exception/debug metadata reconstruction; and symbolic lifting from both manual raw `CodeAttr` fixtures and compiled control-flow bytecode
- `test_operands.py` — symbolic operand wrapper coverage: constructor validation (opcode rejection, JVM `u1`/`u2`/`i2` bounds, bootstrap-index validation, MethodHandle reference-kind validation), mapping-table sanity (roundtrips for `_IMPLICIT_VAR_SLOTS`/`_VAR_SHORTCUTS`/`_WIDE_TO_BASE`/`_BASE_TO_WIDE`), lifting tests for all 9 wrapper families (FieldInsn/MethodInsn/InterfaceMethodInsn/TypeInsn/LdcInsn/MultiANewArrayInsn/VarInsn/IIncInsn/InvokeDynamicInsn) from compiled `InstructionShowcase.java`, VarInsn normalisation (implicit → VarInsn, no raw implicit opcode survives lifting), LDC value-type discrimination (including `LdcMethodHandle` and `LdcDynamic` lowering coverage), lowering encoding-selection tests (implicit/explicit/WIDE for VarInsn; narrow/wide for IIncInsn; LDC/LDC_W/LDC2_W for LdcInsn based on CP index range; InterfaceMethodref vs Methodref for MethodInsn.is_interface; auto-computed count for InterfaceMethodInsn), mutation-time validation during lowering for mutable wrappers, edit-from-scratch tests (FieldInsn CP entry creation, deduplication of identical LdcInsn, mixed symbolic + raw instruction lists), and InstructionShowcase round-trip verification
- `test_hierarchy.py` — hierarchy-resolution coverage: adapters from parsed `ClassFile` and `ClassModel`, linear superclass walks with an implicit `java/lang/Object` root, supertype traversal through superclass and interface edges, subtype checks, common-superclass lookup, explicit missing-class and cycle failures, and method-override detection across same-package package-private methods, protected/public inheritance, interface methods, and non-overridable final/static/private declarations
- `test_helpers.py` — persistent Java fixture-cache coverage for `tests/helpers.py`, including cache hits across separate temp directories, invalidation when source contents change, and invalidation when `javac --release` changes.

Test fixtures are generated from Java source in `tests/resources/` rather than relying on large binary artifacts. Helper utilities in `tests/helpers.py` compile focused fixtures and small JARs with `javac`, persist those outputs in a content-addressed cache under `.pytest_cache/pytecode-javac`, and only re-run `javac` when the ordered source list, source contents, `--release`, or `javac` identity changes.
