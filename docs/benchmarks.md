# Benchmarks: Native Rust and Python Wrapper Overhead

Benchmark reporting that compares native Rust stage timings with the wrapper-inclusive
Python path across the same pipeline stages. Each stage is benchmarked in isolation
with fresh setup.

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

### Native Rust path: 19× lower end-to-end overhead

All five stages show Python/Rust median ratios ranging from 1.2× (I/O-bound
jar-read) to 49× (class-parse). The compute-heavy stages — parse, lift, lower,
write — show the largest wrapper overhead because they involve intensive data
structure construction that benefits from Rust's compiled execution and reduced
cross-language materialization.

### Bridge overhead used to dominate Python model lifting

These numbers were captured before the later bridge cleanup phases. At that
time, Rust parse + lift stayed in Rust for ~500ms, but materializing full Python
dataclass models through the PyO3 bridge still cost ~19s for 5,928 classes.
The current codebase now single-parses `ClassModel.from_bytes()` through Rust and
uses Rust-backed serialization for clean roundtrips and normal code-mutation
emission, so this document should be read as historical benchmark context rather
than the exact current `ClassModel` cost model.

### When wrapper overhead stays low

Operations that stay mostly inside Rust keep Python-layer cost close to the
native baseline.

### When native Rust wins outright

Operations that stay **entirely in Rust** see the full speedup:
- `verify_classmodel()`: 11.9× lower median cost than the wrapper-inclusive path
- `bench-smoke` CLI: 19× lower end-to-end median cost
- Any future Rust-only pipeline (transforms, analysis) benefits fully

### Where Python still matters

Python still owns the legacy high-level editing DSL: symbolic labels, operand
wrappers, debug-info helpers, and user-defined transform callbacks all operate
on Python objects. The remaining compatibility cost is therefore concentrated in
legacy Python model materialization and callback-oriented interop, not in the
core Rust parse/write engine anymore.

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
