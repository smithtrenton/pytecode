# Cython benchmark results

This document tracks performance comparisons between the pure-Python and Cython implementations of pytecode's hot-path modules.

## Methodology

Benchmarks use `tools/benchmark_cython.py` to run each pipeline stage under both backends in isolated subprocesses. Each stage is run multiple times and the median wall-clock time is reported. The profiler (`tools/profile_jar_pipeline.py`) also supports a `--backend python|cython` flag for single-backend profiling with cProfile.

Test artifacts:
- **Focused JAR**: `225.jar` (all stages)
- **Corpus**: common-jar directory (model-lift + model-lower stages)

Results are saved as JSON under `output/profiles/` for trend tracking.

## Initial port results (all 4 modules)

Benchmark: `225.jar`, 3 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 2.246 | 1.958 | 0.872 | **~13% faster** |
| class-write | 1.310 | 0.837 | 0.639 | **~36% faster** |

Ported modules contributing to these results:
- `pytecode._internal.bytes_utils` — BytesReader/BytesWriter (typed memoryview, inline struct ops)
- `pytecode.classfile.modified_utf8` — Modified UTF-8 codec (C-level byte iteration)
- `pytecode.classfile.reader` — ClassReader (typed locals, C-level dispatch)
- `pytecode.classfile.writer` — ClassWriter (typed locals, direct buffer ops)

### Notes

- `class-write` benefits more because `BytesWriter` avoids Python `struct.Struct` overhead on every field emit.
- `class-parse` improvement is limited by Python object creation (dataclass construction for attributes, constant pool entries, instructions) which dominates the profile.
- `model-lift` and `model-lower` stages were not yet Cython-ported — they operate on Python dataclass trees and were the next targets.

## Phase 2: edit-layer ports

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 2.19 | 1.81 | 0.83 | **~17% faster** |
| model-lift | 3.02 | 2.61 | 0.86 | **~14% faster** |
| model-lower | 2.19 | 2.53 | 1.15 | ~15% slower (see notes) |
| class-write | 1.19 | 0.74 | 0.62 | **~38% faster** |

When model-lower is benchmarked in isolation (without prior stages in the same process), it measures **0.71–0.75x** (25–29% faster). The regression seen in the full-pipeline run is attributed to GC/memory pressure from prior stages — the profiler does not call `gc.collect()` between stages.

Ported modules contributing to these results:
- `pytecode.edit.labels` — label resolution, branch lowering, instruction sizing (`cdef int` typed hot loops)
- `pytecode.edit.constant_pool_builder` — CP entry allocation, deduplication (typed index variables)
- `pytecode.edit.model` — model lift/lower orchestration (`cdef` locals, removed `cast()`)

### Notes

- **class-write** continues to show the largest improvement, now 50% faster (0.50x) in some runs.
- **model-lift** benefits from Cython despite being dataclass-construction heavy — the constant-pool traversal and attribute lifting loops gain from typed locals.
- **model-lower** shows variable results in the full pipeline due to interaction with prior-stage memory allocation. In isolation it is consistently faster. A potential optimization is adding `gc.collect()` between stages in the profiler, or pre-computing type dispatch tables in `_lower_instruction()`.
- **class-parse** improved slightly from Phase 1 (0.87→0.83x), likely due to reduced import overhead now that more modules are compiled.

### Total pipeline (end-to-end)

| | Python | Cython | Ratio |
|---|---|---|---|
| Full pipeline | 8.64s | 7.74s | **0.90x (~10% faster overall)** |

The end-to-end speedup is modest because most time is spent in Python object creation (dataclasses), which Cython does not accelerate.

## Phase 3: attribute clone port

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 2.36 | 2.11 | 0.89 | **~11% faster** |
| model-lift | 3.45 | 2.94 | 0.85 | **~15% faster** |
| model-lower | 2.48 | 2.57 | 1.04 | ~4% slower (see notes) |
| class-write | 1.39 | 0.79 | 0.57 | **~43% faster** |

Ported module:
- `pytecode.edit._attribute_clone` — 40-way `isinstance()` attribute cloning dispatch (`cdef` internal functions)

### Notes

- **model-lift** improved slightly from Phase 2 (0.86→0.85x) — the attribute clone helpers were a Cython→Python boundary crossing that is now eliminated.
- **model-lower** improved significantly from Phase 2 (1.15→1.04x) — reducing Python object creation during attribute cloning lowers GC pressure from prior stages.
- **class-write** shows the largest improvement yet (0.62→0.57x), likely from reduced overall memory/GC interaction.
- **class-parse** shows typical run-to-run variance (0.83→0.89x) — no code changes affect this stage.

### Total pipeline (end-to-end)

| | Python | Cython | Ratio |
|---|---|---|---|
| Full pipeline | 9.73s | 8.46s | **0.87x (~13% faster overall)** |

The overall speedup improved from ~10% (Phase 2) to ~13%, primarily from eliminating Cython→Python boundary crossings in the attribute cloning hot path.

## Phase 4: analysis port

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 2.396 | 2.098 | 0.876 | **~12% faster** |
| model-lift | 3.141 | 2.718 | 0.865 | **~14% faster** |
| model-lower | 2.885 | 2.359 | 0.818 | **~18% faster** |
| class-write | 1.359 | 0.810 | 0.596 | **~40% faster** |

