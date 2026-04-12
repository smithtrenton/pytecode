# Best way to validate `pytecode`'s new CFG feature

This document records the research that led to the current ASM-based CFG differential suite. Keep it as design background and oracle-selection rationale rather than as an active implementation plan.

## Executive Summary

`pytecode` already has a real control-flow graph builder: it computes leaders from branch targets and handler boundaries, builds ordered basic blocks, adds normal successors, and attaches block-level exception-handler edges.[^1][^2] The main validation gap is not fixture breadth; it is that the current compiled-Java integration tests mostly assert coarse shape properties such as "has at least N blocks" or "some block has exception edges" rather than exact edge equivalence.[^3][^4]

The best primary oracle is ASM's `Analyzer`, but not `CheckClassAdapter.verify()` alone. The strongest approach is to run a small JVM helper that subclasses `Analyzer`, records `newControlFlowEdge()` and `newControlFlowExceptionEdge()`, emits instruction-level edges plus try/catch metadata as JSON, and then normalizes those results into `pytecode` blocks before diffing them against `build_cfg()`.[^5][^6][^7]

ASM is the best fit because its exception handling model is unusually close to `pytecode`'s current design: it computes handler coverage over a half-open protected range (`begin <= j < end`) and exposes both normal and exceptional edges directly during analysis.[^5][^6] BCEL is a reasonable secondary slow-suite oracle because it also exposes normal successors and protecting exception handlers at the instruction level, but its API is more tightly coupled to verifier machinery.[^8] Soot is powerful, but its exceptional CFG semantics depend on `ThrowAnalysis` and `omitExceptingUnitEdges`, which makes it a weaker "ground truth" oracle for `pytecode`'s current block-and-declared-catch-type model.[^9]

## Architecture/System Overview

The current public API already treats CFG construction and stack/local simulation as first-class analysis features. `README.md` exposes `build_cfg()`, `simulate()`, `ControlFlowGraph`, `BasicBlock`, `SimulationResult`, and `FrameState`, and notes that analysis can optionally use a `ClassResolver` for reference-type merging at join points.[^1] Internally, `BasicBlock` stores ordered instructions, normal successor ids, and `(handler_block_id, catch_type)` pairs, while `build_cfg()` constructs leaders, partitions blocks, adds branch/switch/fall-through successors, and then attaches exception handlers to blocks in the protected range.[^2]

`simulate()` is a separate forward dataflow pass. It merges stack and local states slot-by-slot at join points, tracks `max_stack` / `max_locals`, and propagates handler entry states with a single caught exception on the stack when an instruction is conservatively considered throwable.[^10]

```text
Java fixture source
   |
   +-- javac --release 8 ----------------------------------------+
   |                                                             |
   v                                                             v
.class bytes                                              JVM oracle process
   |                                                      (ASM Analyzer subclass)
   |                                                             |
   +--> ClassModel.from_bytes()                                  +-- normal instruction edges
         |                                                       +-- exceptional instruction->handler edges
         v                                                       +-- try/catch table
      CodeModel
         |
         v
      build_cfg()
         |
         +-- ordered blocks
         +-- successor_ids
         +-- exception_handler_ids

Normalize both sides to:
  - block entry instruction index
  - block successor set
  - block exception handler set
  - catch types / catch-all markers
```

The important alignment is that both ASM and `pytecode` operate near the raw bytecode/control-transfer layer. That means you can compare them after normalization without introducing a heavier IR like Jimple or SSA just to validate CFG shape.[^2][^5][^6]

## Current pytecode state and the actual validation gap

`pytecode`'s CFG builder is already substantial enough to justify a real oracle. `build_cfg()` identifies leaders from the first real instruction, branch/switch targets, and exception-handler boundary labels; labels preceding leader instructions are mapped back to the block they start; normal edges come from branch/switch targets and fall-through; exception edges are then attached to all blocks whose ids lie in the protected half-open interval.[^2] That is enough structure that edge-level differential tests will catch real bugs instead of just smoke-test regressions.[^2]

