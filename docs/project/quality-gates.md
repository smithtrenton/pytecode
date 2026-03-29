# Quality gates, non-goals, and project summary

## Recommended quality gates

Before calling the library a manipulation toolkit, it should have:

- ~~unit tests for every attribute type parser and instruction operand shape~~
- round-trip tests for representative classfiles across Java versions
- fixture coverage for modern attributes (records, sealed classes, module metadata)
- compatibility tests across multiple compiler outputs (javac 8, 11, 17, 21)
- ~~negative tests for malformed transformations and invalid classfile inputs~~
- verifier acceptance tests (generated classes pass `java -verify`)
- stable emitted bytes for deterministic scenarios
- ~~structured diagnostic output for all validation failures~~ ([#11](https://github.com/smithtrenton/pytecode/issues/11) — done)
- ~~differential CFG checks against an external oracle for representative compiled fixtures before later frame-computation and validation work depends on that analysis layer~~ ([#17](https://github.com/smithtrenton/pytecode/issues/17) — done)
- Tier 1 (roundtrip) passing for all existing fixtures before any higher tier is attempted
- Tier 2 (structural verifier) accepting all classes that `javap -v` accepts
- Tier 3 (javac comparison) showing zero "error"-severity diffs for basic fixtures
- Tier 4 (JVM loading) passing with `-Xverify:all` for all roundtrip outputs

## Non-goals for now

To keep the scope focused, the project does not need to become:

- a Java source parser
- a decompiler
- a full JVM runtime
- a bytecode optimizer unless optimization is explicitly desired later

## Summary

`pytecode` has a solid parser-oriented foundation — typed models, complete instruction decoding, attribute parsing, JAR integration — and now a deterministic emission layer (`ClassWriter.write()` plus `ClassModel.to_bytes()`), a mutable editing model (`ClassModel`/`MethodModel`/`FieldModel`/`CodeModel`) with symbolic class references, label-based control-flow editing, lifted exception/debug metadata, bidirectional conversion to/from the parsed `ClassFile`, full symbolic instruction operand wrappers for all major non-control-flow instruction families (`FieldInsn`, `MethodInsn`, `InterfaceMethodInsn`, `TypeInsn`, `VarInsn`, `IIncInsn`, `LdcInsn`, `InvokeDynamicInsn`, `MultiANewArrayInsn`), a pluggable hierarchy-resolution layer in `pytecode.hierarchy`, a control-flow analysis layer in `pytecode.analysis` for CFG construction, verification-type simulation, max-stack/max-local recomputation, and StackMapTable generation, JVM-backed differential CFG validation against ASM, and a structural validation layer in `pytecode.verify` for classfile and ClassModel diagnostics. The editing model follows Design A (Mutable Dataclasses), chosen after evaluating five candidate designs and surveying eight additional JVM bytecode libraries; the phased extension plan adds pass-style composition (Phase 2) and an optional streaming visitor layer (Phase 3) on top of the tree model.

The test suite has unit-level coverage across all modules, including per-attribute-type parsing, all instruction operand shapes, constant-pool edge cases, Modified UTF-8 behavior, descriptor validation, constant-pool builder safety, binary writer primitives, label/layout lowering, symbolic operand wrapper lifting/lowering, hierarchy resolution, control-flow simulation, CFG oracle comparisons, frame recomputation, structural validation diagnostics, byte-for-byte `ClassWriter.write()` roundtrips across the compiled Java fixture corpus, and byte-for-byte `ClassModel.to_bytes()` roundtrips across the same corpus with preserved nested `Code`-attribute ordering.

A four-tier bytecode validation framework now sits on top of the implemented `ClassWriter` ([#12](https://github.com/smithtrenton/pytecode/issues/12)). Tier 1 binary roundtrip fidelity has landed; the remaining tiers still cover JVM spec format and static constraint checking (Tier 2), semantic comparison against javac output including CP ordering and instruction selection analysis (Tier 3), and end-to-end JVM loading and execution testing via a verification harness (Tier 4). The constant pool strategy uses preserve-on-roundtrip as the default mode with deterministic insertion order for v1 from-scratch generation and opt-in javac-compatible ordering reserved for later compatibility work.

The remaining work is centered on broader validation and mutation workflows now that emission, validation, and frame recomputation are in place. The roadmap continues with:

- broader debug info management during mutation ([#13](https://github.com/smithtrenton/pytecode/issues/13))
- broader round-trip and JVM compatibility testing beyond the landed Tier 1 suite ([#14](https://github.com/smithtrenton/pytecode/issues/14))
- composable transform pipelines (Phase 2 of [#6](https://github.com/smithtrenton/pytecode/issues/6))
