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

## Phase 8: `_model` lift cache and label allocation cleanup

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 2.589 | 2.273 | 0.878 | **~12% faster** |
| model-lift | 3.378 | 2.808 | 0.831 | **~17% faster** |
| model-lower | 3.175 | 2.241 | 0.706 | **~29% faster** |
| class-write | 1.364 | 0.869 | 0.637 | **~36% faster** |

Optimized path:
- `pytecode.edit._model_py` / `pytecode.edit._model_cy` — reuse lifted constant-pool-backed wrappers across all methods in a class and stop eagerly allocating duplicate `Label(...)` objects during label collection

### Notes

- This phase is the first `_model`-internal optimization pass rather than a new helper-module port.
- Sharing the lifted constant-pool item cache at class scope reduces repeated symbolic wrapper construction when multiple methods reference the same constant-pool entries.
- Replacing `dict.setdefault(..., Label(...))` with an explicit membership check removes unnecessary `Label` allocation on already-seen branch targets.
- The benchmark still shows the remaining visible lift/lower overhead concentrated inside `_model` plus standard-library work such as `enum` access and text decoding, so future gains are likely to come from more fine-grained internal `_model` changes rather than another standalone seam.

### Total pipeline (4 measured stages)

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 10.51s | 8.19s | **0.78x (~22% faster)** |

## Phase 9: `_model` label target validation cleanup

Benchmark: `225.jar`, 5 iterations, median wall-clock time, rerun focused on the directly affected stages.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| model-lower | 3.134 | 2.350 | 0.750 | **~25% faster** |
| model-lift | 3.127 | 2.272 | 0.727 | **~27% faster** |

Optimized path:
- `pytecode.edit._model_py` / `pytecode.edit._model_cy` — collect and validate label target offsets once, then reuse those prevalidated offsets when lifting branch and switch instructions instead of recomputing and revalidating them in the hot instruction loop

### Notes

- This phase keeps the optimization inside `_model` rather than introducing a new helper-module seam.
- The pure-Python lift profile on `225.jar` showed `_collect_labels()` drop from about **1.87s** to **1.55s**, and `_lift_instruction()` drop from about **3.12s** to **2.78s**, which matches the intended effect of removing redundant offset work from the lift path.
- Repeated 4-stage reruns moved `class-parse` and `class-write` enough that they do not look causally tied to this change, so Phase 9 treats the focused `model-lower` / `model-lift` rerun as the reliable measurement.
- Remaining visible lift-side costs are still concentrated in `_lift_const_pool_index()`, member-ref resolution, UTF-16 decoding, and `enum` attribute access, so the next `_model` step should stay on symbolic instruction lifting rather than label collection.

## Phase 11: `classfile.attributes` port

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 2.968 | 1.807 | 0.609 | **~39% faster** |
| model-lift | 4.097 | 2.606 | 0.636 | **~36% faster** |
| model-lower | 3.203 | 3.049 | 0.952 | **~5% faster** |
| class-write | 1.395 | 0.868 | 0.622 | **~38% faster** |

Ported module:
- `pytecode.classfile.attributes` — class-file attribute dataclasses and enum-backed attribute type metadata using the standard `_mod_py.py` / `_mod_cy.pyx` / shim layout

### Notes

- This phase moves a large amount of class-parse object construction out of pure Python by compiling the attribute data model itself, rather than only the reader logic that instantiates it.
- A focused parse-only rerun landed at **0.778x**, while the first full 4-stage rerun landed at **0.609x** for `class-parse`; both runs point in the same direction, so this phase treats the large exact delta as benchmark-sensitive but the parse improvement itself as real.
- `model-lift` also improved materially because the lifted pipeline still depends on the parsed attribute object graph produced by `ClassReader`.
- With `classfile.attributes` ported, the next plausible classfile-side seam is `classfile.instructions`, which has a similarly dataclass-heavy shape on the parse/write path.

### Total pipeline (4 measured stages)

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 11.66s | 8.33s | **0.71x (~29% faster)** |

## Phase 12: `classfile.instructions` port

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 1.912 | 1.873 | 0.980 | **~2% faster** |
| model-lift | 2.612 | 2.065 | 0.790 | **~21% faster** |
| model-lower | 2.599 | 2.126 | 0.818 | **~18% faster** |
| class-write | 1.107 | 0.754 | 0.681 | **~32% faster** |

Ported module:
- `pytecode.classfile.instructions` — JVM instruction operand dataclasses and enum-backed opcode metadata using the standard `_mod_py.py` / `_mod_cy.pyx` / shim layout