The current compiled-fixture tests are broad but shallow. `CfgFixture.java` intentionally covers straight-line control flow, branches, loops, dense and sparse switches, single and multiple handlers, `finally`, nested exceptions, object creation, arrays, monitors, null checks, and explicit `throw` paths.[^4] But the associated integration tests mostly check minimum block counts or the existence of some handler edges instead of exact adjacency and handler coverage.[^3] In practice, that means the repo already has the right corpus shape; it just needs a stronger assertion strategy.[^3][^4]

The good news is that the repository is already set up for compiled-Java differential testing. `tests/helpers.py` caches `javac --release 8` outputs under `.pytest_cache/pytecode-javac` and exposes helpers that compile a fixture source and return the resulting `.class` path, so a JVM-side oracle can consume the exact same class bytes that `ClassModel.from_bytes()` and `build_cfg()` already use.[^11]

## Candidate library comparison

| Candidate | What it exposes | Alignment with `pytecode` CFG | Integration cost | Verdict |
|---|---|---|---|---|
| ASM `Analyzer`[^5][^6][^7] | Direct normal and exceptional edge hooks during analysis | Very high: raw bytecode, half-open protected ranges, declared catch types | Low to moderate | **Best primary oracle** |
| ASM `CheckClassAdapter.verify()`[^7][^12] | Analyzer-driven verification and printed frame dumps | Medium: excellent diagnostics, but not a direct graph comparator | Low | **Use as secondary diagnostics** |
| BCEL `InstructionContext`[^8] | Normal successors plus protecting exception handlers | Medium: explicit, but verifier-centric and instruction-oriented | Moderate | **Good secondary slow-suite oracle** |
| Soot `ExceptionalUnitGraph`[^9] | Exceptional / unexceptional successors plus `ExceptionDest` sets from `ThrowAnalysis` | Lower: semantics vary with options and throw analysis | High | **Not the primary oracle** |

## Why ASM is the best fit

### 1. ASM exposes exactly the edge hooks you need

ASM's `Analyzer` computes a per-instruction handler table by walking every try/catch block and assigning it to every covered instruction index in a half-open interval (`for (int j = begin; j < end; ++j)`).[^5] During analysis it calls `newControlFlowEdge()` for fall-through, jumps, and switch targets, and it calls `newControlFlowExceptionEdge()` before merging handler states for protected instructions.[^6] Those hooks are explicitly overridable so callers can build or record a CFG while the analyzer runs.[^7]

That is almost tailor-made for a differential oracle. You do not need to infer the graph from printed verifier output; the analyzer tells you every edge as it discovers it.[^6][^7]

### 2. ASM's handler-range semantics match `pytecode` unusually well

`pytecode` attaches exception handler edges to blocks whose ids lie in `start_id <= block.id < end_id` after mapping handler labels to blocks.[^2] ASM similarly treats handlers as half-open protected ranges over analyzed instruction positions.[^5] That shared half-open model is important: it means the oracle is likely to disagree only when there is a real graph construction bug, not because the libraries fundamentally disagree about where a protected region ends.[^2][^5]

### 3. ASM maps naturally to `pytecode`'s stored exception metadata

`pytecode` stores handler edges as `(handler_block_id, catch_type)` where `catch_type` can be a JVM internal name or `None` for catch-all behavior.[^2] ASM's `TryCatchBlockNode` model provides the same declared handler information and its exception-edge hook even has an overload that receives the full `TryCatchBlockNode`, not just the numeric successor, which makes it easy to preserve catch-type information in the oracle output.[^7]

### 4. `CheckClassAdapter.verify()` is useful, but it is the wrong primary tool

