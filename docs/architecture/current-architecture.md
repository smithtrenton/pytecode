# Current architecture

The codebase is currently organized as a parser/lowering/emission pipeline with a strongly typed read model.

## Runtime and packaging

- Requires Python 3.14+ (`pyproject.toml`)
- Ships a `py.typed` marker so downstream type checkers can consume package types
- Keeps the core library runtime dependency-free
- Uses Ruff lint and format checks, basedpyright, and `pytest`-based validation in development (see `README.md`)

## Public entry points

- `pytecode.ClassReader`
  - Constructed directly from classfile bytes, or via `ClassReader.from_bytes()` / `ClassReader.from_file()`
  - Parses eagerly during initialization
  - Produces `class_info`, an `info.ClassFile` dataclass tree
- `pytecode.ClassWriter`
  - Stateless serializer for `info.ClassFile`
  - Writes classfile structures back to bytes in JVM spec order
  - Recomputes emitted lengths/counts from the live dataclass tree and preserves unknown attribute payloads verbatim
- `pytecode.JarFile`
  - Reads the contents of a JAR
  - Separates `.class` entries from non-class resources
  - Parses classes via `ClassReader`
  - Supports explicit archive entry mutation via `add_file()` / `remove_file()`
  - Rewrites archives safely via `rewrite()`, optionally lowering `.class` entries through `ClassModel`
  - Preserves signature artifacts as pass-through resources but does not re-sign modified archives

- `pytecode.ClassModel`
  - Mutable editing model for JVM class files
  - Uses symbolic (resolved) references instead of raw constant-pool indexes
  - Constructed from a parsed `ClassFile` via `ClassModel.from_classfile()` or from raw bytes via `ClassModel.from_bytes()`
  - Produces a spec-faithful `ClassFile` via `to_classfile()` and can serialize directly via `to_bytes()`

These are the current public exports in `pytecode.__init__`.

Advanced transform-composition helpers intentionally live in `pytecode.transforms` rather than `pytecode.__init__`, keeping the top-level API small while still exposing a supported submodule for pipelines, matchers, lifting helpers, and selectors.

## Module responsibilities

### `pytecode/_internal/bytes_utils.py`

Low-level big-endian binary I/O primitives for both reading and writing. This is the I/O foundation for class parsing and classfile emission.

**Read side**: Standalone `_read_u1/i1/u2/i2/u4/i4/_read_bytes` helper functions and a stateful `BytesReader` that tracks a cursor offset.

**Write side**: Standalone `_write_u1/i1/u2/i2/u4/i4/_write_bytes` helper functions and a stateful `BytesWriter` that appends to an internal buffer. `BytesWriter` provides `write_u1/i1/u2/i2/u4/i4/bytes` methods, `align(n)` for opcode-alignment padding, and a full set of `reserve_u1/i1/u2/i2/u4/i4` and `patch_u1/i1/u2/i2/u4/i4` methods for deferred length-prefixed structures.

### `pytecode/classfile/reader.py`

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

### `pytecode/classfile/writer.py`

