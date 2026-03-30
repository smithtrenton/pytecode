# Quality gates, non-goals, and project summary

## Recommended quality gates

Before calling the library a manipulation toolkit, it should have:

- ~~unit tests for every attribute type parser and instruction operand shape~~
- ~~round-trip tests for representative classfiles across Java versions~~ ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)
- ~~fixture coverage for modern attributes (records, sealed classes, module metadata)~~ ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)
- ~~compatibility tests across multiple compiler outputs (javac 8, 11, 17, 21, 25)~~ ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)
- ~~negative tests for malformed transformations and invalid classfile inputs~~
- ~~verifier acceptance tests (generated classes pass `java -verify`)~~ ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)
- ~~stable emitted bytes for deterministic scenarios~~ ([#12](https://github.com/smithtrenton/pytecode/issues/12), [#14](https://github.com/smithtrenton/pytecode/issues/14) — done)
- generated API reference docs covering the supported public surface, with no undocumented public entry points ([#19](https://github.com/smithtrenton/pytecode/issues/19))
- ~~structured diagnostic output for all validation failures~~ ([#11](https://github.com/smithtrenton/pytecode/issues/11) — done)
- ~~differential CFG checks against an external oracle for representative compiled fixtures before later frame-computation and validation work depends on that analysis layer~~ ([#17](https://github.com/smithtrenton/pytecode/issues/17) — done)
- ~~Tier 1 (roundtrip) passing for all existing fixtures before any higher tier is attempted~~ ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)
- ~~Tier 2 (structural verifier) accepting all classes that `javap -v` accepts~~ ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)
- ~~Tier 3 (javac comparison) showing zero "error"-severity diffs for basic fixtures~~ ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)
- ~~Tier 4 (JVM loading) passing with `-Xverify:all` for all roundtrip outputs~~ ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)

## Non-goals for now

To keep the scope focused, the project does not need to become:

- a Java source parser
- a decompiler
- a full JVM runtime
- a bytecode optimizer unless optimization is explicitly desired later

## Summary

`pytecode` has a solid parser-oriented foundation — typed models, complete instruction decoding, attribute parsing, JAR integration — and now a deterministic emission layer (`ClassWriter.write()` plus `ClassModel.to_bytes()`), a mutable editing model (`ClassModel`/`MethodModel`/`FieldModel`/`CodeModel`) with symbolic class references, label-based control-flow editing, lifted exception/debug metadata, bidirectional conversion to/from the parsed `ClassFile`, full symbolic instruction operand wrappers for all major non-control-flow instruction families (`FieldInsn`, `MethodInsn`, `InterfaceMethodInsn`, `TypeInsn`, `VarInsn`, `IIncInsn`, `LdcInsn`, `InvokeDynamicInsn`, `MultiANewArrayInsn`), a transform-composition layer in `pytecode.transforms` for callable pipelines, composable `Matcher` predicates, owner-filtered lifting helpers, regex selectors plus lightweight structural helpers, and access-flag convenience matchers, a pluggable hierarchy-resolution layer in `pytecode.hierarchy`, a control-flow analysis layer in `pytecode.analysis` for CFG construction, verification-type simulation, max-stack/max-local recomputation, and StackMapTable generation, JVM-backed differential CFG validation against ASM, a structural validation layer in `pytecode.verify` for classfile and ClassModel diagnostics, archive rewrite support in `pytecode.jar` for safe JAR mutation and serialization, and explicit debug-info stale-state / skip-debug controls. The editing model follows Design A (Mutable Dataclasses), chosen after evaluating five candidate designs and surveying eight additional JVM bytecode libraries; the phased extension plan now has a landed Phase 2 composition layer with richer matcher support, while only the optional visitor layer remains deferred follow-up work.

The test suite has unit-level coverage across all modules, including per-attribute-type parsing, all instruction operand shapes, constant-pool edge cases, Modified UTF-8 behavior, descriptor validation, constant-pool builder safety, binary writer primitives, label/layout lowering, symbolic operand wrapper lifting/lowering, hierarchy resolution, control-flow simulation, CFG oracle comparisons, frame recomputation, structural validation diagnostics, byte-for-byte `ClassWriter.write()` roundtrips across the compiled Java fixture corpus, byte-for-byte `ClassModel.to_bytes()` roundtrips across the same corpus with preserved nested `Code`-attribute ordering, a multi-release validation matrix across the compiled Java fixture corpus at `--release 8, 11, 17, 21, 25` for Tier 1 roundtrip, Tier 2 structural checks, and Tier 4 JVM verification, and Tier 3 semantic-diff engine coverage in `tests/javap_parser.py` / `tests/test_javap_parser.py`.

All four tiers of the bytecode validation framework are now implemented ([#14](https://github.com/smithtrenton/pytecode/issues/14)). `tests/test_validation.py` exercises the fixture/release matrix for Tier 1 byte-for-byte roundtrip, Tier 2 structural verification via `verify_classfile()` + `javap`, and Tier 4 JVM loading with `-Xverify:all`; the Tier 3 CP-aware semantic-diff engine lives in `tests/javap_parser.py` and is covered by `tests/test_javap_parser.py`. The constant pool strategy uses preserve-on-roundtrip as the default mode with deterministic insertion order for current from-scratch generation; an opt-in javac-compatible ordering mode remains outside the current roadmap scope.

The remaining work is now centered on documentation coverage and the optional visitor follow-up now that emission, archive rewrite, validation, frame recomputation, explicit debug-info lifecycle controls, and the richer transform-composition layer are in place. The roadmap continues with:

- generated API reference docs with full public-surface coverage ([#19](https://github.com/smithtrenton/pytecode/issues/19))
- an optional visitor-style streaming transform API ([#21](https://github.com/smithtrenton/pytecode/issues/21))
