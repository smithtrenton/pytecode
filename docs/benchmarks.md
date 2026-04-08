# Benchmarks: Rust vs Python Performance

Benchmark comparing the Rust (`pytecode-engine`) and Python (`pytecode`) implementations
across all pipeline stages. Each stage is benchmarked in isolation with fresh setup.

**Corpus:** byte-buddy-1.17.5.jar (5,928 classes, 20.4 MB)
**Iterations:** 5 per stage (median reported)
**Platform:** Windows, release build (`cargo run --release`)

## Stage-by-Stage Comparison

| Stage | Rust (ms) | Python (ms) | Speedup |
|-------|-----------|-------------|---------|
| jar-read | 136 | 166 | **1.2×** |
| class-parse | 109 | 5,339 | **49.0×** |
| model-lift | 397 | 10,320 | **26.0×** |
| model-lower | 321 | 2,011 | **6.3×** |
| class-write | 47 | 1,575 | **33.5×** |
| **Total** | **1,010** | **19,411** | **19.2×** |

## Key Observations

### Rust-native pipeline: 19× faster end-to-end

All five stages show speedups ranging from 1.2× (I/O-bound jar-read) to 49×
(class-parse). The compute-heavy stages — parse, lift, lower, write — show the
largest gains because they involve intensive data structure construction that
benefits from Rust's zero-allocation patterns and compiled codegen.

### Bridge overhead dominates when crossing back to Python

Rust parse + lift stays in Rust for ~500ms. However, crossing back to Python via
the PyO3 bridge to create Python dataclass objects costs ~19s for 5,928 classes —
comparable to the pure Python path (~18s). This means `ClassModel.from_bytes()`
(Rust + bridge) is **not faster** than `ClassModel.from_classfile()` (pure Python)
for workflows that need Python model objects.

### When Rust wins

Operations that stay **entirely in Rust** see the full speedup:
- `verify_classmodel()`: 11.9× faster than Python verification
- `bench-smoke` CLI: 19× end-to-end
- Any future Rust-only pipeline (transforms, analysis) benefits fully

### When Python is fine

The bridge cost means Rust doesn't help when the goal is to produce Python
`ClassModel` objects. The pure Python path is the right choice for:
- Interactive editing (model objects needed in Python)
- Python-side transforms (closures mutate Python models)
- Small workloads where parse time is negligible

## Reproducing

```bash
# Full comparison (5 iterations, ~3 minutes)
uv run python tools/compare_rust_python_benchmarks.py \
  --jar crates/pytecode-engine/fixtures/jars/byte-buddy-1.17.5.jar \
  --iterations 5 \
  --output output/benchmarks/rust-vs-python-byte-buddy.json

# Rust-only
cargo run --release -p pytecode-cli -- bench-smoke \
  --jar crates/pytecode-engine/fixtures/jars/byte-buddy-1.17.5.jar \
  --iterations 5

# Python-only
uv run python tools/benchmark_jar_pipeline.py \
  --jar crates/pytecode-engine/fixtures/jars/byte-buddy-1.17.5.jar \
  --iterations 5
```
