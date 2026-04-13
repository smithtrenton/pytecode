# Recommended target architecture

This document captures the intended layered architecture for pytecode. Most of the layers described here are now implemented; treat this as a reference model for future changes rather than as a list of missing features.

## 1. Binary I/O layer ([#4](https://github.com/smithtrenton/pytecode/issues/4))

The low-level read/write primitives now live in the Rust engine and its archive
support crates. Python callers use the typed surfaces in `pytecode.classfile`,
`pytecode.model`, and `pytecode.archive` rather than a standalone Python byte
reader/writer module.

Responsibilities:

- read primitive JVM binary values
- write primitive JVM binary values
- handle alignment and length-prefixed structures
- provide reusable helpers for parser and emitter code

Implementation:

- `pytecode-engine` for classfile parsing, lowering, and emission
- `pytecode-archive` for archive rewrite and ZIP metadata handling

## 2. Parsed spec model layer

Retain a spec-faithful raw classfile layer for exact representation of on-disk
structures.

Responsibilities:

- mirror the classfile format accurately
- preserve raw indexes and attribute layout
- serve as the lossless interchange model between parser and writer

Today this layer is exposed through the typed Rust-backed objects in
`pytecode.classfile`.

## 3. Mutable editing model ([#6](https://github.com/smithtrenton/pytecode/issues/6) — now Rust-owned)

The mutable editing model is now fully Rust-owned via `pytecode.ClassModel`. The Rust engine handles symbolic references, label resolution, bytecode lowering, constant-pool management, and branch widening natively. Python exposes this through thin PyO3 bindings.

The Rust-first package surface uses `pytecode.ClassModel`, `pytecode.ClassReader`, and `pytecode.ClassWriter` for parse/edit/write flows, plus `pytecode.transforms` (`PipelineBuilder`, matcher factories, and Rust-backed transform factories) for production transform execution.

### 3a. Descriptor and signature parsing ([#3](https://github.com/smithtrenton/pytecode/issues/3))

Descriptor and signature parsing is now handled by the Rust engine (`pytecode-engine`). The Python `pytecode.classfile.descriptors` module has been removed.

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

## 5. Validation layer ([#11](https://github.com/smithtrenton/pytecode/issues/11) — core layer landed)

Explicit validation is now implemented in `pytecode.analysis.verify`. For
future changes, the validation architecture is still best understood as a
layered model: the repository actively covers Tier 1 and Tier 2 behavior in the
Rust workspace and Python API suites, keeps Tier 3 `javap` tooling in
`tests/javap_parser.py`, and treats JVM-backed differential or oracle checks as
optional extensions rather than part of the default suite. Each tier catches a
different class of bugs and builds on the one below:

| Tier | What it catches | Speed | External deps |
|------|-----------------|-------|---------------|
| 1 — Binary Roundtrip | Serialization bugs, offset miscalculation, endianness | Fast | None |
| 2 — Structural Verification | Spec violations, illegal attributes, invalid CP refs | Medium (subprocess) | javap; optionally AsmTools |
| 3 — Semantic Comparison | Non-idiomatic output, CP ordering drift, wide-instruction overuse | Slow (subprocess) | javac + javap |
| 4 — JVM Loading & Execution | StackMapTable errors, verifier failures, type-safety | Slowest (JVM launch) | java + optional test harness |

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
- optionally prove JVM verifier acceptance of generated classes when a dedicated harness is warranted

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
- provide explicit preserve/strip helpers so callers can omit `LineNumberTable`, `LocalVariableTable`, `LocalVariableTypeTable`, `SourceFile`, and `SourceDebugExtension` metadata deliberately during lowering or archive rewrite
- surface debug-policy choices through `DebugInfoPolicy` and `verify_classmodel()` diagnostics rather than a separate Python-side debug-info module
- keep `JarFile.rewrite(skip_debug=True)` only as a compatibility alias; new code should prefer `debug_info=DebugInfoPolicy.STRIP`

Staleness is defined semantically rather than by raw bytecode-offset movement alone. Offsets can move safely under label rebinding; metadata becomes stale when callers mutate source mapping or local-variable meaning/scope/signature/slot usage without updating the corresponding debug metadata.
