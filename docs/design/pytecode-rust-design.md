# pytecode in Rust: general plan and roadmap

## Purpose

This document outlines a Rust-first plan for reimplementing `pytecode` as a pure Rust library, with later Python integration as a thin compatibility layer.

The core strategic goal is:

1. build a complete, high-performance Rust implementation of the `pytecode` design,
2. prove it as a standalone Rust library first,
3. then expose it back to Python as a fast backend for the existing Python package.

The project should optimize for correctness, compatibility, and performance in that order.

## Why Rust

Rust is a strong fit for `pytecode` because the library is fundamentally about:

- structured binary parsing and emission,
- memory-safe manipulation of graph-like data,
- deterministic lowering and validation,
- CPU-heavy archive and bytecode processing,
- potential parallelism across classes and JAR entries.

Rust gives us:

- predictable performance,
- memory safety without a GC,
- strong enum/struct modeling for JVM formats,
- zero-cost abstractions for typed indices and instruction variants,
- easy future parallelization for archive-wide transforms,
- a good path to Python bindings without making Python the primary design constraint.

## Project goals

### Primary goals

1. **Rust-first completeness**
   - implement the full conceptual `pytecode` pipeline in Rust,
   - treat the Rust library as the source implementation,
   - avoid a design that only exists to satisfy Python binding mechanics.

2. **Behavioral compatibility**
   - preserve the semantics captured in [`pytecode-design.md`](pytecode-design.md),
   - preserve the two-model architecture,
   - preserve safe bytecode editing abstractions,
   - preserve validation, analysis, and JAR rewrite behavior.

3. **High performance**
   - outperform the current pure-Python paths materially on parse, lift, lower, write, analyze, and JAR rewrite workloads,
   - minimize allocations and redundant conversions,
   - enable parallelism where behavior allows it.

4. **Future Python backend**
   - keep clear FFI boundaries so the Rust engine can be exposed to Python later,
   - avoid leaking Python-centric assumptions into core Rust APIs.

### Secondary goals

- ergonomic Rust-native API,
- deterministic output,
- strong validation and differential testing,
- straightforward packaging and CI.

## Non-goals

At least initially, this project should **not** try to:

- preserve Python implementation internals,
- reproduce Cython-specific fast paths,
- ship Python bindings before the Rust core is viable,
- support streaming visitor APIs before the tree/edit pipeline is complete,
- optimize every advanced edge case before the compatibility surface exists.

## Source-of-truth constraints

The **primary design source of truth** for the Rust implementation should be the **JVM specification**. If there is ever tension between the current Python implementation and the JVM spec, the Rust design should follow the spec and treat Python behavior as something to reconcile, document, or fix rather than blindly preserve.

The current project docs and tests should be treated as the main **compatibility reference**:

- [`pytecode-design.md`](pytecode-design.md) for semantic contracts,
- README examples for expected workflows,
- public module inventory for supported surfaces,
- tests for concrete behavior,
- current architecture docs for responsibility boundaries.

The compatibility target is the **behavior** of the current library where that behavior is consistent with the spec, not the exact Python class layout.

## Top-level product shape

The end state should be a Rust workspace with a reusable core library and a later Python binding layer.

Suggested workspace shape:

| Crate | Purpose |
|---|---|
| `pytecode-core` | shared low-level primitives, typed indexes, common errors, utility traits |
| `pytecode-classfile` | raw JVM classfile parsing and emission |
| `pytecode-edit` | symbolic editing model, labels, operands, constant-pool builder |
| `pytecode-analysis` | CFG, frame simulation, hierarchy, verifier |
| `pytecode-archive` | JAR read/mutate/rewrite support |
| `pytecode-transform` | matcher DSL and transform orchestration |
| `pytecode-cli` | smoke-test and benchmark/debug tooling |
| `pytecode-python` | later PyO3/maturin bindings for Python integration |

This can begin as fewer crates if that speeds delivery. The important point is keeping the layers conceptually separate even if the first implementation is more consolidated.

## Architectural principles

### 1. Keep the two-model architecture

The Rust version should preserve:

