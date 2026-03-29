# Project roadmap

## Roadmap aligned to the project goal

The user-provided roadmap is correct, but it benefits from calling out a few
enabling pieces explicitly. Some of these are now implemented foundations, while
others remain future work.

### Already identified

1. Create an API to manipulate classfiles
2. Calculate frames
3. Validate the manipulated classfile and generate new classfiles

### Capabilities that should stay explicit in the roadmap

#### 1. A bytecode/classfile writer ([#4](https://github.com/smithtrenton/pytecode/issues/4), [#12](https://github.com/smithtrenton/pytecode/issues/12)) — done

This layer is now implemented via `pytecode.class_writer`. `ClassWriter.write()` serializes `ClassFile` structures back to bytes in spec order, and `ClassModel.to_bytes()` exposes the direct lowering-plus-emission path for the mutable editing model. Emission recomputes derived lengths/counts from the live dataclass tree, preserves imported constant-pool ordering by default, and preserves unknown attribute payloads verbatim.

#### 2. Constant-pool management ([#5](https://github.com/smithtrenton/pytecode/issues/5)) — implemented foundation

Any manipulation API needs to create, deduplicate, update, and reindex
constant-pool entries. That foundation is now present via
`ConstantPoolBuilder`, including spec-aware Modified UTF-8 handling, lookup
helpers, MethodHandle validation, and deterministic ordering, so the roadmap
should treat it as an enabling dependency that subsequent editing/emission work
builds on.

#### 3. Symbolic labels and branch management ([#7](https://github.com/smithtrenton/pytecode/issues/7)) — implemented foundation

Editing bytecode safely required label-based branch targets instead of manual offset arithmetic. That foundation is now in place via `pytecode\labels.py`: labels support forward references, survive instruction insertion/removal, rebind exception/debug metadata, recompute `TABLESWITCH`/`LOOKUPSWITCH` padding, and widen overflowing branches during lowering.

#### 3a. Symbolic instruction operands ([#16](https://github.com/smithtrenton/pytecode/issues/16) — done)

This layer is now implemented in `pytecode.operands`. The editing model lifts the major non-control-flow instruction families to symbolic wrappers for constant-pool references, local-variable slots, `LDC` values, `INVOKEDYNAMIC`, and `MULTIANEWARRAY`, so ordinary transforms no longer need to manage raw operand encodings by hand.

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

#### 5a. Differential CFG validation against external oracles ([#17](https://github.com/smithtrenton/pytecode/issues/17) — done)

`pytecode.analysis.build_cfg()` is now validated against a JVM-side ASM oracle instead of relying only on coarse fixture assertions. The implemented test infrastructure compiles a small `RecordingAnalyzer` helper against ASM, records instruction-level normal and exceptional edges plus try/catch metadata, normalizes them into `pytecode` block spans / successor sets / handler sets, and compares both the broad `CfgFixture.java` corpus and the targeted `CfgEdgeCaseFixture.java` corpus against the Python CFG builder.

#### 6. Class hierarchy resolution ([#8](https://github.com/smithtrenton/pytecode/issues/8) — done)

This layer is now implemented in `pytecode.hierarchy`. Type merging at control-flow join points still belongs to later analysis work, but the required hierarchy queries now have a pluggable, typed foundation.

#### 7. Max stack and max locals recomputation ([#10](https://github.com/smithtrenton/pytecode/issues/10) — done)

Now implemented in `pytecode.analysis` via `compute_maxs()` and `compute_frames()`. These are integrated into `lower_code()` and `ClassModel.to_classfile()` as opt-in via `recompute_frames=True`.

#### 8. Version-aware verification rules ([#11](https://github.com/smithtrenton/pytecode/issues/11) — done)

Now implemented in `pytecode.verify`. The validation module checks version-aware feature gating and attribute constraints alongside structural classfile validation. See `pytecode/verify.py` for the full set of checks.

#### 9. Round-trip fidelity and compatibility testing ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)

