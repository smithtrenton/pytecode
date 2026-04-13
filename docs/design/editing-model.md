# Editing model design rationale

This document records the design evaluation that led to the current editing API.
It predates the Rust migration and should be read as historical rationale, not
as a description of the current implementation. Five candidate designs were
analyzed against pytecode's original roadmap and Python ergonomics, and a
survey of eight additional JVM bytecode libraries identified further design
patterns that informed the resulting extension strategy.

## Context

At the time of this evaluation, pytecode's parsed output was a tree of Python
objects carrying raw constant-pool indexes and byte offsets. Today that raw
surface is Rust-backed and exposed through `pytecode.classfile`, but the core
editing problem is unchanged: direct raw mutation forces callers to manage
constant-pool indexes, recalculate branch offsets, and handle WIDE instruction
expansion themselves. The editing model therefore needed to support all planned
features:

| Feature | Issue | Interaction |
|---------|-------|-------------|
| Constant-pool management | [#5](https://github.com/smithtrenton/pytecode/issues/5) | Create/deduplicate/reindex CP entries during edits |
| Label-based branches | [#7](https://github.com/smithtrenton/pytecode/issues/7) | Use labels, not raw offsets |
| Class hierarchy resolution | [#8](https://github.com/smithtrenton/pytecode/issues/8) | Frame computation needs hierarchy info |
| Control-flow analysis | [#9](https://github.com/smithtrenton/pytecode/issues/9) | CFG and stack simulation depend on instruction representation |
| Frame recomputation | [#10](https://github.com/smithtrenton/pytecode/issues/10) | Must run after instruction edits |
| Validation | [#11](https://github.com/smithtrenton/pytecode/issues/11) | Validates editing model output before emission |
| Emission | [#12](https://github.com/smithtrenton/pytecode/issues/12) | Serializes editing model back to bytes |
| Debug info management | [#13](https://github.com/smithtrenton/pytecode/issues/13) | Debug attrs must track label positions through edits |

## Candidate designs

**Design A — Direct Mutable Dataclasses**: Mutable `@dataclass` objects with symbolic (resolved) references. Users edit by mutating fields in-place. A lowering step converts back to indexed form before emission.

- *Advantages*: Pythonic and discoverable, minimal conceptual overhead, natural fit with existing `@dataclass` codebase, good for small surgical edits, straightforward serialization via tree walk.
- *Disadvantages*: Full materialization required, no streaming, invariant maintenance deferred to validation/emission, two-model problem (read model vs edit model requires conversion).

**Design B — Builder Objects (BCEL-style)**: `ClassBuilder`/`MethodBuilder`/`InstructionListBuilder` objects that accumulate state and produce finalized output via `build()`.

- *Advantages*: Encapsulated invariant management, clear lifecycle with explicit `build()` finalization, good for class generation from scratch, constant pool handled automatically.
- *Disadvantages*: Verbose (setter methods instead of attribute access), not Pythonic, transformation of existing classes is awkward (must reconstruct builder state), large API surface.

**Design C — Visitor/Transformer Pattern (ASM-style)**: Abstract `ClassVisitor`/`MethodVisitor` classes that receive events during a streaming pass. Transformations override specific `visit_*` methods and delegate to downstream visitors.

- *Advantages*: Streaming / low memory, composable transformation chains, efficient for bulk JAR processing, proven at scale (ASM is the de facto JVM standard).
- *Disadvantages*: Steep learning curve, un-Pythonic (deep class hierarchies, callback lifecycle), random access is impossible, difficult debugging through visitor chains, massive API surface (~30 `visit_*` methods), poor fit with existing tree-based codebase.

**Design D — Pass Pipelines**: Transformations as composable functions (`ClassTree → ClassTree`). Passes are pure functions composed into a pipeline.

- *Advantages*: Highly composable, testable in isolation, Pythonic (functions and closures), clear data flow, easy ordering/scheduling.
- *Disadvantages*: Deep-copy overhead if passes produce new trees, requires a tree model underneath (not a standalone design), overkill for simple one-field edits, pass ordering complexity at scale.

**Design E — Dual Approach (Tree + Optional Visitor)**: Tree model (Design A) as primary API, with an optional visitor/event model (Design C) for streaming use cases. This is ASM's actual architecture (`org.objectweb.asm.tree` over the core visitor API).

- *Advantages*: Best of both worlds, incremental implementation (build tree first, add visitor later), proven architecture.
- *Disadvantages*: Largest implementation effort (~1.5–2× either approach alone), cognitive overhead (two APIs), consistency burden across both models, visitor layer may never be needed for a Python-centric audience.

## Comparative feature matrix

| Criterion | A: Mutable DC | B: Builder | C: Visitor | D: Passes | E: Dual |
|-----------|:---:|:---:|:---:|:---:|:---:|
| Pythonic feel | ★★★★★ | ★★☆☆☆ | ★☆☆☆☆ | ★★★★☆ | ★★★★☆ |
| Learning curve | Low | Medium | High | Low | Medium |
| Streaming support | ✗ | ✗ | ✓ | ✗ | ✓ |
| Random access | ✓ | Partial | ✗ | ✓ | ✓ |
| Class generation | Medium | Excellent | Poor | Medium | Good |
| Class transformation | Good | Fair | Excellent | Good | Excellent |
| Composability | Poor | Poor | Excellent | Excellent | Excellent |
| Implementation effort | Low | Medium | High | Low+ | High |
| Fit with existing codebase | Excellent | Fair | Poor | Good | Good |
| Invariant safety | Low | High | Medium | Medium | Medium |
| Memory efficiency | Low | Low | High | Low | Varies |

## Recommendation and phased approach

Given pytecode's characteristics — Python-first audience, interactive/scripting
primary use case, small team, and a roadmap full of other features — **a
mutable symbolic model as the primary editing surface** was the strongest
starting point, with pass composition (Design D) layered on naturally. That
composition layer has now landed via `pytecode.transforms`, while a full
visitor layer (Design E) was evaluated and found not yet justified (see Phase 3
below).

- **Phase 1 (done)**: `ClassModel`/`MethodModel`/`FieldModel`/`CodeModel` as Rust-backed mutable symbolic objects with `ConstantPoolBuilder`, label-based instruction editing, and symbolic operand wrappers.
- **Phase 2 (done)**: Pass-style composition in `pytecode.transforms` — callable `Pipeline` objects, `pipeline()` construction, `on_classes()` / `on_fields()` / `on_methods()` / `on_code()` lifting helpers, owner-class filtering on the field/method/code lifting helpers, and a richer `Matcher` DSL with `&` / `|` / `~` composition, regex helpers, lightweight structural helpers, access-flag convenience matchers, plus the original functional combinators for callers that prefer them. Transforms remain ordinary in-place `ClassModel` callables so they plug directly into existing lowering and `JarFile.rewrite()` flows.
- **Phase 3 (evaluated — [#21](https://github.com/smithtrenton/pytecode/issues/21) — done)**: A full visitor/streaming API was evaluated and found not yet justified — no concrete use cases exist for high-throughput streaming or memory-efficient bulk processing that the current tree model cannot handle. The evaluation identified a concrete gap: `FieldTransform`, `MethodTransform`, and `CodeTransform` lacked access to their owning context. Rather than introducing a second traversal model, the existing protocols were updated to pass context directly: field and method transforms receive the owning `ClassModel`, code transforms receive both the owning `MethodModel` and `ClassModel`. This addresses the real gap while keeping the API surface minimal.
- **Future follow-up**: Declarative instruction-pattern matching may still be layered on top of the current matcher/pipeline surface if real-world use cases justify the extra API surface.

## Survey of other bytecode libraries

A broader survey of JVM bytecode manipulation libraries identified additional design patterns:

| Library | Core Design Pattern | Relevance to pytecode |
|---------|-------------------|----------------------|
| **Javassist** | Source-level abstraction + mutable tree | Mutable tree validates Design A; `insertBefore`/`insertAfter` helpers are worth adopting. Source-level compilation not portable (requires Java compiler). |
| **Byte Buddy** | Fluent declarative DSL over ASM | Matcher-based selection (`ElementMatcher` predicates) worth borrowing for Phase 2. Fluent builder chains less Pythonic. |
| **Soot / SootUp** | IR lifting (Jimple, Shimple, Baf, Grimp) | Out of scope (massive effort). pytecode's symbolic model is effectively "Baf-like." SootUp's `BodyInterceptor` aligns with planned pass pipelines. |
| **WALA Shrike** | Patch-based instrumentation | Strong alternative to mutable `InstructionList` — patches reference original positions, so independent edits don't interfere. pytecode could offer both: direct mutation for simple edits, patch-based editing for complex multi-edit scenarios. |
| **JDK Class-File API** (JEP 457/484) | Immutable elements + builders + composable transforms | Most architecturally relevant new pattern. Transform lifting (`CodeTransform` → `MethodTransform` → `ClassTransform`) validates planned Phase 2 design. In Python, structural pattern matching on elements replaces visitor hierarchies. |
| **ProGuardCORE** | Visitor + instruction pattern matching engine | Declarative find-and-replace on bytecode sequences. Layerable on top of the tree model as a Phase 2+ addition. |
| **Krakatau** | Text-based assembly/disassembly | Not directly applicable (different use case). |
| **CafeDude (CAFED00D)** | Simple read/write tree (obfuscation-resilient) | Uses a mutable tree internally, further validating Design A as the universal substrate. |

## Summary of applicable patterns

| Pattern | Source | Applicable? | Phase |
|---------|--------|:-----------:|-------|
| Source-level abstraction | Javassist | ✗ (requires Java compiler) | — |
| Fluent declarative DSL / matchers | Byte Buddy | Partially | Future follow-up |
| IR lifting | Soot/SootUp | ✗ (massive scope) | — |
| Patch-based editing | WALA Shrike | ✓ (alternative to mutable InstructionList) | Phase 1–2 |
| Immutable element + transform lifting | JDK Class-File API | ✓ (informs Phase 2 transform design) | Phase 2 |
| Instruction pattern matching | ProGuardCORE | ✓ (declarative find-and-replace) | Future follow-up |

None of the surveyed designs displace a mutable symbolic editing surface as the
Phase 1 choice. Every library surveyed either uses a mutable tree internally
(Javassist, CafeDude, BCEL), builds on a visitor/tree substrate (Byte Buddy,
ProGuardCORE), or uses an IR that is a different kind of tree (Soot). A mutable
editing model remains the common substrate.