- a **raw spec model** for exact classfile representation,
- a **symbolic edit model** for ergonomic mutation.

This is central to the current library's design. It should not be collapsed into a single model just to reduce implementation work.

### 2. Optimize the boundaries, not just the internals

The heavy transitions are:

- bytes -> raw model,
- raw model -> edit model,
- edit model -> raw model,
- raw model -> bytes,
- JAR entry loops over many classes.

Performance work should focus first on those boundaries because they dominate end-to-end workload cost.

### 3. Use Rust-native safety for invariants

Important invariants should move from runtime convention to type-level structure where practical:

- typed constant-pool indexes,
- typed access flags,
- explicit label IDs,
- explicit class internal names vs descriptors where useful,
- clear distinction between raw instructions and symbolic instructions.

### 4. Design for Python binding later, not now

The Rust library should expose a clean Rust API first. The Python layer can later:

- wrap Rust structs directly where practical,
- or expose opaque handles and convert at API boundaries,
- or keep the current Python façade and delegate heavy work into Rust.

The core should not be forced into awkward Python-shaped ownership or mutability patterns before needed.

## Core data-model plan

### Raw spec model

The raw model should stay close to the JVM classfile format:

- `ClassFile`, `FieldInfo`, `MethodInfo`,
- constant-pool entry enum + per-entry structs,
- attribute enum/struct families,
- raw instruction enum and operand structs,
- descriptors/signatures,
- Modified UTF-8 codec support.

Recommended design:

- use enums for closed sets like constant-pool tags, attributes, and instructions,
- use typed newtypes for indexes like `CpIndex`, `MethodIndex`, `FieldIndex`,
- preserve unknown attributes as opaque byte payloads,
- keep imported ordering and slot layout where roundtrip fidelity depends on it.

### Symbolic edit model

The edit model should preserve the same abstraction level as current `pytecode`:

- symbolic class/member references,
- labels instead of branch offsets,
- symbolic exception/debug tables,
- constant-pool builder owned by the model,
- explicit lowering back to raw structures.

Recommended design:

- `ClassModel`, `FieldModel`, `MethodModel`, `CodeModel`,
- `LabelId` instead of pointer-identity objects,
- symbolic operand enums/structs for field/method/type/var/ldc/invokedynamic/multianewarray,
- edit-friendly collections with stable IDs where necessary.

### Label representation

Python currently uses identity-based label objects. In Rust, a better fit is:

- `LabelId(u32)` or a similar typed handle,
- a `LabelArena` or stable ID allocator per `CodeModel`,
- instruction streams that can contain label markers plus symbolic instructions.

This preserves semantics while fitting Rust ownership rules better.

## Performance strategy

The performance plan should be explicit from the start.

### Fast-path priorities

1. classfile parsing,
2. raw-to-symbolic lifting,
3. symbolic-to-raw lowering,
4. classfile writing,
5. JAR rewriting,
6. analysis/frame recomputation.

### Likely techniques

- contiguous `Vec`-backed storage,
- typed indexes instead of hash-heavy object graphs where possible,
- zero-copy or delayed-copy handling for unknown attributes and unmodified resource payloads,
- deterministic but cache-friendly constant-pool interning,
- pre-sized buffers for emission,
- parallel class processing for JAR rewrites,
- careful avoidance of intermediate string churn,
- borrowed parsing where it helps, owned lowering where it simplifies correctness.

### Performance rule

Do not overcomplicate the first version with speculative micro-optimizations. Start with a correct, allocation-aware design that leaves room for profiling-driven optimization.

## Compatibility strategy

The Rust library should aim for **semantic compatibility** with the current Python library.

Compatibility expectations:

1. parse the same supported classfile features,
2. preserve unknown attributes,
3. preserve deterministic lowering behavior,
4. preserve label-based editing semantics,
5. preserve transform pipeline semantics,
6. preserve verifier and hierarchy behavior,
7. preserve JAR rewrite guarantees,
8. preserve public option meanings such as `skip_debug`, `recompute_frames`, and debug-info policy handling.

Areas where compatibility can be adapted:

