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
- Unit test coverage for all attribute types, instruction operand shapes, constant-pool entries, byte utilities, class reader, and JAR handling (256 tests)

### Not implemented yet

## Current architecture

The codebase is currently organized as a parser pipeline with a strongly typed read model.

### Public entry points

- `pytecode.ClassReader`
  - Accepts classfile bytes or a file path
  - Parses eagerly during initialization
  - Produces `class_info`, an `info.ClassFile` dataclass tree
- `pytecode.JarFile`
  - Reads the contents of a JAR
  - Separates `.class` entries from non-class resources
  - Parses classes via `ClassReader`

At the moment, these are the only public exports in `pytecode.__init__`.

### Module responsibilities

#### `pytecode\bytes_utils.py`

Low-level big-endian byte readers and the `BytesReader` base class. This is the I/O foundation for class parsing. Includes `ByteParser` abstract class and concrete parser types (`U1`, `I1`, `U2`, `I2`, `U4`, `I4`, `Bytes`) for composable binary reading, plus standalone helper functions and a stateful `BytesReader` that tracks offset.

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

#### `pytecode\constant_pool.py`

Typed dataclasses for all 17 constant-pool entry types plus the `ConstantPoolInfoType` enum mapping tags to dataclasses. The enum embeds both the numeric tag and the corresponding dataclass, so new constant types only require adding an enum member.

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

Enums and flags representing JVM constants, access flags, verification types, and target-type metadata.

#### `pytecode\jar.py`

JAR container support. This is currently a convenience layer around archive reading plus class parsing.

#### `run.py`

A repository smoke-test script that parses a JAR and writes pretty-printed class output alongside copied resource files. This is primarily a validation utility for current parser behavior.

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
- The public API is read-only in practice
- There is no semantic editing layer or writer layer yet
- Most parser behavior lives in one large module (`class_reader.py`, ~618 lines), which will become harder to evolve once mutation and emission are added
- Parsed objects still use raw constant-pool indexes heavily, which is accurate to the classfile spec but not ideal for ergonomic transformations
- No descriptor or signature parsing utilities exist — code that needs to understand method types must parse descriptor strings ad hoc
- There is an existing `TODO` comment in `class_reader.py:11` noting a desire to rework the reader to use dataclass annotations for byte reading instead of manual parsing

### Test coverage

The test suite provides both integration-level and unit-level coverage (256 tests total):

**Unit tests** ([#2](https://github.com/smithtrenton/pytecode/issues/2) — done):

- `test_attributes.py` (87 tests) — per-attribute-type parsing for all 30 standard attribute types, stack map frame variants, verification types, annotation element values, type annotations, and the `UnimplementedAttr` fallback.
- `test_instructions.py` (62 tests) — instruction decoding for all operand shapes including no-operand, local variable index, constant-pool index, bipush/sipush, branch16/branch32, iinc, invokedynamic, invokeinterface, newarray, multianewarray, lookupswitch, tableswitch, and wide variants.
- `test_constant_pool.py` (26 tests) — all 17 constant-pool entry types, Long/Double double-slot handling, mixed pool parsing, and unknown tag errors.
- `test_class_reader.py` (28 tests) — classfile parsing including magic number validation, version field validation, constant-pool indexing, access flags, interfaces, fields, methods, Code attributes, and error paths for invalid/truncated classfiles.
- `test_bytes_utils.py` (42 tests) — all primitive byte readers (U1/I1/U2/I2/U4/I4/Bytes), `BytesReader` stateful cursor, rewind, and buffer overrun.
- `test_jar.py` (11 tests) — JAR reading, class/non-class separation, path normalization, and compiled JAR class count.

Test fixtures are generated from Java source in `tests/resources/` rather than relying on large binary artifacts.

## Recommended target architecture

To reach the stated project goal, the library should evolve from a parser into a layered classfile toolkit.

### 1. Binary I/O layer ([#4](https://github.com/smithtrenton/pytecode/issues/4))

Keep low-level binary readers and add the inverse writer primitives.

Responsibilities:

- read primitive JVM binary values
- write primitive JVM binary values
- handle alignment and length-prefixed structures
- provide reusable helpers for parser and emitter code

Likely modules:

- `bytes_utils.py` for reading
- future writer helpers such as `byte_writer.py` or a symmetric write API

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

The user-provided roadmap is correct, but it is missing a few enabling pieces.

### Already identified

1. Create an API to manipulate classfiles
2. Calculate frames
3. Validate the manipulated classfile and generate new classfiles

### Missing capabilities that should be added to the roadmap

#### 1. A bytecode/classfile writer ([#4](https://github.com/smithtrenton/pytecode/issues/4), [#12](https://github.com/smithtrenton/pytecode/issues/12))

Generation is mentioned, but it is worth calling out explicitly as a major subsystem. Writing new class files is more than a final `to_bytes()` method; it requires a full serialization pipeline including instruction offset resolution, constant-pool layout, attribute length computation, and `WIDE` instruction insertion when operands exceed single-byte range.

#### 2. Constant-pool management ([#5](https://github.com/smithtrenton/pytecode/issues/5))

Any manipulation API will need to create, deduplicate, update, and reindex constant-pool entries. Without this, even simple edits become fragile. A `ConstantPoolBuilder` or similar utility should support deduplication on insertion, symbol-table-style lookups (class name → index), type-safety constraints (ensuring only valid constant types appear in specific positions), and deterministic ordering for reproducible output.

#### 3. Symbolic labels and branch management ([#7](https://github.com/smithtrenton/pytecode/issues/7))

Editing bytecode safely requires label-based branch targets instead of manual offset arithmetic. Labels must support forward references (needed for forward branches), must survive instruction insertion and removal, and must handle exception handler range binding. `TABLESWITCH`/`LOOKUPSWITCH` padding recalculation also depends on stable label resolution.

#### 4. Descriptor and signature parsing ([#3](https://github.com/smithtrenton/pytecode/issues/3))

A dedicated descriptor parsing utility is needed throughout the library — for frame computation (establishing initial local slots from method parameters), for constant-pool management (creating method/field references), for validation (checking type correctness), and for the editing API (adding methods or fields). This is easy to overlook but becomes a pervasive dependency.

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
3. Add descriptor and signature parsing utilities. ([#3](https://github.com/smithtrenton/pytecode/issues/3))
4. Introduce a writer foundation for primitive values and classfile sections. ([#4](https://github.com/smithtrenton/pytecode/issues/4))
5. Add constant-pool management utilities (deduplication, symbol lookup, reindexing). ([#5](https://github.com/smithtrenton/pytecode/issues/5))
6. Design the mutable editing model and public transformation API. ([#6](https://github.com/smithtrenton/pytecode/issues/6))
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

`pytecode` already has a solid parser-oriented foundation: typed models, complete instruction decoding, attribute parsing, and JAR integration.

The test suite now has unit-level coverage across all modules (256 tests), including per-attribute-type parsing, all instruction operand shapes, constant-pool edge cases, and error paths.

The missing work is not only "manipulate, calculate frames, validate, and emit." To fully meet the project's objective, the roadmap should also explicitly include:

- classfile writing infrastructure
- constant-pool management
- descriptor and signature parsing
- symbolic branch/label handling
- control-flow and data-flow analysis
- class hierarchy resolution
- max stack/max locals recomputation
- version-aware validation
- structured diagnostics
- debug info management during mutation
- round-trip and JVM compatibility testing
- an explicit API design decision (tree vs visitor vs builder)

Those additions turn the current parser into a realistic Python counterpart to libraries like ASM and BCEL.
