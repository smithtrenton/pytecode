# Recommended target architecture

To reach the stated project goal, the library should evolve from a parser into a layered classfile toolkit.

## 1. Binary I/O layer ([#4](https://github.com/smithtrenton/pytecode/issues/4))

Both read and write primitives now live in `bytes_utils.py`.

Responsibilities:

- read primitive JVM binary values
- write primitive JVM binary values
- handle alignment and length-prefixed structures
- provide reusable helpers for parser and emitter code

Module:

- `bytes_utils.py` — read side (`BytesReader`) and write side (`BytesWriter`)

## 2. Parsed spec model layer

Retain the existing spec-faithful dataclasses for exact representation of on-disk structures.

Responsibilities:

- mirror the classfile format accurately
- preserve raw indexes and attribute layout
- serve as the lossless interchange model between parser and writer

This is the layer you already have today.

## 3. Mutable editing model ([#6](https://github.com/smithtrenton/pytecode/issues/6))

Add a higher-level object model for safe manipulation. This should not force users to hand-edit raw constant-pool indexes and branch offsets.

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

After evaluating five candidate designs — **(A)** direct mutable dataclasses, **(B)** builder objects (BCEL-style), **(C)** visitor/transformer pattern (ASM-style), **(D)** pass pipelines, and **(E)** dual tree+visitor — **Design A (Mutable Dataclasses)** was chosen as the primary editing API. The tree model is designed so that pass-style composition (Design D) can be layered on top, and a visitor layer (Design E) can be added later if streaming becomes necessary. See [editing model design rationale](../design/editing-model.md) for the full analysis, comparative feature matrix, library survey, and phased implementation plan.

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

## 4. Analysis and control-flow layer ([#9](https://github.com/smithtrenton/pytecode/issues/9) — done)

`pytecode.analysis` now provides control-flow graph construction, stack/local simulation with verification types, and forward dataflow analysis over the editing model. The implemented layer covers:

- verification type system mirroring JVM spec §4.10.1.2 (VType union with 9 types, merge rules)
- opcode metadata table with stack effects and control-flow properties for all ~205 opcodes
- basic block partitioning and CFG construction with branch, exception, switch, and fall-through edges
- category-2-aware frame state with push/pop/set_local/get_local operations
- worklist-based forward dataflow simulation computing per-block entry/exit states, max_stack, and max_locals
- optional `ClassResolver` integration for reference-type merging at join points
- structured error types for stack underflow, invalid locals, and type-merge failures

Max stack/max locals recomputation and StackMapTable generation depend on this layer and are deferred to [#10](https://github.com/smithtrenton/pytecode/issues/10).

### 4a. Class hierarchy resolution ([#8](https://github.com/smithtrenton/pytecode/issues/8) — done)

`pytecode.hierarchy` now provides a pluggable mechanism for resolving class relationships. The implemented layer covers:

- subtype checks (`is_subtype()`)
- superclass/interface traversal (`iter_superclasses()`, `iter_supertypes()`)
- common-superclass lookup (`common_superclass()`)
- method-override detection (`find_overridden_methods()`)
- a minimal `ClassResolver` protocol plus the in-memory `MappingClassResolver`

Frame computation still depends on max_stack/max_locals recomputation and StackMapTable generation ([#10](https://github.com/smithtrenton/pytecode/issues/10)), which build on the now-complete analysis layer.

## 5. Validation layer ([#11](https://github.com/smithtrenton/pytecode/issues/11))

Add explicit validation that can run before emission, organized as a four-tier framework. Each tier catches a different class of bugs, is independently testable, and builds on the one below:

| Tier | What it catches | Speed | External deps |
|------|-----------------|-------|---------------|
| 1 — Binary Roundtrip | Serialization bugs, offset miscalculation, endianness | Fast (pure Python) | None |
| 2 — Structural Verification | Spec violations, illegal attributes, invalid CP refs | Medium (subprocess) | javap; optionally AsmTools |
| 3 — Semantic Comparison | Non-idiomatic output, CP ordering drift, wide-instruction overuse | Slow (subprocess) | javac + javap |
| 4 — JVM Loading & Execution | StackMapTable errors, verifier failures, type-safety | Slowest (JVM launch) | java + test harness |

Responsibilities across all tiers:

- verify constant-pool references are valid and well-typed
- verify branch targets, exception ranges, and code lengths
- verify access-flag combinations and version-dependent rules
- verify descriptors, annotations, and attribute constraints
- validate attribute versioning (some attributes are only valid in specific classfile versions — e.g., type annotations require version 52+, modules require version 53+)
- validate `WIDE` prefix usage and operand ranges
- report actionable, structured diagnostics (not just exceptions) with location context (class, method, bytecode offset, constant-pool index)
- support diagnostic collection mode (report all errors, not fail-fast)
- compare emitted output against javac-produced class files for instruction selection and CP ordering fidelity
- prove JVM verifier acceptance of generated classes via `defineClass()` + `-Xverify:all`

The detailed design for each tier is in the [bytecode validation framework](../design/validation-framework.md).

## 6. Emission layer ([#12](https://github.com/smithtrenton/pytecode/issues/12))

Add deterministic classfile serialization.

Responsibilities:

- write classfile structures back to bytes
- rebuild or preserve constant-pool layout
- recalculate lengths, offsets, and indexes
- optionally minimize or canonicalize generated structures

## 7. JAR rewrite layer ([#15](https://github.com/smithtrenton/pytecode/issues/15))

Once class emission exists, add archive-level rewrite support.

Responsibilities:

- apply transformations to parsed classes in a JAR
- preserve non-class resources
- preserve JAR metadata (META-INF/MANIFEST.MF, signatures, service loader configs)
- write modified archives safely

This is not strictly required for a classfile library, but it follows naturally from the existing `JarFile` support and would make the project more useful in practice.

## Cross-cutting concern: debug info management ([#13](https://github.com/smithtrenton/pytecode/issues/13))

Mutation invalidates debug attributes (LineNumberTable, LocalVariableTable, LocalVariableTypeTable) because they bind to bytecode offsets. The editing model should either:

- rebind debug info to labels so it survives instruction edits automatically
- explicitly mark debug info as stale after mutation and provide helpers to update or strip it
- provide a "strip debug info" utility for users who do not need it

Without this, mutated classes will produce confusing stack traces and debugger behavior.