- exact Rust API naming,
- exact internal storage layout,
- exact error-type hierarchy if the behavior is still clear,
- exact match to Python dataclass ergonomics.

## Suggested implementation phases

### Phase 0: project setup and compatibility harness

**Goal:** create the Rust workspace and define the compatibility target before core implementation grows.

Deliverables:

- Cargo workspace layout,
- crate skeletons,
- fixture import strategy using existing `.class`/`.jar` test resources,
- cross-language differential test harness,
- benchmark harness aligned with current stage names,
- CI for format, lint, tests, benchmarks-on-demand.

Exit criteria:

- Rust workspace builds cleanly,
- fixtures can be loaded from the existing repository,
- baseline benchmarks and diff tests can run even if most features are stubs.

### Phase 1: raw classfile parse/write core

**Goal:** implement the raw model, parser, writer, descriptors, constants, and Modified UTF-8.

Deliverables:

- raw classfile structs/enums,
- Modified UTF-8 codec,
- descriptor parser/formatter,
- constant-pool parsing and emission,
- attribute parsing and emission,
- raw instruction parsing and emission,
- roundtrip support for representative classfiles.

Exit criteria:

- parse -> write -> parse works across the existing fixture corpus,
- unknown attributes survive roundtrip,
- deterministic writer behavior is established,
- invalid input produces structured parse errors.

### Phase 2: constant-pool builder and symbolic edit model

**Goal:** implement the Rust equivalent of `ClassModel`, `CodeModel`, and the pool builder.

Deliverables:

- `ConstantPoolBuilder`,
- raw-to-symbolic lift path,
- symbolic model structs,
- symbolic operand wrappers,
- symbolic labels and exception/debug metadata,
- symbolic-to-raw lowering without frame recomputation.

Exit criteria:

- unmodified lift/lower roundtrips preserve semantics and high-fidelity behavior,
- editing simple class/field/method metadata works,
- control-flow instructions lower correctly from labels.

### Phase 3: code editing completeness

**Goal:** make bytecode editing ergonomic and correct for real transforms.

Deliverables:

- full label resolution,
- switch lowering with padding,
- branch widening/inversion logic,
- local-variable shorthand/wide normalization,
- symbolic LDC and member/type instruction lowering,
- debug-info lifting/lowering and staleness behavior.

Exit criteria:

- existing edit/labels/operands/debug-info behaviors have Rust equivalents,
- code-shape edits can be performed without raw offset or CP index management.

### Phase 4: analysis, hierarchy, and verification

**Goal:** implement the advanced correctness layers needed for frame recomputation and validation.

Deliverables:

- verification-type system,
- CFG construction,
- stack/local simulation,
- frame recomputation,
- hierarchy resolver interfaces and mapping resolver,
- classfile and classmodel verification diagnostics.

Exit criteria:

- `recompute_frames` works on representative fixture classes,
- verifier produces structured diagnostics,
- hierarchy-aware merges and override detection work,
- advanced analysis results are usable from Rust directly.

### Phase 5: transforms and archive layer

**Goal:** implement high-level workflows users actually rely on.

Deliverables:

- transform pipeline abstraction,
- matcher DSL or equivalent selector layer,
- JAR read/mutate/rewrite support,
- safe atomic rewrite behavior,
- resource preservation and metadata preservation,
- optional parallel class processing inside archive rewrites.

Exit criteria:

- Rust can perform end-to-end JAR transformations analogous to current README workflows,
- transforms can target classes/fields/methods/code with deterministic traversal semantics.

### Phase 6: polish, optimization, and packaging

**Goal:** make the Rust library production-ready on its own.

Deliverables:

- profiling-driven optimization passes,
- stronger documentation,
- examples and CLI helpers,
- stable error messages and public docs,
- versioning and release process,
- benchmark reports against current Python implementation.

Exit criteria:

- Rust implementation is feature-complete enough to stand alone,
- benchmark results justify moving Python hot paths onto Rust.

### Phase 7: Python backend integration

**Goal:** make Rust the high-performance backend for the Python package.

Deliverables:

- PyO3/maturin binding crate,
- Python-compatible top-level entry points,
- conversion or handle strategy between Python-facing objects and Rust internals,
- fallback/feature-flag rollout plan,
- packaging and wheel build integration.

