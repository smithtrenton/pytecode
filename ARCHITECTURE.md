# pytecode architecture

## Purpose

`pytecode` is a Python library for parsing, inspecting, and eventually manipulating JVM class files and bytecode.

The project goal is to provide a Python alternative to Java libraries such as ASM and BCEL. Today, the library is focused on parsing raw classfile bytes into typed Python objects. The longer-term goal is to support safe classfile transformation, verification, and emission of new `.class` files.

## Current status

### Implemented today

- Reading classfile bytes into an in-memory object model
- Parsing all 17 constant-pool entry types
- Parsing fields, methods, interfaces, and class-level metadata
- Parsing `Code` attributes and decoding all 205 standard JVM opcodes (0x00–0xCA) plus WIDE variants into typed instruction records
- Parsing 30 standard attribute types including annotations, stack map tables, module metadata, records, and permitted subclasses
- Preserving unknown or unrecognized attributes as raw bytes through `UnimplementedAttr`
- Reading JAR files and parsing every `.class` entry in them
- Descriptor and signature parsing utilities: structured field/method descriptor types, generic signature parsing (class, method, and field signatures), round-trip construction, slot-size helpers, and stricter validation of malformed internal names and signature segments — see `pytecode/descriptors.py`
- Binary writer foundation: big-endian write primitives, stateful `BytesWriter` with alignment, reserve/patch helpers for length-prefixed structures — see `pytecode/bytes_utils.py`
- Shared JVM Modified UTF-8 codec for `CONSTANT_Utf8` values — see `pytecode/modified_utf8.py`
- Unit test coverage for all attribute types, instruction operand shapes, constant-pool entries, byte utilities, class reader, JAR handling, descriptor/signature parsing, Modified UTF-8 handling, constant-pool builder hardening, and mutable editing model (650 tests)
- Constant-pool management: `ConstantPoolBuilder` with Modified UTF-8 handling, deduplication, symbol-table lookups, compound-entry auto-creation, MethodHandle/import validation, double-slot handling, defensive-copy reads/exports, and deterministic ordering — see `pytecode/constant_pool_builder.py`
- Mutable editing model (Phase 1): `ClassModel`, `MethodModel`, `FieldModel`, `CodeModel` — mutable dataclasses with symbolic references (resolved names instead of CP indexes), bidirectional conversion to/from the parsed `ClassFile` model, `ConstantPoolBuilder` integration for raw attribute/instruction passthrough — see `pytecode/model.py`

### Not implemented yet