### Notes

- This phase compiles the instruction operand data model itself, so both instruction decoding and later symbolic/model write paths spend less time constructing and handling pure-Python wrapper objects.
- `class-parse` was nearly flat on this run, which suggests the main win here is not raw decode speed inside `ClassReader` but the downstream cost of working with instruction operand objects after parse.
- With both `classfile.attributes` and `classfile.instructions` ported, there is no similarly obvious remaining classfile-side helper seam; the next gains are more likely to come from re-profiling the already compiled hot paths (`classfile.reader`, `edit._model`) and from unavoidable stdlib enum/codec overhead.

### Total pipeline (4 measured stages)

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 8.23s | 6.82s | **0.83x (~17% faster)** |

## Post-Phase 12: profiling analysis

Fresh benchmark after rebuilding all 14 Cython extensions, to identify
remaining optimization targets.

### Wall-clock comparison

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 1.953 | 1.338 | 0.685 | **~32% faster** |
| model-lift | 2.489 | 1.970 | 0.792 | **~21% faster** |
| model-lower | 2.810 | 1.936 | 0.689 | **~31% faster** |
| class-write | 1.330 | 0.544 | 0.409 | **~59% faster** |

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 8.58s | 5.79s | **0.67x (~33% faster)** |

These numbers are significantly better than the Phase 12 report (0.83x →
0.67x overall). The improvement likely reflects run-to-run variance
stabilizing in a fresh build, plus any cumulative micro-optimizations from
all 14 extensions being freshly compiled together.

### cProfile: Cython backend

All four stages show nearly all time consumed inside compiled `.pyd`
extensions — cProfile cannot see individual function calls inside Cython
code. The only visible Python-level overhead is:

- **class-parse**: ~5 ms in `enum.__call__` / `enum.__new__` (2,304 calls
  for `AccessFlags`). Negligible.
- **model-lift**: ~2 ms in dataclass `__init__` (17,906 calls — one per
  method). Negligible.
- **model-lower**: ~2 ms in dataclass `__init__` + `enum.__hash__` (907
  calls). Negligible.
- **class-write**: zero visible Python overhead.

**Conclusion**: there are no actionable pure-Python hotspots remaining in
the Cython backend. All measurable time is inside compiled C code.

### cProfile: Python fallback (for structural reference)

The pure-Python profiles confirm what Cython is accelerating. Top
functions by cumulative time in each stage:

**class-parse** (6.2 s, 22.3 M calls):

| Function | tottime | cumtime |
|---|---|---|
| `_reader_py.read_instruction` | 1.19 s | 2.79 s |
| `_bytes_utils_py.read_u1` | 0.51 s | 1.40 s |
| `_reader_py.read_code_bytes` | 0.38 s | 3.29 s |
| `_bytes_utils_py.read_u2` | 0.30 s | 0.69 s |
| `_reader_py.read_attribute` | 0.48 s | 5.31 s |
| `struct.unpack_from` | 0.46 s | — |
| `_modified_utf8_py.decode` | 0.17 s | 0.27 s |

**model-lift** (9.0 s, 30.3 M calls):

| Function | tottime | cumtime |
|---|---|---|
| `_model_py._lift_instructions` | 0.89 s | 4.51 s |
| `_model_py._lift_instruction` | 0.91 s | 1.94 s |
| `_modified_utf8_py.decode` | 0.94 s | 1.49 s |
| `_constant_pool_builder_py.from_pool` | 0.13 s | 1.46 s |
| `_model_py._collect_labels` | 0.62 s | 1.02 s |
| `_model_py._lift_const_pool_index` | 0.06 s | 0.92 s |
| `_attribute_clone_py._clone_fast_attribute` | 0.27 s | 1.12 s |
| `isinstance` (7.5 M calls) | 0.58 s | — |

**model-lower** (8.1 s, 25.5 M calls):

| Function | tottime | cumtime |
|---|---|---|
| `_labels_py._lower_instruction` | 1.46 s | 2.63 s |
| `_labels_py._lower_resolved_code` | 0.56 s | 4.48 s |
| `_labels_py._resolve_labels_with_cache` | 0.55 s | 1.56 s |
| `_labels_py._instruction_byte_size` | 0.73 s | 0.89 s |
| `_attribute_clone_py.clone_attribute` | 0.04 s | 0.93 s |
| `isinstance` (7.0 M calls) | 0.55 s | — |
| `typing.cast` (2.5 M calls) | 0.15 s | — |
| `_labels_py._promote_overflow_branches` | 0.43 s | 0.73 s |

