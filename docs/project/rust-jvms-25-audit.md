# Historical JVMS 25 audit

This document records the earlier audit's feature inventory. The September 2026
review reproduced bugs in unary arithmetic, shifts, frame categories, archive
mutations, and Java strings despite the earlier completion claim. Feature presence
and internal roundtrip tests do not establish classfile or verifier conformance.

See [compatibility and limits](compatibility-and-limits.md) for current behavior and
[the improvement plan](improvement-plan-2026-09.md) for implementation evidence.

## Implemented areas

- Raw constant-pool tags, predefined attributes, and unknown-attribute preservation.
- Instruction parsing, symbolic lift/lower, labels, and debug information policies.
- Descriptor and generic-signature parsing.
- Structural diagnostics for constant-pool linkage, access flags, attributes,
  bootstrap references, and method/class structure.
- CFG analysis, stack/local simulation, and frame emission, including selected
  legacy subroutine cases.

The current inspection policy covers majors 45 through 70, including historical
preview files. Runtime preview compatibility is checked separately. This replaces
the earlier claim that historical preview minors should always be rejected.

## Evidence and limits

Rust fixtures exercise records, sealed hierarchies, modules, lambdas/bootstrap
methods, type annotations, nests, and classfile versions through Java 25. A separate
Java 26 lane compiles and runs GA and preview examples. Independent JVM checks force
method verification and execute selected recomputed methods; generated properties
and bounded sanitizer fuzzing supplement the fixtures.

These checks provide regression evidence for the exercised paths. They do not
prove every operand, assignability rule, constructor path, exception edge, attribute
combination, or resource-exhaustion case correct. Continue adding independent JVM
and malformed-input regressions when changing those areas. The raw structural
verifier is not a substitute for the JVM's full type verifier.
