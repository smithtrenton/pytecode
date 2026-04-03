# Phase 1 Benchmarks

This document tracks the Phase 1 binary I/O experiment for the Rust migration. The focus here is the low-level `BytesReader` / `BytesWriter` path and the Modified UTF-8 codec exposed through the existing Python API.

## Scope

- `pytecode._internal.bytes_utils`
- `pytecode.classfile.modified_utf8`
- Sample corpus:
  - `client-1.12.5.2.jar`
  - `injected-client-1.12.22.1.jar`
- Sampling strategy: 60 classes per JAR, 120 classes total

## Method

The comparison uses the same branch in two modes:

1. **Rust-enabled**: default imports, so public Python wrappers delegate to `pytecode._rust`.
2. **Python fallback**: block `pytecode._rust` imports with `PYTECODE_BLOCK_RUST=1` so the same code path uses the pure-Python fallback implementations.

Stages measured:

- `class-parse`: `ClassReader.from_bytes`
- `model-lift`: `ClassModel.from_classfile`
- `model-lower`: `ClassModel.to_classfile`
- `class-write`: `ClassWriter.write`

## Results

### Original Phase 0 wrapper design

The first wrapper implementation reconstructed Rust reader/writer objects on every method call.

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| class-parse | 2.189s | 0.279s | 7.83x slower |
| model-lift | 0.261s | 0.347s | 0.75x |
| model-lower | 0.238s | 0.238s | 1.00x |
| class-write | 2.275s | 0.180s | 12.62x slower |

## Persistent-wrapper redesign

Phase 1 replaced the per-call shim with persistent Rust-backed `BytesReader` / `BytesWriter` wrappers and changed the Rust API to return Python `bytes` directly where appropriate.

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| class-parse | 0.403s | 0.236s | 1.71x slower |
| model-lift | 0.239s | 0.295s | 0.81x |
| model-lower | 0.209s | 0.209s | 1.00x |
| class-write | 0.412s | 0.153s | 2.70x slower |

## Interpretation

- The persistent-wrapper design removed most of the pathological overhead from the original shim.
- The remaining parse/write slowdown suggests that crossing the Python/Rust boundary for each primitive read/write is still too expensive.
- `model-lift` already trends in Rust's favor, which is a useful signal for later phases.
- The likely next real performance step is **Phase 3**, where `ClassReader` and `ClassWriter` can be made Rust-native instead of layering Rust primitives under Python parser/emitter loops.

## Resume Guidance

- Treat the numbers in this document as the current Phase 1 baseline.
- If Phase 1 continues, rerun the same sampled two-JAR A/B comparison after each wrapper change so regression/improvement is measurable.
- If the goal is a larger throughput win, prefer moving effort to Rust-native `ClassReader` / `ClassWriter` work in Phase 3 over further micro-optimizing per-primitive Python↔Rust calls.

## Validation State

At the time of this benchmark, the branch is green for:

- `cargo test`
- `uv run maturin develop`
- `uv run ruff check .`
- `uv run basedpyright`
- `uv run pytest -q`
- `uv run python tools\generate_api_docs.py --check`
- `uv build`