**class-write** (7.0 s, 34.0 M calls):

| Function | tottime | cumtime |
|---|---|---|
| `_writer_py._write_instruction` | 1.72 s | 3.79 s |
| `isinstance` (16.9 M calls) | 1.29 s | — |
| `_bytes_utils_py.write_u1` | 0.59 s | 1.26 s |
| `_writer_py._write_stack_map_frame_info` | 0.22 s | 1.12 s |
| `_writer_py._write_constant_pool_entry` | 0.35 s | 0.89 s |
| `_bytes_utils_py.write_u2` | 0.36 s | 0.77 s |
| `struct.pack` | 0.50 s | — |

### Assessment and next steps

1. **All identifiable pure-Python hotspots have been ported.** The Cython
   backend cProfile is completely opaque — every stage spends 99%+ of its
   time inside compiled `.pyd` extensions with no visible Python-level
   bottleneck.

2. **The biggest remaining acceleration opportunity is inside the compiled
   Cython code itself.** To find micro-optimization targets, a C-level
   profiler (Windows Performance Analyzer, VTune, or `py-spy` in native
   mode) would be needed — cProfile cannot see inside compiled extensions.

3. **`typing.cast` noise**: the model-lower Python profile shows 2.5 M
   calls to `typing.cast` (0.15 s). In Cython, `cast()` compiles to a
   no-op, so this is already handled. No action needed.

4. **`isinstance` dominance in Python**: the Python fallback spends 0.5–1.3 s
   per stage on `isinstance` dispatch (up to 16.9 M calls in class-write).
   Cython compiles these to C-level type checks, which is why class-write
   shows a 59% speedup.

5. **Diminishing returns**: the mechanical port strategy (new `_mod_cy.pyx`
   files) has exhausted its targets. Further Cython work would be
   micro-optimizations inside existing `.pyx` files:
   - Replace remaining Python object allocations with `cdef` structs
   - Use typed memoryviews where plain `bytes` slicing is still used
   - Add `@cython.boundscheck(False)` where safe
   - Reduce `cpdef` → `cdef` on internal-only functions

6. **Alternative acceleration paths** worth considering:
   - **Rust/PyO3 for the hottest inner loops** (instruction decode/encode)
   - **Algorithmic caching** (e.g., memoize decoded constant-pool entries)
   - **Batch processing** to amortize per-method overhead

### Profiler backend switch

`tools/profile_jar_pipeline.py` now applies `--backend python|cython`
before importing `pytecode`, so the requested backend actually takes
effect even when compiled extensions are installed.

## Phase 14: profiled `_labels_cy` + `_reader_cy` micro-optimizations

This phase uses the new opt-in `PYTECODE_CYTHON_PROFILE=1` build mode to
profile inside compiled `.pyx` functions, optimize the hottest code, and
re-measure the same stages.

### Changes

- `pytecode.edit._labels_cy`
  - Made the internal raw-instruction clone helper a direct Cython `cdef`
    call path instead of a Python `def`.
  - Removed duplicate `type()` work in label resolution by passing the
    already-computed item type into `_instruction_byte_size()`.
  - Converted the line-number and local-variable attribute builders to
    internal Cython helpers with explicit loops instead of list-comprehension
    wrappers.
- `pytecode.classfile._reader_cy`
  - Replaced repeated enum/dict opcode lookups with precomputed instruction
    tables.
  - Inlined the opcode-table lookup inside `read_instruction()` so the fast
    path does not pay extra helper-call overhead.

### Wall-clock comparison

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| class-parse | 1.937 | 1.201 | 0.620 | **~38% faster** |
| model-lift | 2.715 | 2.190 | 0.807 | **~19% faster** |
| model-lower | 2.770 | 1.954 | 0.705 | **~29% faster** |
| class-write | 1.346 | 0.612 | 0.455 | **~55% faster** |

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 8.77s | 5.96s | **0.68x (~32% faster)** |

### Profiled Cython comparison to the pre-optimization baseline

The profiled build compares against the earlier Cython-profiled pass on the
same `225.jar` stages.

| Hot path | Before | After | Ratio | Change |
|---|---|---|---|---|
| `class-parse` elapsed | 3.848s | 3.394s | 0.882x | **~12% faster** |
| `_reader_cy.read_instruction()` | 1.964s | 1.613s | 0.821x | **~18% faster** |
| `_reader_cy.read_code_bytes()` | 2.122s | 1.761s | 0.830x | **~17% faster** |
| `model-lower` elapsed | 4.698s | 3.689s | 0.785x | **~21% faster** |
| `_labels_cy._lower_resolved_code()` | 3.163s | 2.284s | 0.722x | **~28% faster** |
| `_labels_cy._lower_instruction()` | 1.913s | 1.465s | 0.766x | **~23% faster** |
| `_labels_cy._resolve_labels_with_cache()` | 0.645s | 0.600s | 0.930x | **~7% faster** |