Ported module:
- `pytecode.analysis` — CFG construction, dataflow simulation, stack-map frame computation (`cdef`/`cpdef` typed hot loops, `except *`/`except -1` exception specifiers)

### Notes

- **model-lower** improved significantly from Phase 3 (1.04→0.82x), likely from reduced GC pressure now that more modules are compiled and the analysis dispatch overhead is eliminated.
- **class-write** continues to show the largest cumulative benefit (0.60x).
- The `compute_frames()` function is called only when `recompute_frames=True` (explicit frame recomputation during bytecode editing) — its Cython speedup does not show in the standard pipeline benchmark. Users who recompute frames will see additional improvement on top of the numbers above.
- `wraparound=False`/`boundscheck=False` directives were intentionally **not** applied to `_analysis_cy.pyx` because the module uses negative list indices (`blocks[-1]`, `block.instructions[-1]`). Only `cdivision=True` is enabled.

### Total pipeline (4 measured stages)

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 9.78s | 7.99s | **0.82x (~18% faster)** |

## Phase 5: operands port

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 2.701 | 2.030 | 0.752 | **~25% faster** |
| model-lift | 3.601 | 3.050 | 0.847 | **~15% faster** |
| model-lower | 3.216 | 2.544 | 0.791 | **~21% faster** |
| class-write | 1.392 | 0.851 | 0.612 | **~39% faster** |

Ported module:
- `pytecode.edit.operands` — symbolic operand wrapper construction and validation (`FieldInsn`, `MethodInsn`, `VarInsn`, `LdcInsn`, etc.) using the standard `_mod_py.py` / `_mod_cy.pyx` / shim layout

### Notes

- **model-lift** improved from **0.865x** in Phase 4 to **0.847x** here after compiling the operand-wrapper constructors that dominated the remaining visible Python time.
- **model-lower** also improved from **0.818x** to **0.791x** because the same symbolic wrappers are created, validated, and lowered across the edit pipeline.
- **class-parse** does not use `edit.operands`; the larger shift there is benchmark variance rather than a direct effect of this port.
- A post-port `cProfile` run on the Cython backend no longer shows the operand-wrapper `__init__` methods among the top costs. The next clear pure-Python hotspot in the lowering path is `pytecode.classfile.descriptors.parse_method_descriptor()`.

### Total pipeline (4 measured stages)

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 10.91s | 8.48s | **0.78x (~22% faster)** |

## Phase 6: descriptors port

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 2.246 | 1.974 | 0.879 | **~12% faster** |
| model-lift | 3.134 | 2.624 | 0.837 | **~16% faster** |
| model-lower | 2.885 | 1.961 | 0.680 | **~32% faster** |
| class-write | 1.176 | 0.761 | 0.647 | **~35% faster** |

Ported module:
- `pytecode.classfile.descriptors` — field/method descriptor parsing, generic signature parsing, descriptor serialization, and slot counting using the standard public-module shim pattern

### Notes

- The targeted follow-up benchmark for the directly affected stages measured **0.804x** for `model-lower` and **0.841x** for `model-lift`, confirming that the descriptors seam improves the intended path even when unrelated pipeline stages fluctuate.
- The first 4-stage run briefly showed `class-parse` at **1.013x**; a rerun returned it to **0.879x**, so this phase treats `class-parse` movement as benchmark variance rather than a causal effect of the descriptors port.
- A post-port `cProfile` run no longer shows `parse_method_descriptor()` or its recursive helpers among the top costs. The remaining visible pure-Python work in the lowering path is now concentrated in `pytecode.edit.debug_info`.
- Most of the remaining `model-lift` overhead is standard-library work (`enum` attribute access and UTF-16 decoding), so future wins there are likely to be smaller or require a different seam than the earlier structural ports.

### Total pipeline (4 measured stages)

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 9.44s | 7.32s | **0.78x (~22% faster)** |

## Phase 7: debug_info port

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 2.533 | 2.193 | 0.866 | **~13% faster** |
| model-lift | 3.497 | 2.829 | 0.809 | **~19% faster** |
| model-lower | 3.085 | 2.367 | 0.767 | **~23% faster** |
| class-write | 1.372 | 0.859 | 0.626 | **~37% faster** |

Ported module:
- `pytecode.edit.debug_info` — debug-info policy normalization, stale-state checks, and stripping helpers using the standard `_mod_py.py` / `_mod_cy.pyx` / shim layout

### Notes

- The `debug_info` helpers disappeared from the top visible Python costs in the post-port lowering profile, which means this seam is no longer a meaningful standalone hotspot.
- The remaining visible `model-lower` overhead is now mostly standard-library work (`enum` attribute access, `dataclasses.is_dataclass`) plus time still aggregated inside `lower_models()` itself.
- The remaining visible `model-lift` overhead is also dominated by standard-library work, especially `enum` access and UTF-16 decoding, which suggests future wins will likely require `_model` micro-optimizations or a different seam than the previous helper-module ports.
- Because the remaining costs are less isolated than earlier phases, Phase 7 is best treated as another incremental step rather than a dramatic new seam.

### Total pipeline (4 measured stages)

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 10.49s | 8.25s | **0.79x (~21% faster)** |
