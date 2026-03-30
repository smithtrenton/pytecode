# pytecode documentation

## Purpose

`pytecode` is a Python library for parsing, inspecting, manipulating, and emitting JVM class files and bytecode.

The project goal is to provide a Python alternative to Java libraries such as ASM and BCEL. Today, the library covers parsing, deterministic classfile emission, a mutable symbolic editing model, hierarchy resolution, control-flow analysis, multi-release four-tier validation, optional JAR rewrite support, and explicit debug-info stale-state / skip-debug controls. The remaining roadmap work is higher-level transform composition and generated API reference coverage.

## Current status

### Implemented today

- Reading classfile bytes into an in-memory object model
- Parsing all 17 constant-pool entry types
- Parsing fields, methods, interfaces, and class-level metadata
- Parsing `Code` attributes and decoding the full supported JVM opcode set, including WIDE-expanded forms, into typed instruction records
- Parsing the standard attribute families including annotations, stack map tables, module metadata, records, and permitted subclasses
- Preserving unknown or unrecognized attributes as raw bytes through `UnimplementedAttr`
- Reading JAR files and parsing every `.class` entry in them
- Rewriting JAR files via `JarFile.add_file()`, `JarFile.remove_file()`, and `JarFile.rewrite()`, including safe temp-file replacement, entry-order preservation, and pass-through preservation of non-class resources plus signature artifacts
- Descriptor and signature parsing utilities: structured field/method descriptor types, generic signature parsing (class, method, and field signatures), round-trip construction, slot-size helpers, and stricter validation of malformed internal names and signature segments — see `pytecode/descriptors.py`
- Binary writer foundation: big-endian write primitives, stateful `BytesWriter` with alignment, reserve/patch helpers for length-prefixed structures — see `pytecode/bytes_utils.py`
- Shared JVM Modified UTF-8 codec for `CONSTANT_Utf8` values — see `pytecode/modified_utf8.py`
- Deterministic classfile serialization via `pytecode.class_writer`: `ClassWriter.write()` emits parsed or lowered `ClassFile` trees back to `.class` bytes, and `ClassModel.to_bytes()` provides a thin lowering-plus-emission convenience path
- Structural classfile validation with structured diagnostics: `pytecode/verify.py` validates magic number, version, constant-pool well-formedness, access flag mutual exclusions, class structure, field/method constraints, Code attribute (branches, exception handlers, CP refs), attribute versioning, descriptor validity, and ClassModel label validity — see `pytecode/verify.py`
- Unit test coverage for all attribute types, instruction operand shapes, constant-pool entries, byte utilities, class reader, JAR handling, descriptor/signature parsing, Modified UTF-8 handling, constant-pool builder hardening, mutable editing model, hierarchy resolution, control-flow/stack simulation, frame recomputation, and validation
- Constant-pool management: `ConstantPoolBuilder` with Modified UTF-8 handling, deduplication, symbol-table lookups, compound-entry auto-creation, MethodHandle/import validation, double-slot handling, defensive-copy reads/exports, and deterministic ordering — see `pytecode/constant_pool_builder.py`
- Mutable editing model: `ClassModel`, `MethodModel`, `FieldModel`, `CodeModel` — mutable dataclasses with symbolic class/field/method references, bidirectional conversion to/from the parsed `ClassFile` model, `ConstantPoolBuilder` integration for raw operand/index passthrough, and label-aware code editing surfaces — see `pytecode/model.py`
- Label-based instruction editing ([#7](https://github.com/smithtrenton/pytecode/issues/7)): `pytecode/labels.py` introduces `Label`, symbolic branch/switch wrappers, lifted exception/debug metadata, automatic offset recalculation, switch padding recomputation, and wide-branch promotion during lowering
- Symbolic instruction operand wrappers ([#16](https://github.com/smithtrenton/pytecode/issues/16)): `pytecode/operands.py` introduces nine editing-model wrappers (`FieldInsn`, `MethodInsn`, `InterfaceMethodInsn`, `TypeInsn`, `VarInsn`, `IIncInsn`, `LdcInsn`, `InvokeDynamicInsn`, `MultiANewArrayInsn`) that replace raw constant-pool indexes and local-variable slot encodings. All wrapper types lift automatically in `model.py` during `from_classfile()` and lower back to raw instructions in `labels.py` during `to_classfile()`.
- Class hierarchy resolution ([#8](https://github.com/smithtrenton/pytecode/issues/8)): `pytecode/hierarchy.py` introduces a pluggable `ClassResolver` protocol, in-memory `MappingClassResolver`, typed resolved hierarchy snapshots (`ResolvedClass`, `ResolvedMethod`, `InheritedMethod`), and helper queries for superclass walks, supertype traversal, subtype checks, common-superclass lookup, and method-override detection.
- Control-flow analysis ([#9](https://github.com/smithtrenton/pytecode/issues/9)): `pytecode/analysis.py` introduces control-flow graph construction, verification-type-based stack/local simulation, structured merge/locals diagnostics, and optional `ClassResolver`-driven reference merging used by the current frame-recomputation and validation layers.
- CFG differential validation ([#17](https://github.com/smithtrenton/pytecode/issues/17)): the test suite now compares `pytecode.analysis.build_cfg()` against a JVM-side ASM oracle across compiled fixture corpora, normalizing instruction-level edges into the same block-level spans, successor sets, and handler sets used by `pytecode`.
- Max stack/max locals recomputation and StackMapTable generation ([#10](https://github.com/smithtrenton/pytecode/issues/10)): `pytecode/analysis.py` now exposes `compute_maxs()` and `compute_frames()` for opt-in recomputation of `max_stack`, `max_locals`, and `StackMapTable` entries. `lower_code()` and `ClassModel.to_classfile()` support `recompute_frames=True` for end-to-end integration. All seven compact StackMapTable frame encodings are supported.
- Debug-info lifecycle controls ([#13](https://github.com/smithtrenton/pytecode/issues/13), [#18](https://github.com/smithtrenton/pytecode/issues/18)): label-based preservation remains the default path for lifted `LineNumberTable`, `LocalVariableTable`, and `LocalVariableTypeTable` metadata, while `pytecode.debug_info` now provides explicit preserve/strip helpers, `DebugInfoState` stale markers, `mark_class_debug_info_stale()` / `mark_code_debug_info_stale()` helpers, and automatic strip-on-lowering behavior for explicitly stale class/code debug metadata. `verify_classmodel()` warns when known-stale debug metadata is still present on the mutable model, and `ClassModel.from_classfile()` / `ClassModel.from_bytes()` plus `JarFile.rewrite()` accept `skip_debug=True` for an ASM-like lift path that omits `SourceFile`, `SourceDebugExtension`, `LineNumberTable`, `LocalVariableTable`, `LocalVariableTypeTable`, and `MethodParameters` before model materialization.
- Tier 1 roundtrip coverage for emission: `tests/test_class_writer.py` now exercises byte-for-byte `ClassWriter.write()` roundtrips over compiled Java fixtures and `ClassModel.to_bytes()` roundtrips over the same corpus, plus raw edge cases such as unknown attributes and double-slot constant-pool gaps
- Validation-framework coverage ([#14](https://github.com/smithtrenton/pytecode/issues/14)): `tests/test_validation.py` parametrizes the compiled Java fixture corpus across `--release 8, 11, 17, 21, 25`, filtered by each fixture's minimum supported release, and runs byte-for-byte roundtrip (T1), `verify_classfile()` + `javap` structural checks (T2), and JVM loading via `VerifierHarness.java` with `-Xverify:all` (T4). The Tier 3 CP-aware semantic-diff engine lives in `tests/javap_parser.py` and is covered by `tests/test_javap_parser.py`.

### Not implemented yet

- Pass-style transformation helpers and composable pipelines on top of the current mutable model foundation ([#6](https://github.com/smithtrenton/pytecode/issues/6))
- Generated API reference docs with full coverage of the supported public surface ([#19](https://github.com/smithtrenton/pytecode/issues/19))

## Documentation guide

| Topic | Location |
|-------|----------|
| Current architecture: runtime, entry points, modules, data flow, design characteristics, test coverage | [architecture/current-architecture.md](architecture/current-architecture.md) |
| Recommended target architecture: layered design, cross-cutting concerns | [architecture/target-architecture.md](architecture/target-architecture.md) |
| Editing model design rationale: candidate designs, comparison matrix, library survey | [design/editing-model.md](design/editing-model.md) |
| Bytecode validation framework: 4-tier validation, tools, CP ordering, test infrastructure | [design/validation-framework.md](design/validation-framework.md) |
| CFG validation research: ASM/BCEL/Soot comparison and recommended differential oracle design | [design/cfg-validation-research.md](design/cfg-validation-research.md) |
| Project roadmap and implementation order | [project/roadmap.md](project/roadmap.md) |
| Quality gates, non-goals, and project summary | [project/quality-gates.md](project/quality-gates.md) |