- Label-based instruction editing and branch-offset recalculation for safe bytecode mutation ([#7](https://github.com/smithtrenton/pytecode/issues/7))
- Class hierarchy resolution, control-flow analysis, and frame/max-stack recomputation ([#8](https://github.com/smithtrenton/pytecode/issues/8), [#9](https://github.com/smithtrenton/pytecode/issues/9), [#10](https://github.com/smithtrenton/pytecode/issues/10))
- Structured validation/diagnostics and binary classfile emission ([#11](https://github.com/smithtrenton/pytecode/issues/11), [#12](https://github.com/smithtrenton/pytecode/issues/12))
- Archive rewrite support for writing transformed JARs back to disk ([#15](https://github.com/smithtrenton/pytecode/issues/15))

## Current architecture

The codebase is currently organized as a parser pipeline with a strongly typed read model.

### Runtime and packaging

- Requires Python 3.14+ (`pyproject.toml`)
- Ships a `py.typed` marker so downstream type checkers can consume package types
- Keeps the core library runtime dependency-free
- Uses Ruff, basedpyright, and `pytest`-based validation in development (see `README.md`)

### Public entry points

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

### Module responsibilities

#### `pytecode\bytes_utils.py`

Low-level big-endian binary I/O primitives for both reading and writing. This is the I/O foundation for class parsing and future classfile emission.

**Read side**: Standalone `_read_u1/i1/u2/i2/u4/i4/_read_bytes` helper functions and a stateful `BytesReader` that tracks a cursor offset.

**Write side**: Standalone `_write_u1/i1/u2/i2/u4/i4/_write_bytes` helper functions and a stateful `BytesWriter` that appends to an internal buffer. `BytesWriter` provides `write_u1/i1/u2/i2/u4/i4/bytes` methods, `align(n)` for opcode-alignment padding, and a full set of `reserve_u1/i1/u2/i2/u4/i4` and `patch_u1/i1/u2/i2/u4/i4` methods for deferred length-prefixed structures.

#### `pytecode\class_reader.py`

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

#### `pytecode\constant_pool.py`

Typed dataclasses for all 17 constant-pool entry types plus the `ConstantPoolInfoType` enum mapping tags to dataclasses. The enum embeds both the numeric tag and the corresponding dataclass, so new constant types only require adding an enum member.

#### `pytecode\modified_utf8.py`

Shared JVM Modified UTF-8 codec helpers for `CONSTANT_Utf8` values. This module
centralizes spec-correct encoding and decoding of:

- embedded NUL (`U+0000`) using the two-byte modified form
- supplementary characters via UTF-16 surrogate pairs
- malformed byte-sequence rejection (for example, illegal four-byte UTF-8 forms)

It is used by `ConstantPoolBuilder`, `ClassReader`, and test helpers so
constant-pool string handling stays consistent across parsing, editing, and
fixtures.

#### `pytecode\attributes.py`

Over 80 typed dataclasses for classfile attributes and nested structures (verification types, stack map frames, annotations, type annotations, module info, record components, etc.). It also defines `AttributeInfoType`, which maps attribute names to concrete dataclasses via an enum with a `_missing_` fallback. Unknown attribute names are routed to `UnimplementedAttr`, allowing parse-time preservation of vendor or future attributes.

#### `pytecode\instructions.py`

Typed dataclasses for decoded bytecode instructions and operand shapes (12 operand-specific subclasses covering local indexes, constant pool indexes, branches, switches, etc.), plus the `InsnInfoType` opcode enum (205 standard opcodes plus WIDE variants) that maps byte values to instruction record types, and an `ArrayType` enum for `newarray`.

#### `pytecode\info.py`

Top-level dataclasses representing the parsed classfile structure:

- `ClassFile`
- `FieldInfo`
- `MethodInfo`

These dataclasses hold references to attribute and constant-pool structures defined elsewhere.

#### `pytecode\constants.py`

Enums and flags representing JVM constants, access flags, verification types, and target-type metadata. (The former `FieldType` enum was removed here and superseded by `BaseType` in `descriptors.py`.)

#### `pytecode\descriptors.py`

Descriptor and generic signature utilities. Provides:

- **Data model**: `BaseType` enum (8 JVM primitives), `VoidType`, `ObjectType`, `ArrayType`, `MethodDescriptor` frozen dataclasses, and a full generic signature type hierarchy (`ClassTypeSignature`, `TypeVariable`, `ArrayTypeSignature`, `TypeArgument`, `TypeParameter`, `ClassSignature`, `MethodSignature`)
- **Parsing**: `parse_field_descriptor()`, `parse_method_descriptor()`, `parse_class_signature()`, `parse_method_signature()`, `parse_field_signature()` — all recursive-descent, raising `ValueError` with position context on malformed input, including malformed internal names, empty path segments, and empty inner-class suffixes
- **Construction**: `to_descriptor()` — converts structured types back to JVM descriptor strings (round-trip)
- **Slot helpers**: `slot_size()` and `parameter_slot_count()` — category-aware (long/double occupy 2 slots)
- **Validation**: `is_valid_field_descriptor()` and `is_valid_method_descriptor()` with spec-aware internal-name checks

All types are imported directly from `pytecode.descriptors`.

#### `pytecode\constant_pool_builder.py`

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

#### `pytecode\model.py`

Mutable editing model for safe classfile manipulation. This module provides the higher-level object model described in issue [#6](https://github.com/smithtrenton/pytecode/issues/6), implementing Design A (Mutable Dataclasses). Four types form the editing layer:

- **`ClassModel`** — top-level mutable representation of a class file. Fields use symbolic (resolved) references: `name: str`, `super_name: str | None`, `interfaces: list[str]`, along with `access_flags`, `version: tuple[int, int]`, lists of `FieldModel` and `MethodModel`, class-level attributes, and a `ConstantPoolBuilder`. Provides `from_classfile()` and `from_bytes()` factory methods for construction, and `to_classfile()` for lowering back to a spec-faithful `ClassFile`.
- **`MethodModel`** — mutable representation of a method with resolved `name: str` and `descriptor: str`, `access_flags`, an optional `CodeModel` (None for abstract/native methods), and non-Code attributes. The Code attribute is lifted out of the attribute list into the dedicated `code` field.
- **`FieldModel`** — mutable representation of a field with resolved `name: str` and `descriptor: str`, `access_flags`, and attributes.
- **`CodeModel`** — wraps the instruction list, exception handler table, `max_stack`, `max_locals`, and nested Code attributes. Serves as the extension point for label-based instruction editing ([#7](https://github.com/smithtrenton/pytecode/issues/7)).

The model carries a `ConstantPoolBuilder` seeded from the original constant pool so that raw attributes and instructions (which still contain CP indexes) remain valid through editing. Symbolic references are resolved during `from_classfile()` and re-allocated during `to_classfile()`. Both conversion directions use deep copies for all mutable raw structures (attribute lists, instruction lists, exception tables) so the `ClassModel` owns its data independently from the source `ClassFile` — consistent with the defensive-copy convention already used by `ConstantPoolBuilder`.

For the broader design rationale, trade-offs, and future phases behind this editing model, see `DESIGN-EDITING-MODEL.md`.

#### `pytecode\jar.py`

JAR container support. This is currently a convenience layer around archive reading plus class parsing: it separates `.class` entries from non-class resources and parses each class via `ClassReader`. JAR rewrite support remains future work ([#15](https://github.com/smithtrenton/pytecode/issues/15)).

#### `run.py`

A repository smoke-test script that parses a JAR and writes pretty-printed class output alongside copied resource files. This is primarily a manual validation utility for current parser behavior against the checked-in `225.jar` sample.

#### `tools\parse_wiki_instructions.py`

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

## Design characteristics of the current implementation

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
- The editing model (`model.py`) provides symbolic references for class/field/method names but still carries raw constant-pool indexes inside instructions and attributes — these will be resolved by the label system ([#7](https://github.com/smithtrenton/pytecode/issues/7)) and attribute resolution (future issues)
- There is no classfile emission layer yet — `ClassModel.to_classfile()` produces a spec-model `ClassFile` but binary serialization requires issue [#12](https://github.com/smithtrenton/pytecode/issues/12)

### Test coverage

The test suite provides both integration-level and unit-level coverage (650 tests total):

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
- `test_model.py` — mutable editing model: from-scratch creation of `ClassModel`/`MethodModel`/`FieldModel`/`CodeModel`, `from_classfile()` symbolic resolution with error handling for malformed constant-pool references, `from_bytes()` convenience, round-trip `ClassFile → ClassModel → to_classfile()` equivalence across 11 Java fixture classes (interfaces, abstract classes, enums, multi-interface, field access flag variants, try/catch exception handlers, annotations, static initializers, generic classes with `Signature` attributes, outer/inner classes with `InnerClasses` attributes), in-place mutation (add/remove fields and methods, rename class, change access flags), and ownership-boundary tests confirming the model does not share mutable state with the source or lowered `ClassFile`.

Test fixtures are generated from Java source in `tests/resources/` rather than relying on large binary artifacts. Helper utilities in `tests/helpers.py` compile focused fixtures and small JARs with `javac` during tests.

## Recommended target architecture

To reach the stated project goal, the library should evolve from a parser into a layered classfile toolkit.

### 1. Binary I/O layer ([#4](https://github.com/smithtrenton/pytecode/issues/4))

Both read and write primitives now live in `bytes_utils.py`.

Responsibilities:

- read primitive JVM binary values
- write primitive JVM binary values
- handle alignment and length-prefixed structures
- provide reusable helpers for parser and emitter code

Module:

- `bytes_utils.py` — read side (`BytesReader`) and write side (`BytesWriter`)

### 2. Parsed spec model layer

Retain the existing spec-faithful dataclasses for exact representation of on-disk structures.

Responsibilities:

- mirror the classfile format accurately
- preserve raw indexes and attribute layout
- serve as the lossless interchange model between parser and writer

This is the layer you already have today.

### 3. Mutable editing model ([#6](https://github.com/smithtrenton/pytecode/issues/6))

Add a higher-level object model for safe manipulation.This should not force users to hand-edit raw constant-pool indexes and branch offsets.

Responsibilities:

- editable classes, fields, methods, and attributes
- symbolic references instead of bare indexes where possible
- labels for branch targets (supporting forward references)
- helpers for adding, removing, and replacing instructions
- helpers for creating or updating constants and descriptors
- automatic offset and padding recalculation (including `TABLESWITCH`/`LOOKUPSWITCH` alignment)
- exception handler ranges bound to labels, not byte offsets
- transparent `WIDE` instruction handling (users should never need to think about wide-form variants)
- preservation of unknown attributes through transformations

This layer is the core missing capability behind the requested "API to manipulate the classfiles."

A key design decision is whether this API is centered on direct mutable dataclasses, builder objects, visitor/transformer patterns (as in ASM), or pass pipelines. This choice will heavily affect both usability and maintainability.

ASM's approach uses a dual API: a low-memory event/visitor model for streaming transformations, and a tree model for in-memory editing. BCEL uses `ClassGen`/`MethodGen` builder objects with `InstructionList` as a mutable linked list. The right choice for `pytecode` should be made early, as it shapes the entire manipulation surface.

### 3a. Descriptor and signature parsing ([#3](https://github.com/smithtrenton/pytecode/issues/3))

Add utilities for parsing and constructing JVM type descriptors and method signatures.

Responsibilities:

- parse method descriptors into structured parameter and return types
- parse field descriptors into structured type representations
- parse generic signatures (Signature attribute)
- compute parameter slot counts (accounting for long/double two-slot types)
- construct descriptor strings from structured type objects
- validate descriptor well-formedness

This is a cross-cutting concern used by the editing model, frame computation, constant-pool management, and validation. Without it, descriptor parsing will be scattered ad hoc throughout the codebase.

### 4. Analysis and control-flow layer ([#9](https://github.com/smithtrenton/pytecode/issues/9))

Add intermediate analysis structures for bytecode reasoning.

Responsibilities:

- control-flow graph construction (basic blocks, edges for branches, exception handlers, and fall-through)
- exception handler range analysis (overlapping handlers, nested ranges, handler entry stack state)
- stack and local variable simulation (type tracking per slot, category-aware — int-like types, long, double, reference)
- type merging at control-flow join points (needed for frame computation)
- data-flow analysis used by frame computation and validation
- method descriptor parsing to establish initial parameter slots in the frame

This layer will make frame calculation and verification much more maintainable.

### 4a. Class hierarchy resolution ([#8](https://github.com/smithtrenton/pytecode/issues/8))

Add a pluggable mechanism for resolving class relationships.

Responsibilities:

- answer "is X a subtype of Y?" queries (needed for type merging in frame computation)
- resolve superclass and interface chains
- support method override detection
- provide a pluggable `ClassResolver` interface so users can supply classpath information

Frame computation requires knowing the common superclass of two reference types at merge points. Without class hierarchy resolution, the library cannot compute frames correctly for code that uses inheritance. ASM leaves this to users; BCEL provides a built-in `Repository`. A pluggable interface is the pragmatic middle ground.

### 5. Validation layer ([#11](https://github.com/smithtrenton/pytecode/issues/11))

Add explicit validation that can run before emission.

Responsibilities:

- verify constant-pool references are valid and well-typed
- verify branch targets, exception ranges, and code lengths
- verify access-flag combinations and version-dependent rules
- verify descriptors, annotations, and attribute constraints
- validate attribute versioning (some attributes are only valid in specific classfile versions — e.g., type annotations require version 52+, modules require version 53+)
- validate `WIDE` prefix usage and operand ranges
- report actionable, structured diagnostics (not just exceptions) with location context (class, method, bytecode offset, constant-pool index)
- support diagnostic collection mode (report all errors, not fail-fast)

### 6. Emission layer ([#12](https://github.com/smithtrenton/pytecode/issues/12))

Add deterministic classfile serialization.

Responsibilities:

- write classfile structures back to bytes
- rebuild or preserve constant-pool layout
- recalculate lengths, offsets, and indexes
- optionally minimize or canonicalize generated structures

### 7. JAR rewrite layer ([#15](https://github.com/smithtrenton/pytecode/issues/15))

Once class emission exists, add archive-level rewrite support.

Responsibilities:

- apply transformations to parsed classes in a JAR
- preserve non-class resources
- preserve JAR metadata (META-INF/MANIFEST.MF, signatures, service loader configs)
- write modified archives safely

This is not strictly required for a classfile library, but it follows naturally from the existing `JarFile` support and would make the project more useful in practice.

### Cross-cutting concern: debug info management ([#13](https://github.com/smithtrenton/pytecode/issues/13))

Mutation invalidates debug attributes (LineNumberTable, LocalVariableTable, LocalVariableTypeTable) because they bind to bytecode offsets. The editing model should either:

- rebind debug info to labels so it survives instruction edits automatically
- explicitly mark debug info as stale after mutation and provide helpers to update or strip it
- provide a "strip debug info" utility for users who do not need it

Without this, mutated classes will produce confusing stack traces and debugger behavior.

## Roadmap aligned to the project goal

The user-provided roadmap is correct, but it benefits from calling out a few
enabling pieces explicitly. Some of these are now implemented foundations, while
others remain future work.

### Already identified

1. Create an API to manipulate classfiles
2. Calculate frames
3. Validate the manipulated classfile and generate new classfiles

### Capabilities that should stay explicit in the roadmap

#### 1. A bytecode/classfile writer ([#4](https://github.com/smithtrenton/pytecode/issues/4), [#12](https://github.com/smithtrenton/pytecode/issues/12))

Generation is mentioned, but it is worth calling out explicitly as a major subsystem. Writing new class files is more than a final `to_bytes()` method; it requires a full serialization pipeline including instruction offset resolution, constant-pool layout, attribute length computation, and `WIDE` instruction insertion when operands exceed single-byte range.

#### 2. Constant-pool management ([#5](https://github.com/smithtrenton/pytecode/issues/5)) — implemented foundation

Any manipulation API needs to create, deduplicate, update, and reindex
constant-pool entries. That foundation is now present via
`ConstantPoolBuilder`, including spec-aware Modified UTF-8 handling, lookup
helpers, MethodHandle validation, and deterministic ordering, so the roadmap
should treat it as an enabling dependency that subsequent editing/emission work
builds on.

#### 3. Symbolic labels and branch management ([#7](https://github.com/smithtrenton/pytecode/issues/7))

Editing bytecode safely requires label-based branch targets instead of manual offset arithmetic. Labels must support forward references (needed for forward branches), must survive instruction insertion and removal, and must handle exception handler range binding. `TABLESWITCH`/`LOOKUPSWITCH` padding recalculation also depends on stable label resolution.

#### 4. Descriptor and signature parsing ([#3](https://github.com/smithtrenton/pytecode/issues/3)) — implemented foundation

A dedicated descriptor parsing utility is needed throughout the library — for
frame computation (establishing initial local slots from method parameters), for
constant-pool management (creating method/field references), for validation
(checking type correctness), and for the editing API (adding methods or
fields). This foundation is now in place and already performs stricter
well-formedness checks, so later roadmap items can rely on it instead of
re-implementing descriptor logic ad hoc.

#### 5. Control-flow and data-flow analysis ([#9](https://github.com/smithtrenton/pytecode/issues/9))

Frame calculation depends on more than raw instruction parsing. A control-flow graph and stack/local simulation layer will likely be necessary for correctness. The simulator must be type-aware (tracking category sizes for long/double, handling null as any reference type) and must understand exception handler entry assumptions (1 value on stack for caught exception type).

#### 6. Class hierarchy resolution ([#8](https://github.com/smithtrenton/pytecode/issues/8))

Type merging at control-flow join points requires knowing the common superclass of two reference types. This in turn requires resolving the class hierarchy, which depends on classpath information the library cannot know on its own. A pluggable `ClassResolver` interface should be provided.

#### 7. Max stack and max locals recomputation ([#10](https://github.com/smithtrenton/pytecode/issues/10))

These are adjacent to frame computation but distinct enough to deserve explicit roadmap status.

#### 8. Version-aware verification rules ([#11](https://github.com/smithtrenton/pytecode/issues/11))

The JVM classfile format changes across versions. Validation and emission should understand feature gating and version constraints (e.g., type annotations require classfile version 52+, modules require 53+, records require 60+, sealed classes require 61+).

#### 9. Round-trip fidelity and compatibility testing ([#14](https://github.com/smithtrenton/pytecode/issues/14))

To be a practical ASM/BCEL alternative, the project should prove:

- parse → emit → parse stability (idempotent round-trips)
- compatibility across representative Java compiler outputs (javac 8, 11, 17, 21)
- verifier acceptance of generated classes (run through `java -verify`)
- preservation of unknown or unsupported attributes where possible
- deterministic emission for reproducible builds

#### 10. Error and diagnostics model ([#11](https://github.com/smithtrenton/pytecode/issues/11))

Manipulation and validation need structured errors, not only parser exceptions. Users will need actionable messages when a transformation creates an invalid class. Errors should carry location context (class name, method, bytecode offset, constant-pool index) and the validation layer should support collecting all diagnostics rather than failing on the first.

#### 11. API shape and extension strategy ([#6](https://github.com/smithtrenton/pytecode/issues/6))

Decide early whether the manipulation API is centered on:

- direct mutable dataclasses
- builder objects (BCEL-style `ClassGen`/`MethodGen`)
- visitor/transformer patterns (ASM-style `ClassVisitor`/`MethodVisitor`)
- pass pipelines

This choice will heavily affect maintainability and usability. A dual approach (tree model for in-memory editing, optional visitor model for streaming) is worth considering.

#### 12. Debug info management ([#13](https://github.com/smithtrenton/pytecode/issues/13))

Mutation invalidates LineNumberTable, LocalVariableTable, and LocalVariableTypeTable entries because they reference bytecode offsets. The library should provide utilities to rebind debug info after transformation, strip it cleanly, or preserve it through label-based indirection.

#### 13. JSR/RET legacy support

The `JSR` and `RET` instructions (used for subroutine inlining in pre-Java 6 classfiles) are present in the opcode table but create complex control-flow for frame computation. The library should decide whether to fully support these legacy instructions or to document them as unsupported for analysis purposes. Modern JVMs do not allow them in classfiles with version ≥ 51.

## Recommended implementation order

1. ~~Fix the known parser bugs.~~ ([#1](https://github.com/smithtrenton/pytecode/issues/1) — done)
2. ~~Add unit tests for each attribute type, instruction operand shape, and constant-pool entry.~~ ([#2](https://github.com/smithtrenton/pytecode/issues/2) — done)
3. ~~Add descriptor and signature parsing utilities.~~ ([#3](https://github.com/smithtrenton/pytecode/issues/3) — done)
4. ~~Introduce a writer foundation for primitive values and classfile sections.~~ ([#4](https://github.com/smithtrenton/pytecode/issues/4) — done)
5. ~~Add constant-pool management utilities (deduplication, symbol lookup, reindexing).~~ ([#5](https://github.com/smithtrenton/pytecode/issues/5) — done)
6. ~~Design the mutable editing model and public transformation API.~~ ([#6](https://github.com/smithtrenton/pytecode/issues/6) — Phase 1 done)
7. Add label-based instruction editing with automatic offset recalculation. ([#7](https://github.com/smithtrenton/pytecode/issues/7))
8. Add a pluggable class hierarchy resolver. ([#8](https://github.com/smithtrenton/pytecode/issues/8))
9. Build control-flow graph construction and stack/local simulation. ([#9](https://github.com/smithtrenton/pytecode/issues/9))
10. Implement max stack, max locals, and stack map frame recomputation. ([#10](https://github.com/smithtrenton/pytecode/issues/10))
11. Implement validation with structured diagnostics and version-aware rules. ([#11](https://github.com/smithtrenton/pytecode/issues/11))
12. Add classfile emission with deterministic constant-pool layout. ([#12](https://github.com/smithtrenton/pytecode/issues/12))
13. Add debug info rebinding utilities. ([#13](https://github.com/smithtrenton/pytecode/issues/13))
14. Add round-trip and verifier-focused regression coverage. ([#14](https://github.com/smithtrenton/pytecode/issues/14))
15. Add optional JAR rewrite support. ([#15](https://github.com/smithtrenton/pytecode/issues/15))

## Recommended quality gates

Before calling the library a manipulation toolkit, it should have:

- ~~unit tests for every attribute type parser and instruction operand shape~~
- round-trip tests for representative classfiles across Java versions
- fixture coverage for modern attributes (records, sealed classes, module metadata)
- compatibility tests across multiple compiler outputs (javac 8, 11, 17, 21)
- ~~negative tests for malformed transformations and invalid classfile inputs~~
- verifier acceptance tests (generated classes pass `java -verify`)
- stable emitted bytes for deterministic scenarios
- structured diagnostic output for all validation failures

## Non-goals for now

To keep the scope focused, the project does not need to become:

- a Java source parser
- a decompiler
- a full JVM runtime
- a bytecode optimizer unless optimization is explicitly desired later

## Summary

`pytecode` has a solid parser-oriented foundation — typed models, complete instruction decoding, attribute parsing, JAR integration — and now a mutable editing model (`ClassModel`/`MethodModel`/`FieldModel`/`CodeModel`) with symbolic references and bidirectional conversion to/from the parsed `ClassFile`.

The test suite has unit-level coverage across all modules (650 tests), including per-attribute-type parsing, all instruction operand shapes, constant-pool edge cases, Modified UTF-8 behavior, descriptor validation, constant-pool builder safety, binary writer primitives, and mutable model round-trip verification across 11 Java fixture classes.

The remaining work is centered on instruction-level editing, analysis,
validation, and emission. The roadmap continues with:

- symbolic branch/label handling and instruction editing ([#7](https://github.com/smithtrenton/pytecode/issues/7))
- classfile writing infrastructure ([#12](https://github.com/smithtrenton/pytecode/issues/12))
- control-flow and data-flow analysis ([#9](https://github.com/smithtrenton/pytecode/issues/9))
- class hierarchy resolution ([#8](https://github.com/smithtrenton/pytecode/issues/8))
- max stack/max locals recomputation ([#10](https://github.com/smithtrenton/pytecode/issues/10))
- version-aware validation ([#11](https://github.com/smithtrenton/pytecode/issues/11))
- structured diagnostics
- debug info management during mutation ([#13](https://github.com/smithtrenton/pytecode/issues/13))
- round-trip and JVM compatibility testing ([#14](https://github.com/smithtrenton/pytecode/issues/14))
- composable transform pipelines (Phase 2 of [#6](https://github.com/smithtrenton/pytecode/issues/6))

Those additions turn the current toolkit into a realistic Python counterpart to libraries like ASM and BCEL.