Exit criteria:

- Python can use Rust for the heavy paths without breaking the current API story,
- feature parity is sufficient for default use in targeted workflows.

## Recommended roadmap order

The practical roadmap should be:

1. **Raw parse/write first**
   - easiest place to establish correctness and speed,
   - lowest API ambiguity,
   - highest confidence-building milestone.
2. **Edit model second**
   - defines the usability story,
   - unlocks real transformations.
3. **Analysis/verify next**
   - required for safe lowering and advanced workflows.
4. **JAR + transforms after core edit/analyze layers**
   - depends on stable lower-level abstractions.
5. **Python bindings last**
   - once the Rust API and performance profile are stable.

## Python integration strategy

When Rust is ready, Python integration should be phased rather than all-at-once.

### Integration options

| Option | Pros | Cons |
|---|---|---|
| thin Rust-wrapped Python object model | closest to current API | larger binding surface, more wrapper work |
| opaque Rust handles with Python façade | smaller FFI boundary, faster internals | harder Python ergonomics |
| hybrid: Rust for parse/lower/analyze/JAR loops, Python for orchestration | fastest path to adoption | duplicated model logic if taken too far |

Recommended direction:

- start with a **hybrid backend** for the heaviest operations,
- then move more model ownership into Rust once the integration story is proven.

That gives earlier wins without forcing the first Rust release to expose the entire edit model safely into Python.

## Validation strategy

The Rust project should adopt the current repository as its validation oracle.

Recommended validation layers:

1. fixture-based parse/write roundtrip tests,
2. differential tests against Python `pytecode` outputs,
3. verifier tests using the same conceptual cases as current Python tests,
4. JVM-backed execution/verification tests where already used by the repository,
5. targeted benchmark suites for parse/lift/lower/write/JAR workflows.

This is especially important because the Rust port is both a reimplementation and a future backend.

## Risks and design tensions

### 1. Rust-native design vs Python compatibility

If the Rust core is made too Python-shaped too early, it may lose clarity and performance. If it is made too Rust-specific, later Python wrapping may become expensive.

Mitigation:

- keep conceptual compatibility, not literal API lockstep,
- delay final Python binding decisions until the Rust core stabilizes.

### 2. Edit-model ownership complexity

Labels, symbolic references, and mutable code editing can become awkward under strict ownership.

Mitigation:

- use stable IDs and arena-style storage where needed,
- avoid self-referential structures,
- keep lowering explicit.

### 3. Feature breadth

`pytecode` covers a large JVM surface: attributes, signatures, analysis, frames, transforms, and archives.

Mitigation:

- phase work aggressively,
- keep phase exit criteria concrete,
- prioritize the parts that unlock the most workflows.

### 4. Premature FFI work

Binding work can consume time before the Rust core is mature.

Mitigation:

- explicitly defer Python integration until after core phases.

## Initial milestone recommendations

If starting immediately, the first three milestones should be:

### Milestone A: Rust raw classfile engine

- parser,
- writer,
- descriptors,
- Modified UTF-8,
- constant-pool support,
- roundtrip tests.

### Milestone B: Rust symbolic model

- `ClassModel` equivalents,
- labels,
- operand wrappers,
- pool builder,
- basic lowering.

### Milestone C: Rust correctness layer

- CFG,
- verifier,
- hierarchy,
- frame recomputation.

Only after those should archive transforms and Python backend work become top priorities.

## Definition of success

The project is successful when:

1. a Rust-native user can parse, inspect, edit, validate, and rewrite classfiles and JARs without needing Python,
2. the Rust implementation is measurably faster than the current pure-Python implementation on the main workflows,
3. the Python package can later adopt the Rust engine incrementally without changing its core semantics.

## Bottom line

The right path is:

- **Rust first, Python second**,
- **behavioral compatibility, not implementation cloning**,
- **parse/write core first, edit/analyze layers next, bindings last**.

That approach gives the best chance of ending up with both a strong standalone Rust library and a high-performance backend for the Python `pytecode` package.
