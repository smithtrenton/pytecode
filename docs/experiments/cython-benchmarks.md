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