The repo's own design notes already mention `ASM CheckClassAdapter` as a validation tool.[^13] The source backs that up, but it also shows why `verify()` is not the best primary CFG oracle: it is a wrapper that builds a `SimpleVerifier` and an `Analyzer`, may trigger additional class loading, and is oriented toward reporting verifier results and frame dumps rather than returning a graph structure.[^12] It is extremely useful after a mismatch, because it can print analyzer state for debugging, but it is a poorer fit for a direct, exact CFG comparison than a small custom `Analyzer` subclass.[^7][^12]

## Why BCEL is a good secondary oracle, not the first pick

BCEL's verifier API is still a legitimate control-flow source. Its `InstructionContext` interface exposes the normal successors of an instruction via `getSuccessors()` and separately exposes protecting exception handlers via `getExceptionHandlers()`, which it describes as special control-flow successors.[^8] That makes BCEL a real independent instruction-level oracle, and it is valuable if you want a second opinion beyond ASM.[^8]

The downside is ergonomics. BCEL's control-flow surface is wrapped inside its verifier-specific instruction context model rather than being exposed through lightweight edge callbacks.[^8] You can still normalize BCEL instruction contexts into `pytecode` blocks, but it is more boilerplate and less pleasant to debug. My recommendation is to use BCEL on a smaller optional suite once the ASM oracle is in place, rather than making it the first system you wire up.[^8]

## Why Soot is the wrong primary oracle for this feature

Soot's `ExceptionalUnitGraph` is sophisticated, but that sophistication is exactly why it is a weaker correctness oracle for the current `pytecode` feature. Its constructors take a `ThrowAnalysis` and an `omitExceptingUnitEdges` flag, and even the default constructor delegates to the scene's default throw analysis plus the global omit-edge option.[^9] In other words, Soot's notion of exceptional control flow is intentionally policy-driven.[^9]

Soot also represents exception destinations as `ExceptionDest` collections and `ThrowableSet`s, including escaping exceptions, rather than as just a declared catch target and type.[^9] That is a useful analysis model, but it is not the same contract that `pytecode` currently exposes.[^2][^9] If you use Soot as the primary oracle now, some mismatches will reflect different may-throw policy rather than bad block or handler construction in `pytecode`.[^9]

## Recommended validation design

### 1. Keep the fast rule-focused tests

Retain the current hand-built `CodeModel` tests for direct validation of specific invariants like leader creation, fall-through suppression after unconditional branches or returns, and frame merge failures.[^2][^10] These are still the fastest way to pin intended semantics when you change `build_cfg()` or `simulate()`.[^2][^10]

### 2. Add a JVM differential oracle based on ASM

The core design I recommend is:

1. Compile fixture Java with the existing `tests/helpers.py` path.[^11]
2. Parse the resulting bytes with `ClassModel.from_bytes()` and run `build_cfg()` in Python.[^1][^2]
3. In a separate JVM helper, read the same class with ASM, locate the target method, and run a custom `RecordingAnalyzer` that records:
   - `normalEdges: (fromInsnIndex, toInsnIndex)`
   - `exceptionEdges: (fromInsnIndex, handlerInsnIndex, catchType)`
   - raw try/catch entries `(startInsnIndex, endInsnIndex, handlerInsnIndex, catchType)`[^5][^6][^7]
4. Normalize ASM's instruction-level data into `pytecode` block-level expectations:
   - map instruction indices to the block that contains them,
   - derive each block's successor set from the terminating instruction's outgoing normal edges,
   - derive each block's handler set from protected instructions within the block,
   - compare declared catch types and catch-all markers.

That gives you exact graph equivalence without forcing `pytecode` to mirror ASM's internal numbering scheme.[^2][^5][^6]

### 3. Compare the right invariants

Do not compare raw block ids or label object identities. Compare the invariants that matter:

- entry block's first real instruction
- block spans by instruction index
- normal successor set per block
- exception-handler target set per block
- catch types / catch-all markers
- try/catch protected-range coverage