To be a practical ASM/BCEL alternative, the project should prove:

- parse → emit → parse stability (idempotent round-trips)
- compatibility across representative Java compiler outputs (javac 8, 11, 17, 21, 25)
- verifier acceptance of generated classes (run through `java -verify`)
- preservation of unknown or unsupported attributes where possible
- deterministic emission for reproducible builds

Round-trip testing distinguishes three levels of fidelity:

- **Level A — Byte-for-byte identity** (`bytes₁ == bytes₂`): The gold standard for no-modification roundtrips. Achievable because `ConstantPoolBuilder.from_pool()` preserves original indexes and `lower_code()` handles instruction encoding selection. This is the default expectation for unmodified roundtrips.
- **Level B — Structural equivalence** (`parse(bytes₁) ≅ parse(bytes₂)`): For modified roundtrips where CP indexes may have shifted. Compares parsed structures with CP references resolved to symbolic values.
- **Level C — Semantic equivalence** (behavior-preserving): The weakest level — two class files define the same class with the same behavior, even if structural details differ (attribute order, debug attributes, method order).

All four validation tiers are now implemented in `tests/test_validation.py`, covering a comprehensive fixture corpus compiled at `--release 8, 11, 17, 21, 25` via JDK 25:

- **Tier 1 — Byte-for-byte roundtrip**: `ClassWriter.write()` and `ClassModel.to_bytes()` identity checks.
- **Tier 2 — Structural verification**: `verify_classfile()` regression (no new errors vs gold) plus `javap -v -p -c` exit-code validation.
- **Tier 3 — Semantic diff**: Full `javap` output parser (`tests/javap_parser.py`) with CP-aware comparison — zero error-severity diffs between gold and roundtripped output.
- **Tier 4 — JVM loading**: Custom `VerifierHarness.java` classloader with `-Xverify:all`.

#### 10. Error and diagnostics model ([#11](https://github.com/smithtrenton/pytecode/issues/11) — done)

Now implemented in `pytecode.verify`. The `Diagnostic` dataclass carries severity, category, location context (class name, method, CP index, bytecode offset), and a human-readable message. The validation entry points collect all diagnostics by default, with an optional `fail_fast=True` mode that raises `FailFastError` on the first ERROR-severity issue.

#### 11. API shape and extension strategy ([#6](https://github.com/smithtrenton/pytecode/issues/6))

The manipulation API uses Design A (direct mutable dataclasses) as the primary editing surface, chosen for its Pythonic feel, low learning curve, and natural fit with the existing `@dataclass`-based codebase. The phased extension plan is:

- **Phase 1 (done)**: Mutable tree model — `ClassModel`/`MethodModel`/`FieldModel`/`CodeModel` with symbolic references, `ConstantPoolBuilder`, and label-based instruction editing.
- **Phase 2**: Pass-style composition — `Pipeline`/`Pass` protocol for chaining transformations. Model transforms as `(builder, element) → None` functions with transform lifting (inspired by the JDK Class-File API). Add matcher-based selection predicates (inspired by Byte Buddy).
- **Phase 3 (if needed)**: Optional visitor layer for streaming — defer until there is an actual use case for high-throughput, memory-efficient bulk transformations.

See [editing model design rationale](../design/editing-model.md) for the full comparative analysis.

#### 12. Debug info management ([#13](https://github.com/smithtrenton/pytecode/issues/13) — preserve/strip policies landed)

The label-based editing model already preserved debug metadata through instruction edits by rebinding `LineNumberTable`, `LocalVariableTable`, and `LocalVariableTypeTable` entries to labels instead of raw offsets. That foundation is now complemented by explicit debug-info policy helpers in `pytecode.debug_info` plus `debug_info=` lowering controls on `lower_code()`, `ClassModel.to_classfile()`, and `ClassModel.to_bytes()`. Users can therefore keep debug info by default or strip it deliberately during mutation/emission without hand-editing nested attributes.

