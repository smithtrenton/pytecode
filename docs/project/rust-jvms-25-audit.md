# Rust JVMS 25 audit and remediation status

## Scope

This audit checks the Rust workspace against the JVM Specification, Java SE 25 Edition, with the focus on the parts this library is expected to implement:

- class file structure and version handling (JVMS chapter 4)
- constant-pool tags, code attributes, and standard predefined attributes
- bytecode parsing/emission and symbolic editing support
- structural validation and frame recomputation behavior

It does **not** treat full JVM runtime execution semantics as required scope for `pytecode`; the question here is whether the Rust library can parse, preserve, validate, edit, and re-emit spec-valid class files through Java 25.

## Current status

The raw Rust classfile layer is complete for the on-disk class file format features this project scopes to support:

- `pytecode-engine::raw` covers all standard constant-pool tags through `CONSTANT_Package_info`.
- `reader.rs` and `writer.rs` cover all predefined JVMS 25 attributes listed in JVMS 4.7, while still preserving unknown attributes.
- `raw::instructions` recognizes the full valid classfile opcode space, including `invokedynamic`, `multianewarray`, `tableswitch`, `lookupswitch`, `goto_w`, `jsr_w`, and `wide`.
- `raw_roundtrip.rs` roundtrips the checked-in fixture matrix through `--release 25`, including records, sealed hierarchies, modules, lambdas/bootstrap methods, type annotations, nests, and Java 25 classfile version 69 fixtures.
- shared Java SE 25 version rules now reject unsupported future majors and historical preview minors while accepting `69.65535`.
- `analysis::verify_classfile` now checks constant-pool linkage, chapter-4 structure/access rules, owner-aware attribute placement/multiplicity/version rules, bootstrap-linked constant relationships, and generic-signature syntax.
- legacy subroutine bytecode (`jsr`, `jsr_w`, `ret`) now flows through symbolic lift/lower and CFG/frame recomputation, with recomputed lowering preserving valid old-version behavior instead of rejecting those methods.
- `pytecode_engine::signatures` now parses and validates class, method, field, reference, and local-variable type signatures.

That means the Rust implementation now has strong **parse/write, validation, and symbolic-analysis coverage** for the scoped JVMS 25 classfile work.

## Remediation status

| Area | Status | What landed |
| --- | --- | --- |
| Classfile version validation | **Implemented** | Shared Java SE 25 rules now back both parse-time validation and verifier diagnostics, including acceptance of current preview `69.65535` and rejection of unsupported future majors or historical preview minors. |
| JVMS 4.8 format checking | **Implemented for scoped checks** | `verify_classfile` now validates constant-pool linkage and descriptor kinds, class/module structural invariants, field/method/class access-flag rules, `Code` placement/count rules, and owner-aware attribute placement/multiplicity/version gates. |
| Bootstrap / condy / invokedynamic cross-checks | **Implemented** | The verifier now checks `MethodHandle.reference_kind`, reference target kinds, bootstrap method index bounds, bootstrap method/argument entry kinds, and descriptor shape for `Dynamic` and `InvokeDynamic`. |
| Attribute semantics beyond parsing | **Implemented for key predefined attributes** | Module, record, nest, code, method-parameter, stack-map, and related predefined attributes now have semantic validation for placement, uniqueness, and minimum-version rules. |
| Legacy subroutine bytecode in analysis/edit pipeline | **Implemented** | CFG and frame recomputation now model `returnAddress`, `jsr`/`jsr_w` continuations, and `ret` dispatch. Symbolic lift/lower preserves those instructions, and recomputed lowering avoids producing invalid `StackMapTable` entries on classfile versions that cannot encode them. |
| Signature grammar support in Rust | **Implemented** | `pytecode_engine::signatures` now parses and validates generic signatures, and verifier checks apply the right grammar to `Signature` and `LocalVariableTypeTable` payloads. |

## Validation and regression coverage

The Rust workspace now carries targeted regression coverage for the work above:

- parser/verifier tests for strict Java SE 25 classfile version handling,
- structured verifier tests for chapter-4 structure/access/attribute diagnostics,
- constructed verifier tests for invalid bootstrap, condy, invokedynamic, and generic-signature payloads,
- CFG/frame and symbolic-roundtrip tests for legacy `jsr` / `jsr_w` / `ret`,
- fixture-based roundtrip tests for modules, records, sealed hierarchies, type annotations, and bootstrap attributes.

## Bottom line

Within the scope this audit set out to measure, the Rust library now appears **substantially complete for JVMS 25 classfile parsing, deterministic re-emission, structural validation, bootstrap-linked constant checking, legacy subroutine-aware symbolic analysis, and generic-signature validation**.

The one scope note that remains unchanged is the original one from the top of this document: this project does **not** try to implement full JVM runtime execution semantics. The conformance claim here is about classfile/tooling behavior, not a complete virtual machine.