Deterministic classfile emission introduced for issue [#12](https://github.com/smithtrenton/pytecode/issues/12). This module provides:

- **`ClassWriter.write()`** — serializes an `info.ClassFile` back to raw `.class` bytes
- **Spec-order serialization** — writes header, constant pool, class metadata, fields, methods, and attributes in JVM classfile order
- **Full dataclass-surface support** — handles all parsed constant-pool entries, raw instructions, stack map frames, annotations/type annotations, module metadata, record components, and preserved unknown attributes
- **Derived metadata recomputation** — emits attribute lengths, code lengths, and count fields from the current in-memory structure rather than trusting stale counters
- **Roundtrip fidelity focus** — preserves imported constant-pool ordering and, together with `ClassModel.to_bytes()`, now underpins the landed Tier 1 byte-for-byte roundtrip tests

### `pytecode/classfile/constant_pool.py`

Typed dataclasses for all 17 constant-pool entry types plus the `ConstantPoolInfoType` enum mapping tags to dataclasses. The enum embeds both the numeric tag and the corresponding dataclass, so new constant types only require adding an enum member.

### `pytecode/classfile/modified_utf8.py`

Shared JVM Modified UTF-8 codec helpers for `CONSTANT_Utf8` values. This module
centralizes spec-correct encoding and decoding of:

- embedded NUL (`U+0000`) using the two-byte modified form
- supplementary characters via UTF-16 surrogate pairs
- malformed byte-sequence rejection (for example, illegal four-byte UTF-8 forms)

It is used by `ConstantPoolBuilder`, `ClassReader`, and test helpers so
constant-pool string handling stays consistent across parsing, editing, and
fixtures.

### `pytecode/classfile/attributes.py`

Typed dataclasses for classfile attributes and nested structures (verification types, stack map frames, annotations, type annotations, module info, record components, etc.). It also defines `AttributeInfoType`, which maps attribute names to concrete dataclasses via an enum with a `_missing_` fallback. Unknown attribute names are routed to `UnimplementedAttr`, allowing parse-time preservation of vendor or future attributes.

### `pytecode/classfile/instructions.py`

Typed dataclasses for decoded bytecode instructions and operand shapes covering local indexes, constant-pool indexes, branches, switches, and other operand families, plus the `InsnInfoType` opcode enum that maps supported JVM instruction encodings to instruction record types, and an `ArrayType` enum for `newarray`.

### `pytecode/classfile/info.py`

Top-level dataclasses representing the parsed classfile structure:

- `ClassFile`
- `FieldInfo`
- `MethodInfo`

These dataclasses hold references to attribute and constant-pool structures defined elsewhere.

### `pytecode/classfile/constants.py`

Enums and flags representing JVM constants, access flags, verification types, and target-type metadata. (The former `FieldType` enum was removed here and superseded by `BaseType` in `descriptors.py`.)

### `pytecode/classfile/descriptors.py`

Descriptor and generic signature utilities. Provides:

- **Data model**: `BaseType` enum (8 JVM primitives), `VoidType`, `ObjectType`, `ArrayType`, `MethodDescriptor` frozen dataclasses, and a full generic signature type hierarchy (`ClassTypeSignature`, `TypeVariable`, `ArrayTypeSignature`, `TypeArgument`, `TypeParameter`, `ClassSignature`, `MethodSignature`)
- **Parsing**: `parse_field_descriptor()`, `parse_method_descriptor()`, `parse_class_signature()`, `parse_method_signature()`, `parse_field_signature()` — all recursive-descent, raising `ValueError` with position context on malformed input, including malformed internal names, empty path segments, and empty inner-class suffixes
- **Construction**: `to_descriptor()` — converts structured types back to JVM descriptor strings (round-trip)
- **Slot helpers**: `slot_size()` and `parameter_slot_count()` — category-aware (long/double occupy 2 slots)
- **Validation**: `is_valid_field_descriptor()` and `is_valid_method_descriptor()` with spec-aware internal-name checks

All types are imported directly from `pytecode.classfile.descriptors`.

### `pytecode/edit/constant_pool_builder.py`

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

### `pytecode/edit/model.py`

Mutable editing model for safe classfile manipulation. This module provides the higher-level object model described in issue [#6](https://github.com/smithtrenton/pytecode/issues/6), implementing Design A (Mutable Dataclasses). Four core types form the user-facing editing layer, with label-specific helpers delegated to `pytecode/edit/labels.py`:

- **`ClassModel`** — top-level mutable representation of a class file. Fields use symbolic (resolved) references: `name: str`, `super_name: str | None`, `interfaces: list[str]`, along with `access_flags`, `version: tuple[int, int]`, lists of `FieldModel` and `MethodModel`, class-level attributes, and a `ConstantPoolBuilder`. Provides `from_classfile()` and `from_bytes()` factory methods for construction, `to_classfile()` for lowering back to a spec-faithful `ClassFile`, and `to_bytes()` for direct emission via `ClassWriter`.
- **`MethodModel`** — mutable representation of a method with resolved `name: str` and `descriptor: str`, `access_flags`, an optional `CodeModel` (`None` for abstract/native methods), and non-Code attributes. The raw `Code` attribute is lifted out of the attribute list into the dedicated `code` field.
- **`FieldModel`** — mutable representation of a field with resolved `name: str` and `descriptor: str`, `access_flags`, and attributes.
- **`CodeModel`** — wraps a mixed instruction stream (`InsnInfo` plus `Label` pseudo-instructions), symbolic exception handlers, lifted line/local-variable debug tables, `max_stack`, `max_locals`, and residual nested Code attributes. During `from_classfile()`, all supported instruction families are lifted to symbolic wrappers: branch/switch instructions become `BranchInsn`/`LookupSwitchInsn`/`TableSwitchInsn`; field/method/type/LDC/invoke-dynamic/multianewarray constant-pool instructions become their corresponding operand wrappers from `pytecode.edit.operands`; local-variable slot instructions (including all implicit `ILOAD_0`–`ASTORE_3` variants and WIDE forms) become `VarInsn`. All symbolic wrappers lower back to spec-shaped raw instructions during `to_classfile()`.

 The model carries a `ConstantPoolBuilder` seeded from the original constant pool so that raw attributes and any still-raw instruction operands remain valid through editing. Symbolic references are resolved during `from_classfile()` and re-allocated during `to_classfile()`. Both conversion directions use deep copies for all mutable raw structures they retain (attribute lists, instruction records, and constant-pool-backed payloads) so the `ClassModel` owns its data independently from the source `ClassFile` — consistent with the defensive-copy convention already used by `ConstantPoolBuilder`. `CodeModel` also preserves nested `Code`-attribute ordering metadata so unmodified `ClassModel.to_bytes()` roundtrips can remain byte-identical.

For the design rationale behind this editing model, see [editing model design rationale](../design/editing-model.md).

### `pytecode/transforms/__init__.py`

Composable transform helpers layered on top of the mutable editing model introduced by issue [#6](https://github.com/smithtrenton/pytecode/issues/6). This module provides the current transform surface without introducing a second object model:

- **Transform protocols** — `ClassTransform`, `FieldTransform`, `MethodTransform`, and `CodeTransform` define typed in-place callable shapes. `FieldTransform` and `MethodTransform` receive the owning `ClassModel` as a second argument; `CodeTransform` receives the owning `MethodModel` and `ClassModel` so transforms can inspect their traversal context
- **`Pipeline` / `pipeline()`** — deterministic class-transform composition; pipelines are themselves callable so they slot directly into `JarFile.rewrite(transform=...)`
- **Lifting helpers** — `on_classes()`, `on_fields()`, `on_methods()`, and `on_code()` adapt lower-level transforms onto `ClassModel` traversal while preserving in-place ownership boundaries and passing owning context; the field/method/code lifting helpers also support owner-class filtering
- **`Matcher` DSL** — callable `Matcher` predicates with `&` / `|` / `~` composition and readable reprs
- **Selection helpers** — exact-match, regex, semantic, and access-flag convenience helpers for classes, fields, and methods, plus predicate combinators `all_of()`, `any_of()`, and `not_()` for callers that prefer functional composition

Traversal of fields and methods uses collection snapshots so transforms can mutate `ClassModel.fields` / `ClassModel.methods` without changing which original elements are visited during the current pass.

### `pytecode/analysis/hierarchy.py`

Hierarchy-resolution helpers introduced for issue [#8](https://github.com/smithtrenton/pytecode/issues/8). This module provides:

- **Resolved snapshots** — `ResolvedClass` and `ResolvedMethod` frozen dataclasses for hierarchy-relevant class metadata, plus `InheritedMethod` for reporting matching inherited declarations
- **Pluggable interface** — `ClassResolver`, a minimal protocol that resolves an internal class name to a `ResolvedClass | None`
- **Built-in implementation** — `MappingClassResolver` for in-memory hierarchy graphs, with `from_classfiles()` and `from_models()` convenience constructors
- **Query helpers** — `iter_superclasses()`, `iter_supertypes()`, `is_subtype()`, `common_superclass()`, and `find_overridden_methods()`
- **Explicit failure modes** — `UnresolvedClassError` for missing classes and `HierarchyCycleError` for malformed ancestry loops

To keep ordinary project graphs ergonomic, the module treats `java/lang/Object` as an implicit root if a resolver does not provide it explicitly; otherwise missing hierarchy data is surfaced as an error rather than guessed.

### `pytecode/edit/operands.py`

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

### `pytecode/edit/labels.py`

Label-based bytecode editing helpers and lowering utilities introduced for issue [#7](https://github.com/smithtrenton/pytecode/issues/7). This module owns the symbolic control-flow layer:

- **`Label`** — identity-based pseudo-instruction marker for bytecode positions
- **`BranchInsn` / `LookupSwitchInsn` / `TableSwitchInsn`** — editing-model control-flow instructions that target labels instead of raw offsets
- **`ExceptionHandler` / `LineNumberEntry` / `LocalVariableEntry` / `LocalVariableTypeEntry`** — lifted exception/debug metadata bound to labels rather than byte offsets
- **`resolve_labels()`** — computes byte offsets for labels and instructions in a mixed `InsnInfo | Label` stream; for single-slot `LdcInsn` values, exact sizing uses a provided `ConstantPoolBuilder` context without mutating the live pool
- **`lower_code()`** — lowers symbolic code back to a raw `CodeAttr`, recalculating offsets and switch padding, promoting `GOTO`/`JSR` to wide forms, inverting overflowing conditional branches, reconstructing lifted debug attributes, and lowering all operand wrappers from `pytecode.edit.operands` to spec-shaped raw instructions with correct CP index allocation

For the broader design rationale, historical trade-offs, and follow-up ideas behind this editing model, see [editing model design rationale](../design/editing-model.md).

### `pytecode/analysis/__init__.py`

Control-flow graph construction and stack/local simulation introduced for issue [#9](https://github.com/smithtrenton/pytecode/issues/9). This module provides the analysis layer that sits between the editing model, frame computation, and validation:

- **Verification type system** — a `VType` union (VTop, VInteger, VFloat, VLong, VDouble, VNull, VObject, VUninitializedThis, VUninitialized) mirroring JVM spec §4.10.1.2. Helper functions `vtype_from_descriptor()`, `is_category2()`, `is_reference()`, and `merge_vtypes()` for type conversions and join-point merging.
- **Opcode metadata** — an `OpcodeEffect` dataclass and `OPCODE_EFFECTS` lookup table covering the supported JVM instruction set with stack pop/push counts, branch/switch/return/unconditional flags. Variable-effect instructions (invoke, field access, LDC, multianewarray) use sentinel values and are computed dynamically during simulation.
- **Frame state** — a frozen `FrameState` dataclass tracking operand stack and local variable slots as `VType` tuples, with category-2-aware `push`/`pop`/`set_local`/`get_local` operations. `initial_frame()` builds the entry frame from a `MethodModel`'s descriptor and access flags.
- **CFG construction** — `build_cfg()` partitions a `CodeModel`'s instruction stream into `BasicBlock` nodes with fall-through, branch, switch, and exception-handler edges. Block leaders are identified from branch targets, exception handler labels, and post-terminal instructions.
- **Stack simulation** — `simulate()` performs forward dataflow analysis over the CFG using a worklist algorithm, propagating `FrameState` through each instruction and merging at join points. Returns a `SimulationResult` with per-block entry/exit states, computed `max_stack`, and `max_locals`.
- **Error types** — `AnalysisError`, `StackUnderflowError`, `InvalidLocalError`, and `TypeMergeError` for structured simulation diagnostics.

The module operates on `CodeModel` (the symbolic editing model) and accepts an optional `ClassResolver` from `pytecode.analysis.hierarchy` for reference-type merging at join points, defaulting to conservative `java/lang/Object` collapse when unavailable. It now provides max_stack/max_locals recomputation and StackMapTable generation ([#10](https://github.com/smithtrenton/pytecode/issues/10)) and is consumed by the validation layer ([#11](https://github.com/smithtrenton/pytecode/issues/11)).

### `pytecode/analysis/verify.py`

Structural classfile validation with structured diagnostics, introduced for issue [#11](https://github.com/smithtrenton/pytecode/issues/11). This module validates both the parsed `ClassFile` model and the mutable `ClassModel`:

- **Diagnostics model** — `Diagnostic` dataclass with `severity` (`Severity`: ERROR, WARNING, INFO), `category` (`Category`: MAGIC, VERSION, CONSTANT_POOL, ACCESS_FLAGS, CLASS_STRUCTURE, FIELD, METHOD, CODE, ATTRIBUTE, DESCRIPTOR), `location` (`Location` with optional class name, method/field, CP index, bytecode offset), and a human-readable `message`
- **Entry points** — `verify_classfile(cf, *, fail_fast=False)` validates a parsed `ClassFile`; `verify_classmodel(cm, *, fail_fast=False)` validates a mutable `ClassModel`. Both return `list[Diagnostic]` collecting all issues by default; with `fail_fast=True` they raise `FailFastError` on the first ERROR-severity diagnostic
- **Checks performed** — magic number, version range, constant-pool well-formedness (tag validity, index bounds, structural constraints), access flag mutual exclusions, class structure (this_class, super_class, interfaces), field and method constraints, Code attribute validation (branches, exception handlers, CP reference validity), attribute versioning, descriptor validation, and ClassModel-specific label validity

Not exported from `pytecode.__init__`; import directly: `from pytecode.analysis.verify import verify_classfile, verify_classmodel`.

### CFG differential validation infrastructure

Issue [#17](https://github.com/smithtrenton/pytecode/issues/17) added a JVM-backed differential validation layer for `build_cfg()` in the test suite:

- **`tests/resources/oracle/RecordingAnalyzer.java`** compiles against ASM 9.7.1 (`asm`, `asm-tree`, `asm-analysis`, `asm-util`) and records instruction-level normal edges, exceptional edges, and try/catch table entries as JSON.
- **`tests/cfg_oracle.py`** parses that JSON and normalizes both ASM output and `pytecode.analysis.ControlFlowGraph` instances into the same block-level comparison model: block spans, normal successor sets, exception handler sets, and entry block identity.
- **`tests/test_cfg_oracle.py`** differentially validates both `tests/resources/CfgFixture.java` and `tests/resources/CfgEdgeCaseFixture.java`. The suite uses the `oracle` pytest marker, skips cleanly when the JVM or ASM jars are unavailable, and caches downloaded ASM jars under `.pytest_cache/pytecode-oracle` while also honoring `tests/resources/oracle/lib`.

### `pytecode/archive/__init__.py`

JAR container support. This module now covers archive reading, class/non-class separation, in-memory entry mutation, and safe rewrite-to-disk behavior. `JarFile.add_file()` and `remove_file()` update the in-memory archive state, while `JarFile.rewrite()` can either copy entries verbatim or lift `.class` entries through `ClassModel` for in-place transforms before writing a temporary archive and replacing the destination. The `transform=` parameter accepts any supported class transform, including callable `Pipeline` objects from `pytecode.transforms`. Signature-related files are preserved as ordinary resources and are not re-signed automatically.

### `run.py`

A repository smoke-test script that parses a JAR, writes pretty-printed parsed class output under a `parsed` subtree, and writes `.class` files from lifted `ClassModel` instances under a `rewritten` subtree alongside copied resource files. This is primarily a manual validation utility for ad hoc inspection of real archives during development.

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

### JAR rewriting

1. Callers optionally mutate the in-memory archive state with `JarFile.add_file()` / `remove_file()`.
2. `JarFile.rewrite()` iterates the current entry order and copies non-class resources verbatim.
3. When class rewriting is requested, `.class` entries are lifted through `ClassModel`, transformed in place, and lowered back to bytes with the existing classfile emission stack.
4. The updated archive is written to a temporary ZIP and atomically replaced at the destination path.
5. After a successful rewrite, `JarFile` refreshes itself from disk so its in-memory state matches the written archive metadata.

## Design characteristics

### Strengths

- Spec-shaped datamodels make the parser output explicit and inspectable
- The parser retains structural detail rather than flattening everything into dictionaries
- Unknown attributes can still be carried as bytes
- Full opcode coverage with no gaps in the standard instruction set
- JAR-level parsing and rewrite support give the project a practical integration point
- The codebase is dependency-light and easy to run locally
- Enum-driven dispatch in both `ConstantPoolInfoType` and `AttributeInfoType` makes adding new types mechanical

### Current constraints

- Parsing is eager and tightly coupled to `ClassReader`
- Most parser behavior lives in one large module (`class_reader.py`), so parsing remains a maintenance hotspot even though emission now lives in `class_writer.py`
- The editing model now uses labels for control flow, exception ranges, and debug scopes, and symbolic operand wrappers for all major non-control-flow instruction families; only raw pass-through instructions (`BIPUSH`, `SIPUSH`, `NEWARRAY`, and zero-operand `InsnInfo`) remain in their spec-shaped form
- Signed-JAR artifacts are preserved as pass-through bytes during rewrite, but `pytecode` does not generate replacement signatures for modified archives
- Binary classfile emission, archive rewrite support, transform composition via `pytecode.transforms`, and the four validation tiers are implemented via `ClassWriter.write()`, `ClassModel.to_bytes()`, `JarFile.rewrite()`, the current pipeline/matcher helpers with context-passing transform protocols, and the validation suite

## Test coverage

The test suite provides both integration-level and unit-level coverage:

**Unit tests** ([#2](https://github.com/smithtrenton/pytecode/issues/2) — done):

- `test_attributes.py` — per-attribute-type parsing for the standard attribute families, stack map frame variants, verification types, annotation element values, type annotations, and the `UnimplementedAttr` fallback.
- `test_instructions.py` — instruction decoding for all operand shapes including no-operand, local variable index, constant-pool index, bipush/sipush, branch16/branch32, iinc, invokedynamic, invokeinterface, newarray, multianewarray, lookupswitch, tableswitch, and wide variants.
- `test_constant_pool.py` — all 17 constant-pool entry types, Long/Double double-slot handling, Modified UTF-8 `CONSTANT_Utf8` cases, mixed pool parsing, and unknown tag errors.
- `test_class_reader.py` — classfile parsing including magic number validation, version field validation, constant-pool indexing, access flags, interfaces, fields, methods, Code attributes, and error paths for invalid/truncated classfiles.
- `test_bytes_utils.py` — all primitive byte readers and writers (`_read_*`/`_write_*`), `BytesReader` stateful cursor, rewind, and buffer overrun, `BytesWriter` sequential writes, alignment padding, reserve/patch, and round-trip read↔write.
- `test_jar.py` — JAR reading, class/non-class separation, path normalization, explicit entry mutation, safe rewrite behavior, metadata/resource preservation, signed-artifact pass-through, atomic failure handling, and compiled JAR integration.
- `test_transforms.py` — pipeline ordering, matcher composition, regex/semantic/access helper coverage, owner-filtered lifting, snapshot traversal semantics, abstract/native/no-code method handling, runtime guardrails, and `JarFile.rewrite()` transform interop.
- `test_descriptors.py` — all 8 base types, object and array types, method descriptors, slot counting (long/double = 2 slots), round-trip parse → construct → parse, malformed descriptor error handling, generic class/method/field signatures with type parameters, wildcards (`+`/`-`/`*`), inner classes, type variables, throws clauses, and invalid internal-name edge cases.
- `test_constant_pool_builder.py` — builder deduplication, Modified UTF-8 handling, MethodHandle validation, import/export behavior, overflow guards, and defensive-copy semantics.
- `test_modified_utf8.py` — direct Modified UTF-8 codec coverage for NUL, supplementary characters, round-tripping, and malformed byte rejection.
- `test_model.py` — mutable editing model: from-scratch creation of `ClassModel`/`MethodModel`/`FieldModel`/`CodeModel`, `from_classfile()` symbolic resolution with error handling for malformed constant-pool references, `from_bytes()` convenience, round-trip `ClassFile → ClassModel → to_classfile()` equivalence across every compiled Java source fixture under `tests/resources/` (including multi-class outputs such as `Outer$Inner.class` and the helper/interface classes generated from `HierarchyFixture.java`), in-place mutation (add/remove fields and methods, rename class, change access flags), and ownership-boundary tests confirming the model does not share mutable state with the source or lowered `ClassFile`.
- `test_labels.py` — label/layout lowering coverage: offset resolution for linear, forward, backward, and multi-target branches; adjacent/terminal/dangling labels; duplicate label rejection; byte-size verification for every instruction subclass (including switch padding at offsets 0–3); automatic `GOTO_W`/`JSR_W` promotion for both forward and backward overflow; cascading promotion; all 16 conditional-branch inversions (parametrized); editing workflows showing offset recalculation after instruction insertion and removal; dynamic addition of exception handlers and debug entries; code-length boundary enforcement (65535 passes, 65536 raises); lifted exception/debug metadata reconstruction; and symbolic lifting from both manual raw `CodeAttr` fixtures and compiled control-flow bytecode
- `test_operands.py` — symbolic operand wrapper coverage: constructor validation (opcode rejection, JVM `u1`/`u2`/`i2` bounds, bootstrap-index validation, MethodHandle reference-kind validation), mapping-table sanity (roundtrips for `_IMPLICIT_VAR_SLOTS`/`_VAR_SHORTCUTS`/`_WIDE_TO_BASE`/`_BASE_TO_WIDE`), lifting tests for all 9 wrapper families (FieldInsn/MethodInsn/InterfaceMethodInsn/TypeInsn/LdcInsn/MultiANewArrayInsn/VarInsn/IIncInsn/InvokeDynamicInsn) from compiled `InstructionShowcase.java`, VarInsn normalisation (implicit → VarInsn, no raw implicit opcode survives lifting), LDC value-type discrimination (including `LdcMethodHandle` and `LdcDynamic` lowering coverage), lowering encoding-selection tests (implicit/explicit/WIDE for VarInsn; narrow/wide for IIncInsn; LDC/LDC_W/LDC2_W for LdcInsn based on CP index range; InterfaceMethodref vs Methodref for MethodInsn.is_interface; auto-computed count for InterfaceMethodInsn), mutation-time validation during lowering for mutable wrappers, edit-from-scratch tests (FieldInsn CP entry creation, deduplication of identical LdcInsn, mixed symbolic + raw instruction lists), and InstructionShowcase round-trip verification
- `test_hierarchy.py` — hierarchy-resolution coverage: adapters from parsed `ClassFile` and `ClassModel`, linear superclass walks with an implicit `java/lang/Object` root, supertype traversal through superclass and interface edges, subtype checks, common-superclass lookup, explicit missing-class and cycle failures, and method-override detection across same-package package-private methods, protected/public inheritance, interface methods, and non-overridable final/static/private declarations
- `test_helpers.py` — persistent Java fixture-cache coverage for `tests/helpers.py`, including cache hits across separate temp directories, invalidation when source contents change, and invalidation when `javac --release` changes.
- `test_analysis.py` — control-flow graph and simulation coverage: verification type helpers (`vtype_from_descriptor`, `is_category2`, `is_reference`, `merge_vtypes` with and without a resolver), `FrameState` operations (push/pop for category-1 and category-2 types, set_local/get_local with two-slot expansion, stack underflow detection), `initial_frame` for static methods, instance methods, `<init>`, and multi-parameter signatures (including long/double slots), CFG construction (single block, if/else branching, tableswitch, lookupswitch, try-catch exception edges, unconditional GOTO, loops, terminal ATHROW/return blocks), stack simulation for all major opcode families (constants, loads/stores, arithmetic, conversions, comparisons, stack manipulation including DUP/DUP_X1/DUP_X2/DUP2/DUP2_X1/DUP2_X2/SWAP/POP/POP2, field access, method invocations, object creation with NEW→`<init>` uninitialized tracking, type checks, array operations, monitors, IINC, LDC variants), max_stack/max_locals computation, type merging at branch join points, loop convergence via fixed-point iteration, exception-handler pre-instruction state, incompatible join-point merge failures, precise `AALOAD` reference typing, error paths (stack underflow, invalid locals), and integration tests against compiled `CfgFixture.java` methods covering all control-flow patterns.
- `test_verify.py` — structural validation coverage (122 tests): magic number, version range, constant-pool well-formedness, access flag mutual exclusions, class structure, field and method constraints, Code attribute validation (branches, exception handlers, CP reference validity), attribute versioning, descriptor validation, ClassModel label validity, `fail_fast` mode, and diagnostic severity/category/location accuracy.
- `test_validation.py` — multi-release validation-matrix tests: parametrizes the compiled Java fixture corpus across `--release 8, 11, 17, 21, 25`, filtered by each fixture's minimum supported release. For each (fixture, release) pair, it runs byte-for-byte roundtrip (T1), `verify_classfile()` + `javap` structural checks (T2), and JVM loading via `VerifierHarness.java` with `-Xverify:all` (T4), plus execution tests for selected roundtripped fixtures. The Tier 3 CP-aware semantic-diff engine lives in `tests/javap_parser.py` and is unit-tested in `test_javap_parser.py`.
- `test_javap_parser.py` — unit tests for the `javap` output parser and CP-aware semantic diff engine: parsing edge cases, member extraction, instruction operand resolution, and diff severity classification.

Test fixtures are generated from Java source in `tests/resources/` rather than relying on large binary artifacts. Helper utilities in `tests/helpers.py` compile focused fixtures and small JARs with `javac`, persist those outputs in a content-addressed cache under `.pytest_cache/pytecode-javac`, and only re-run `javac` when the ordered source list, source contents, `--release`, or `javac` identity changes. The `FIXTURE_MIN_RELEASES` mapping in `helpers.py` tracks which fixtures require `--release` > 8, and `list_java_resources(max_release=N)` filters accordingly.

Additional validation infrastructure modules:

- `tests/javap_parser.py` — structured parser for `javap -v -p -c` output and CP-aware semantic diff engine. Parses class headers, constant pool, fields, methods with code sections, and attributes into typed dataclasses. The semantic diff resolves all CP index references before comparison, ignoring index ordering differences, and classifies diffs as `error`, `warning`, or `info` severity.
- `tests/jvm_harness.py` — typed Python wrapper for `VerifierHarness.java`. Provides `verify_class()` and `execute_class()` entry points with `VerifyResult`/`ExecutionResult` dataclasses.
- `tests/validation_fixtures.py` — fixture registry mapping each fixture to its minimum release, plus `validation_matrix()` generator producing all valid `(fixture, release)` pairs.
- `tests/resources/VerifierHarness.java` — standalone Java program that reads `.class` bytes from a file, extracts the class name from the constant pool, defines the class via a custom `ClassLoader.defineClass()`, and reports structured JSON output. Run with `-Xverify:all` for strictest verification.