What remains future work is a richer first-class stale-state model for debug metadata when preserve-or-strip is not expressive enough. That follow-up is tracked separately in [#18](https://github.com/smithtrenton/pytecode/issues/18).

#### 13. JSR/RET legacy support

The legacy `JSR` and `RET` instructions (used for subroutine inlining in pre-Java 6 classfiles) are now handled by the opcode table, lowering layer, and analysis/test coverage. They remain a niche compatibility path rather than a modern workflow, because classfiles with version ≥ 51 cannot use them on current JVMs.

#### 14. Generated API reference and pydoc coverage ([#19](https://github.com/smithtrenton/pytecode/issues/19))

As the user-facing surface grows, the project should generate API reference
documentation from Python docstrings/signatures instead of relying only on
narrative guides. "Full coverage" should mean every supported public module,
class, function, method, and convenience entry point appears in the generated
reference output, with a workflow that flags newly introduced undocumented
public APIs. This work should stay aligned with the public-surface decisions in
[#6](https://github.com/smithtrenton/pytecode/issues/6) so the generated docs
track the supported API rather than transient internal helpers.

## Recommended implementation order

1. ~~Fix the known parser bugs.~~ ([#1](https://github.com/smithtrenton/pytecode/issues/1) — done)
2. ~~Add unit tests for each attribute type, instruction operand shape, and constant-pool entry.~~ ([#2](https://github.com/smithtrenton/pytecode/issues/2) — done)
3. ~~Add descriptor and signature parsing utilities.~~ ([#3](https://github.com/smithtrenton/pytecode/issues/3) — done)
4. ~~Introduce a writer foundation for primitive values and classfile sections.~~ ([#4](https://github.com/smithtrenton/pytecode/issues/4) — done)
5. ~~Add constant-pool management utilities (deduplication, symbol lookup, reindexing).~~ ([#5](https://github.com/smithtrenton/pytecode/issues/5) — done)
6. ~~Design the mutable editing model and public transformation API.~~ ([#6](https://github.com/smithtrenton/pytecode/issues/6) — Phase 1 done)
7. ~~Add label-based instruction editing with automatic offset recalculation.~~ ([#7](https://github.com/smithtrenton/pytecode/issues/7) — done)
8. ~~Add symbolic instruction operand wrappers for non-control-flow instructions.~~ ([#16](https://github.com/smithtrenton/pytecode/issues/16) — done)
9. ~~Add a pluggable class hierarchy resolver.~~ ([#8](https://github.com/smithtrenton/pytecode/issues/8) — done)
10. ~~Build control-flow graph construction and stack/local simulation.~~ ([#9](https://github.com/smithtrenton/pytecode/issues/9) — done)
11. ~~Add external-tool differential CFG validation for analysis output.~~ ([#17](https://github.com/smithtrenton/pytecode/issues/17) — done)
12. ~~Implement max stack, max locals, and stack map frame recomputation.~~ ([#10](https://github.com/smithtrenton/pytecode/issues/10) — done)
13. ~~Implement validation with structured diagnostics and version-aware rules.~~ ([#11](https://github.com/smithtrenton/pytecode/issues/11) — done)
14. ~~Add classfile emission with deterministic constant-pool layout.~~ ([#12](https://github.com/smithtrenton/pytecode/issues/12) — done)
15. ~~Broaden debug info management beyond label rebinding.~~ ([#13](https://github.com/smithtrenton/pytecode/issues/13) — done; explicit stale-state modeling moved to [#18](https://github.com/smithtrenton/pytecode/issues/18))
16. ~~Add round-trip and verifier-focused regression coverage.~~ ([#14](https://github.com/smithtrenton/pytecode/issues/14) — done)
17. Add optional JAR rewrite support. ([#15](https://github.com/smithtrenton/pytecode/issues/15))
18. Add pydoc-based API reference generation with full public-surface coverage. ([#19](https://github.com/smithtrenton/pytecode/issues/19))