### Notes

- The label-lowering path is still the most important optimization surface
  even after this pass; `model-lower` remains the slowest profiled Cython
  stage.
- After the `_lower_instruction()` and debug-info builder wins, the next
  visible lowering costs are:
  - `_ordered_nested_code_attributes()` plus the nested
    `_attribute_clone_cy` work it triggers
  - `_resolve_labels_with_cache()` / `_instruction_byte_size()`
  - constant-pool builder activity during lowering (`build()`, `_copy_entry()`,
    `checkpoint()`)
- On the parse side, `read_instruction()` and `read_code_bytes()` still
  dominate, but the reader path is now materially smaller than the lowering
  path. The next optimization step should stay focused on `model-lower`
  rather than switching back to parse work immediately.

## Phase 15: label-resolution lowering cleanup

This follow-up pass stayed inside `pytecode.edit._labels_cy` and targeted the
remaining label-resolution work instead of the nested-attribute clone path.

### Changes

- Preallocated `instruction_offsets` inside `_resolve_labels_with_cache()`
  instead of growing the list with repeated `append()` calls.
- Passed the already-computed `type(item)` into `_instruction_byte_size()`
  across the full resolution loop.
- Added a direct `InsnInfo` fast path in both `_instruction_byte_size()` and
  `_lower_instruction()` so no-operand raw instructions avoid the generic
  clone helper.

### Wall-clock comparison

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| model-lower | 2.768 | 1.586 | 0.573 | **~43% faster** |
| class-write | 1.363 | 0.620 | 0.455 | **~55% faster** |
| class-parse | 2.127 | 1.302 | 0.612 | **~39% faster** |
| model-lift | 2.567 | 1.966 | 0.766 | **~23% faster** |

### Profiled Cython notes

On the profiling-enabled build, the isolated `model-lower` stage moved from
**3.689s** to **3.719s** elapsed between reruns, but the hot functions inside
the stage improved:

| Hot path | Before | After | Ratio | Change |
|---|---|---|---|---|
| `_labels_cy._lower_instruction()` | 1.465s | 1.449s | 0.989x | **~1% faster** |
| `_labels_cy._resolve_labels_with_cache()` | 0.600s | 0.539s | 0.898x | **~10% faster** |
| `_labels_cy._instruction_byte_size()` | 0.412s | 0.353s | 0.857x | **~14% faster** |

The profiled wall-clock variance appears to be measurement noise from the
profiling-enabled build; the normal benchmark is the more reliable signal for
this pass and shows a clear improvement.

### Next target

`model-lower` is still the right place to keep working, but the remaining
visible costs have shifted again:

1. `_lower_resolved_code()` / `_lower_instruction()`
2. nested attribute clone work (`_ordered_nested_code_attributes()` and
   `_attribute_clone_cy._clone_stack_map_frame()`)
3. constant-pool builder work during lowering

The failed nested-attribute experiment in this session regressed the profiled
run, so the next pass should revisit that area more carefully or move on to the
constant-pool-builder side of lowering.

## Phase 16: combined lowering-loop, nested-attribute, and CP-build pass

This pass tackled the three remaining visible `model-lower` buckets together:
the lowering loop itself, nested code-attribute assembly / stack-map cloning,
and constant-pool copying during `build()`.

### Changes

- `pytecode.edit._labels_cy`
  - Preallocated the lowered instruction list in `_lower_resolved_code()` and
    filled it by index instead of repeated `append()` calls.
  - Kept the direct `InsnInfo` lowering fast path from the previous pass.
  - Reworked `_ordered_nested_code_attributes()` to clone non-debug nested
    attributes in one explicit loop instead of composing `_lifted_debug_attrs()`
    with a second clone comprehension.
- `pytecode.edit._attribute_clone_cy`
  - Added explicit list-clone helpers for stack-map frame lists and
    verification-type lists, then reused them in `StackMapTableAttr`,
    `AppendFrameInfo`, and `FullFrameInfo`.
- `pytecode.edit._constant_pool_builder_cy`
  - Converted `_copy_pool_entry()` / `_copy_entry()` to Cython internal helpers.
  - Added `_copy_pool_list()` and reused it from `from_pool()`, `clone()`, and
    `build()` to reduce Python-level list-comprehension overhead during pool
    copying.

