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

#### 1. A bytecode/classfile writer ([#4](https://github.com/smithtrenton/pytecode/issues/4), [#12](https://github.com/smithtrenton/pytecode/issues/12))

Generation is mentioned, but it is worth calling out explicitly as a major subsystem. Writing new class files is more than a final `to_bytes()` method; it requires a full serialization pipeline including instruction offset resolution, constant-pool layout, attribute length computation, and `WIDE` instruction insertion when operands exceed single-byte range.

#### 2. Constant-pool management ([#5](https://github.com/smithtrenton/pytecode/issues/5)) — implemented foundation

Any manipulation API needs to create, deduplicate, update, and reindex
constant-pool entries. That foundation is now present via
`ConstantPoolBuilder`, including spec-aware Modified UTF-8 handling, lookup
helpers, MethodHandle validation, and deterministic ordering, so the roadmap
should treat it as an enabling dependency that subsequent editing/emission work
builds on.

#### 3. Symbolic labels and branch management ([#7](https://github.com/smithtrenton/pytecode/issues/7)) — implemented foundation

Editing bytecode safely required label-based branch targets instead of manual offset arithmetic. That foundation is now in place via `pytecode\labels.py`: labels support forward references, survive instruction insertion/removal, rebind exception/debug metadata, recompute `TABLESWITCH`/`LOOKUPSWITCH` padding, and widen overflowing branches during lowering.

#### 3a. Symbolic instruction operands ([#16](https://github.com/smithtrenton/pytecode/issues/16))

Control flow is now symbolic, but most non-branch instructions still expose raw constant-pool indexes, local-variable slot encodings, and other operand-level details through spec-shaped `InsnInfo` records. A follow-on symbolic operand layer should add editing-model wrappers for common non-control-flow instructions (for example, constant-pool references and local-slot references) so transformations and future analyses can avoid manual raw-index bookkeeping altogether.

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

#### 5a. Differential CFG validation against external oracles ([#17](https://github.com/smithtrenton/pytecode/issues/17))

Now that `pytecode.analysis.build_cfg()` exists, the project should validate its graph shape against a JVM-side oracle instead of relying only on coarse fixture assertions. The recommended design is an ASM `Analyzer`-based differential suite that records normal and exceptional instruction-level edges, normalizes them into `pytecode` block spans / successor sets / handler sets, and uses BCEL only as an optional slower second opinion. This gives the roadmap an explicit place to track confidence in CFG correctness before later work builds on that analysis layer.

#### 6. Class hierarchy resolution ([#8](https://github.com/smithtrenton/pytecode/issues/8) — done)

This layer is now implemented in `pytecode.hierarchy`. Type merging at control-flow join points still belongs to later analysis work, but the required hierarchy queries now have a pluggable, typed foundation.

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

Round-trip testing distinguishes three levels of fidelity:

- **Level A — Byte-for-byte identity** (`bytes₁ == bytes₂`): The gold standard for no-modification roundtrips. Achievable because `ConstantPoolBuilder.from_pool()` preserves original indexes and `lower_code()` handles instruction encoding selection. This is the default expectation for unmodified roundtrips.
- **Level B — Structural equivalence** (`parse(bytes₁) ≅ parse(bytes₂)`): For modified roundtrips where CP indexes may have shifted. Compares parsed structures with CP references resolved to symbolic values.
- **Level C — Semantic equivalence** (behavior-preserving): The weakest level — two class files define the same class with the same behavior, even if structural details differ (attribute order, debug attributes, method order).

#### 10. Error and diagnostics model ([#11](https://github.com/smithtrenton/pytecode/issues/11))

Manipulation and validation need structured errors, not only parser exceptions. Users will need actionable messages when a transformation creates an invalid class. Errors should carry location context (class name, method, bytecode offset, constant-pool index) and the validation layer should support collecting all diagnostics rather than failing on the first.

#### 11. API shape and extension strategy ([#6](https://github.com/smithtrenton/pytecode/issues/6))

The manipulation API uses Design A (direct mutable dataclasses) as the primary editing surface, chosen for its Pythonic feel, low learning curve, and natural fit with the existing `@dataclass`-based codebase. The phased extension plan is:

- **Phase 1 (done)**: Mutable tree model — `ClassModel`/`MethodModel`/`FieldModel`/`CodeModel` with symbolic references, `ConstantPoolBuilder`, and label-based instruction editing.
- **Phase 2**: Pass-style composition — `Pipeline`/`Pass` protocol for chaining transformations. Model transforms as `(builder, element) → None` functions with transform lifting (inspired by the JDK Class-File API). Add matcher-based selection predicates (inspired by Byte Buddy).
- **Phase 3 (if needed)**: Optional visitor layer for streaming — defer until there is an actual use case for high-throughput, memory-efficient bulk transformations.

See [editing model design rationale](../design/editing-model.md) for the full comparative analysis.

#### 12. Debug info management ([#13](https://github.com/smithtrenton/pytecode/issues/13))

Mutation invalidates LineNumberTable, LocalVariableTable, and LocalVariableTypeTable entries because they reference bytecode offsets. The library should provide utilities to rebind debug info after transformation, strip it cleanly, or preserve it through label-based indirection.

#### 13. JSR/RET legacy support

The `JSR` and `RET` instructions (used for subroutine inlining in pre-Java 6 classfiles) are present in the opcode table but create complex control-flow for frame computation. The library should decide whether to fully support these legacy instructions or to document them as unsupported for analysis purposes. Modern JVMs do not allow them in classfiles with version ≥ 51.

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
11. Add external-tool differential CFG validation for analysis output. ([#17](https://github.com/smithtrenton/pytecode/issues/17))
12. Implement max stack, max locals, and stack map frame recomputation. ([#10](https://github.com/smithtrenton/pytecode/issues/10))
13. Implement validation with structured diagnostics and version-aware rules. ([#11](https://github.com/smithtrenton/pytecode/issues/11))
14. Add classfile emission with deterministic constant-pool layout. ([#12](https://github.com/smithtrenton/pytecode/issues/12))
15. Broaden debug info management beyond label rebinding. ([#13](https://github.com/smithtrenton/pytecode/issues/13) — partially addressed)
16. Add round-trip and verifier-focused regression coverage. ([#14](https://github.com/smithtrenton/pytecode/issues/14))
17. Add optional JAR rewrite support. ([#15](https://github.com/smithtrenton/pytecode/issues/15))