This avoids brittle false failures caused by different internal numbering, while still detecting the bugs that matter for CFG correctness.[^2][^5][^7]

### 4. Use `CheckClassAdapter.verify()` only when a diff fails

When the ASM-vs-`pytecode` diff fails, immediately run `CheckClassAdapter.verify()` (or print analyzer state from the helper) and attach its output to the test failure.[^12] That turns verifier output into a diagnostic tool instead of conflating graph correctness with general class verification behavior.[^12]

This matters because the roadmap puts CFG/simulation ahead of max-stack / max-locals / stack-map recomputation and the broader validation framework.[^13] A raw CFG oracle should therefore minimize coupling to later emission and verification work.[^13]

## Concrete test plan for this repository

### Phase A: strengthen the existing compiled-fixture assertions

The current fixture corpus is already broad enough to start. I would upgrade the assertions for a few anchor methods first:

- `ifElse` -> exact two-way successor shape
- `forLoop` / `whileLoop` -> exact back-edge
- `denseSwitch` / `sparseSwitch` -> exact successor set from the switch terminator
- `tryCatchSingle` / `tryCatchMultiple` -> exact handler target and catch types
- `tryCatchFinally` / `nestedTryCatch` -> exact multiple-handler coverage and nesting behavior[^3][^4]

These should remain direct `CodeModel` tests and assert the intended semantics directly.

### Phase B: add an ASM oracle helper

Add a tiny Java helper that prints JSON to stdout. Since the repo already compiles Java fixture sources via `javac`, the helper can be compiled and executed with the same subprocess-driven style used elsewhere in tests.[^11]

For the first version, I would keep the helper focused on CFG facts only:

- class name
- method name + descriptor
- instruction indices and opcodes
- try/catch table
- normal edges
- exceptional edges

That keeps failures sharply about CFG correctness instead of frame typing or classpath resolution.[^5][^6][^12]

### Phase C: add BCEL as an optional second opinion

Once ASM is stable, add an optional slower test marker that runs a smaller subset of the corpus through BCEL and checks that BCEL's instruction-level normal and exceptional successors normalize to the same block graph.[^8] This reduces the chance that a bug shared by `pytecode` and ASM goes unnoticed.

### Phase D: keep Soot off the blocking path

If you later want a deeper research suite around exceptional control flow, Soot becomes interesting. But because its output depends on `ThrowAnalysis` and omit-edge policy, it should not gate the primary CFG correctness suite for the current feature.[^9]

## Edge cases I would cover explicitly

Even with the existing broad fixture, I would add or isolate these as dedicated differential cases:

- overlapping handlers with different catch types
- catch-all / `finally` handlers
- consecutive labels before a single real instruction
- unreachable code after `goto`, `return`, or `athrow`
- constructor control flow involving `new` / `<init>`
- synchronized blocks and compiler-generated `finally` shapes
- multi-catch compiled output
- large `tableswitch` / `lookupswitch`
- `JSR` / `RET`, likely in a separate non-blocking suite because they target older classfile versions and should not dominate the main CFG corpus.[^14]

## Final recommendation

If you want one answer: **use ASM `Analyzer` as a differential CFG oracle, not `CheckClassAdapter.verify()` alone.**[^6][^7][^12]

That gives you:

- the closest semantic match to `pytecode`'s current block-and-handler model,[^2][^5]
- explicit normal and exceptional edge callbacks,[^6][^7]
- straightforward normalization into `BasicBlock.successor_ids` and `exception_handler_ids`,[^2]
- a clean path to stronger downstream diagnostics through `CheckClassAdapter`,[^12]
- and a natural fit with the repo's existing compiled-Java fixture pipeline.[^11]

BCEL is worth adding later as a slower second opinion.[^8] Soot is better saved for future analysis experiments, not for the first correctness oracle.[^9]

## Key Repositories Summary

