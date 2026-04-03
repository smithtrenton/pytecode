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
- `model-lift` and `model-lower` stages are not Cython-ported — they operate on Python dataclass trees and would see minimal benefit from Cython.
