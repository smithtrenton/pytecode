# Recommended target architecture

This document captures the intended layered architecture for pytecode. Most of the layers described here are now implemented; treat this as a reference model for future changes rather than as a list of missing features.

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

## 3. Mutable editing model ([#6](https://github.com/smithtrenton/pytecode/issues/6) — Phases 1-2 landed)

Phase 1 of this layer is now implemented via `ClassModel`, `MethodModel`, `FieldModel`, and `CodeModel`, and the Phase 2 extension now lives in `pytecode.transforms`. Together, these give pytecode a higher-level object model plus a lightweight composition layer for safe manipulation. Users no longer need to hand-edit raw constant-pool indexes and branch offsets for the major editing workflows already covered by labels, symbolic operands, `ConstantPoolBuilder`, callable transform pipelines, and the richer matcher DSL.

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
- composable class/method/field/code transforms with deterministic pass ordering, owner-filtered lifting, and composable selection predicates

This layer is now the core user-facing manipulation surface behind the requested "API to manipulate the classfiles." `pytecode.transforms` provides the landed Phase 2 composition layer (`Pipeline`, `pipeline()`, `Matcher`, `on_*` lifting helpers, owner filters, and the current selector/lightweight-helper surface) with context-passing transform protocols from the Phase 3 evaluation ([#21](https://github.com/smithtrenton/pytecode/issues/21)). The `FieldTransform`, `MethodTransform`, and `CodeTransform` protocols pass owning context (the parent `ClassModel` and, for code transforms, the parent `MethodModel`) so transforms can inspect where they are in the class hierarchy without needing a separate visitor API.

After evaluating five candidate designs — **(A)** direct mutable dataclasses, **(B)** builder objects (BCEL-style), **(C)** visitor/transformer pattern (ASM-style), **(D)** pass pipelines, and **(E)** dual tree+visitor — **Design A (Mutable Dataclasses)** was chosen as the primary editing API. The tree model is designed so that pass-style composition (Design D) can be layered on top, and a visitor layer (Design E) can be added later if streaming becomes necessary. See [editing model design rationale](../design/editing-model.md) for the full analysis, comparative feature matrix, library survey, and phased implementation plan.

### 3a. Descriptor and signature parsing ([#3](https://github.com/smithtrenton/pytecode/issues/3))

Descriptor and signature utilities are now implemented in `pytecode.classfile.descriptors`.

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

Max stack/max locals recomputation and StackMapTable generation are now implemented on this layer ([#10](https://github.com/smithtrenton/pytecode/issues/10) — done).

### 4a. Class hierarchy resolution ([#8](https://github.com/smithtrenton/pytecode/issues/8) — done)

`pytecode.analysis.hierarchy` now provides a pluggable mechanism for resolving class relationships. The implemented layer covers:

- subtype checks (`is_subtype()`)
- superclass/interface traversal (`iter_superclasses()`, `iter_supertypes()`)
- common-superclass lookup (`common_superclass()`)
- method-override detection (`find_overridden_methods()`)
- a minimal `ClassResolver` protocol plus the in-memory `MappingClassResolver`

Max stack/max locals recomputation and StackMapTable generation are now complete ([#10](https://github.com/smithtrenton/pytecode/issues/10) — done), built on the analysis layer.

## 5. Validation layer ([#11](https://github.com/smithtrenton/pytecode/issues/11) — core layer landed; four tiers implemented in [#14](https://github.com/smithtrenton/pytecode/issues/14))

Explicit validation is now implemented in `pytecode.analysis.verify`, and the broader architecture is now exercised as a four-tier framework in the test suite. Tier 1 roundtrip coverage lives in `tests/test_class_writer.py`; `tests/test_validation.py` covers the fixture/release matrix for Tiers 1, 2, and 4; `tests/javap_parser.py` plus `tests/test_javap_parser.py` cover the Tier 3 semantic-diff engine; and `tests/jvm_harness.py` provides the JVM harness used by Tier 4. Each tier catches a different class of bugs, is independently testable, and builds on the one below:

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

## 6. Emission layer ([#12](https://github.com/smithtrenton/pytecode/issues/12) — done)

Deterministic classfile serialization is now implemented via `ClassWriter.write()` and `ClassModel.to_bytes()`.

Responsibilities:

- write classfile structures back to bytes
- rebuild or preserve constant-pool layout
- recalculate lengths, offsets, and indexes
- optionally minimize or canonicalize generated structures

## 7. JAR rewrite layer ([#15](https://github.com/smithtrenton/pytecode/issues/15) — done)

This layer is now implemented in `pytecode.archive`.

Responsibilities:

- apply transformations to parsed classes in a JAR
- preserve non-class resources
- preserve JAR metadata (META-INF/MANIFEST.MF, signatures, service loader configs)
- write modified archives safely

`JarFile.add_file()` and `JarFile.remove_file()` mutate the in-memory archive state, while `JarFile.rewrite()` serializes that state back to disk, optionally lifting `.class` entries through `ClassModel` for in-place transforms and class-level lowering controls. Existing signature-related files are preserved as pass-through resources; rewritten signed JARs are not re-signed automatically and may therefore no longer verify as signed.

## Cross-cutting concern: debug info management ([#13](https://github.com/smithtrenton/pytecode/issues/13), [#18](https://github.com/smithtrenton/pytecode/issues/18) — done)

Mutation can leave debug metadata semantically stale even when label rebinding keeps offset-bound tables structurally valid. The current editing model now covers four explicit pieces of behavior:

- rebind lifted code-debug metadata to labels so ordinary instruction edits keep offset-based tables aligned automatically
- provide explicit preserve/strip helpers so callers can omit `LineNumberTable`, `LocalVariableTable`, `LocalVariableTypeTable`, `SourceFile`, and `SourceDebugExtension` metadata deliberately during lowering
- model known-stale debug metadata explicitly on `CodeModel` and `ClassModel` via `DebugInfoState`, with helper functions in `pytecode.edit.debug_info`; explicitly stale class/code debug metadata is stripped automatically during lowering, and `verify_classmodel()` warns before emission
- provide `skip_debug=True` lift controls on `ClassModel.from_classfile()`, `ClassModel.from_bytes()`, and `JarFile.rewrite()` for an ASM-like path that omits `SourceFile`, `SourceDebugExtension`, `LineNumberTable`, `LocalVariableTable`, `LocalVariableTypeTable`, and `MethodParameters` before the mutable model is materialized

Staleness is defined semantically rather than by raw bytecode-offset movement alone. Offsets can move safely under label rebinding; metadata becomes stale when callers mutate source mapping or local-variable meaning/scope/signature/slot usage without updating the corresponding debug metadata.