| Repository | Purpose | Key files |
|---|---|---|
| [llbit/ow2-asm](https://github.com/llbit/ow2-asm) | Primary recommended oracle; exposes CFG edge hooks during bytecode analysis[^5][^6][^7] | `src/org/objectweb/asm/tree/analysis/Analyzer.java`, `src/org/objectweb/asm/util/CheckClassAdapter.java` |
| [apache/commons-bcel](https://github.com/apache/commons-bcel) | Secondary independent instruction-level oracle via verifier contexts[^8] | `src/main/java/org/apache/bcel/verifier/structurals/InstructionContext.java` |
| [soot-oss/soot](https://github.com/soot-oss/soot) | Powerful but policy-driven exceptional CFG framework[^9] | `src/main/java/soot/toolkits/graph/ExceptionalUnitGraph.java` |

## Confidence Assessment

High confidence:

- ASM is the best primary oracle for `pytecode`'s current CFG feature because its edge hooks and half-open exception-range handling are the closest match to the current `build_cfg()` design.[^2][^5][^6][^7]
- The repository's immediate gap is assertion strength, not fixture diversity.[^3][^4]

Medium confidence:

- BCEL is a solid optional second oracle, but I did not trace its full modern verifier pipeline as deeply as ASM; my recommendation to use it as a secondary suite is based on its explicit `InstructionContext` successor and handler API.[^8]

Lower-confidence / inferred:

- If you adopt a newer ASM release than the public mirror commit inspected here, re-check exact class and method signatures before implementation. The recommendation depends on the `Analyzer` hook pattern, which is strongly evidenced by the inspected source, but I did not fetch the entire latest released source tree in full.

## Footnotes

[^1]: `README.md:19-20`.
[^2]: `pytecode/analysis.py:750-788,790-1006`.
[^3]: `tests/test_analysis.py:1250-1345`.
[^4]: `tests/resources/CfgFixture.java:2-7,90-134,157-266`.
[^5]: [llbit/ow2-asm](https://github.com/llbit/ow2-asm), `src/org/objectweb/asm/tree/analysis/Analyzer.java:113-131,157-176` (commit `a695934043d6f8f0aee3f6867c8dd167afd4aed8`).
[^6]: [llbit/ow2-asm](https://github.com/llbit/ow2-asm), `src/org/objectweb/asm/tree/analysis/Analyzer.java:196-236,268-288` (commit `a695934043d6f8f0aee3f6867c8dd167afd4aed8`).
[^7]: [llbit/ow2-asm](https://github.com/llbit/ow2-asm), `src/org/objectweb/asm/tree/analysis/Analyzer.java:447-491` (commit `a695934043d6f8f0aee3f6867c8dd167afd4aed8`).
[^8]: [apache/commons-bcel](https://github.com/apache/commons-bcel), `src/main/java/org/apache/bcel/verifier/structurals/InstructionContext.java:49-53,75-80` (commit `d1fe24f8cdec137218f910d2bdaa8e4329c92a2b`).
[^9]: [soot-oss/soot](https://github.com/soot-oss/soot), `src/main/java/soot/toolkits/graph/ExceptionalUnitGraph.java:83-87,90-101,126-156,199-225,257-266` (commit `40af5d8ef28eac41c96c3ee98909858153d33329`).
[^10]: `pytecode/analysis.py:394-414,1044-1123,1330-1368`.
[^11]: `tests/helpers.py:20-23,48-59,89-99,165-177`.
[^12]: [llbit/ow2-asm](https://github.com/llbit/ow2-asm), `src/org/objectweb/asm/util/CheckClassAdapter.java:121-123,209-240,257-264,331-349` (commit `a695934043d6f8f0aee3f6867c8dd167afd4aed8`).
[^13]: `docs/design/validation-framework.md:35-46,114-123`; `docs/project/roadmap.md:113-118`.
[^14]: `docs/project/roadmap.md:98-100`.