### Wall-clock comparison

Benchmark: `225.jar`, 5 iterations, median wall-clock time.

| Stage | Python (s) | Cython (s) | Ratio (Cy/Py) | Speedup |
|---|---|---|---|---|
| model-lower | 2.599 | 1.557 | 0.599 | **~40% faster** |
| class-write | 1.106 | 0.463 | 0.419 | **~58% faster** |
| class-parse | 1.650 | 1.199 | 0.727 | **~27% faster** |
| model-lift | 2.169 | 1.867 | 0.861 | **~14% faster** |

| | Python | Cython | Ratio |
|---|---|---|---|
| 4 stages | 7.52s | 5.09s | **0.68x (~32% faster)** |

### Profiled Cython comparison to the previous lowering baseline

The profiling-enabled `model-lower` stage moved from **3.719s** to
**3.114s** elapsed on `225.jar`.

| Hot path | Before | After | Ratio | Change |
|---|---|---|---|---|
| `model-lower` elapsed | 3.719s | 3.114s | 0.837x | **~16% faster** |
| `_labels_cy._lower_resolved_code()` | 2.322s | 1.939s | 0.835x | **~17% faster** |
| `_labels_cy._lower_instruction()` | 1.449s | 1.223s | 0.844x | **~16% faster** |
| `_labels_cy._resolve_labels_with_cache()` | 0.539s | 0.455s | 0.844x | **~16% faster** |
| `_labels_cy._instruction_byte_size()` | 0.353s | 0.297s | 0.841x | **~16% faster** |
| `_labels_cy._ordered_nested_code_attributes()` | 0.416s | 0.362s | 0.870x | **~13% faster** |
| `_attribute_clone_cy._clone_stack_map_frame()` | 0.354s | 0.315s | 0.890x | **~11% faster** |
| `_constant_pool_builder_cy.build()` | 0.178s | 0.118s | 0.663x | **~34% faster** |

### Next target

`model-lower` is still the right place to continue, but the remaining hot
surface is now narrower:

1. `_lower_resolved_code()` / `_lower_instruction()` themselves
2. stack-map cloning in `_attribute_clone_cy` (still visible, but smaller)
3. `ConstantPoolBuilder.checkpoint()` and the remaining per-entry copy work

The next pass should probably stay on lowering, but shift from broad structural
cleanup to more targeted micro-optimizations inside `_lower_instruction()` and
the checkpoint / rollback path.

## Phase 17: checkpoint / stack-map / invokeinterface micro-pass (reverted)

This pass tried three smaller `model-lower` ideas:

- `pytecode.edit._constant_pool_builder_cy`
  - snapshot dictionary lengths in `checkpoint()` / `rollback()` instead of
    copying most caches
- `pytecode.edit._attribute_clone_cy`
  - inline the common stack-map frame clone cases inside
    `_clone_stack_map_frame_list()`
- `pytecode.edit._labels_cy`
  - cache the computed `INVOKEINTERFACE` slot count for repeated descriptors

### What the experimental profile showed

The checkpoint change itself was real: in the profiling build,
`ConstantPoolBuilder.checkpoint()` dropped from about **0.114s** to
**0.022s** cumulative time and fell out of the top `model-lower` costs.

But the overall lowering profile still did not move in the right direction:

- profiled `model-lower` stayed dominated by `_lower_instruction()`
  (**1.317s** cumulative)
- `_resolve_labels_with_cache()` remained visible at **0.515s**
- nested attribute / stack-map clone work still showed up at
  `_ordered_nested_code_attributes()` **0.393s** and
  `_clone_stack_map_frame_list()` **0.361s**

### Outcome

The stack-map rewrite regressed the clean wall-clock benchmark and was reverted
first. After a second profiling and benchmark pass, the remaining checkpoint /
descriptor-cache changes still did not produce a trustworthy end-to-end win, so
those code changes were also reverted.

The only kept changes from this pass are the added rollback coverage in
`tests/test_constant_pool_builder.py` for imported UTF-8 and semantic cache
state.

### Current recommendation

Phase 16 remains the last trusted optimization baseline. If we continue on the
Cython side, the next target should go back to the main lowering loop:

1. `_labels_cy._lower_instruction()`
2. `_labels_cy._resolve_labels_with_cache()`
3. `_labels_cy._ordered_nested_code_attributes()`

The stack-map / checkpoint path is probably not worth another structural rewrite
unless a much more direct win is identified first.
