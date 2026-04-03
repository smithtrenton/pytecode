# Phase 4 Benchmarks

This document tracks the Phase 4 editing-model experiment: moving the first
shared edit-model dependency, `ConstantPoolBuilder`, behind a Rust-backed seam
while preserving the existing Python API.

## Scope

- `src/edit/constant_pool_builder.rs`
- `pytecode.edit.constant_pool_builder`
- `pytecode.edit.model`
- `pytecode.edit.labels`
- `tests/test_rust_phase4_constant_pool_builder.py`

## Method

Two views were measured:

1. **Focused A/B sample** on `client-1.12.5.2.jar` and
   `injected-client-1.12.22.1.jar`, sampling 60 classes per JAR.
2. **Full `225.jar` stage profile** across the same four pipeline stages.

Modes compared:

1. **Rust-enabled**: default imports with `pytecode._rust` available.
2. **Python fallback**: an import-blocking shim prevents `pytecode._rust`
   imports so the same public API uses pure-Python implementations.

Stages measured:

- `class-parse`: `ClassReader.from_bytes`
- `model-lift`: `ClassModel.from_classfile`
- `model-lower`: `ClassModel.to_classfile`
- `class-write`: `ClassWriter.write`

## Results

### Focused two-JAR sample

Saved to:

- `output/profiles/phase4-common-sample-rust.json`
- `output/profiles/phase4-common-sample-python.json`

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| class-parse | 0.807s | 0.603s | 1.34x slower |
| model-lift | 1.006s | 0.692s | 1.45x slower |
| model-lower | 0.671s | 0.615s | 1.09x slower |
| class-write | 0.878s | 0.376s | 2.33x slower |

Compared to the Phase 3 sample baseline (`phase3-common-sample-*`), the Phase 4
editing-model seam regressed the saved Rust timings across all four stages. The
small sample still magnifies Python/Rust boundary overhead.

### Full `225.jar`

Saved to:

- `output/profiles/phase4-225-rust.json`
- `output/profiles/phase4-225-python.json`

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| class-parse | 5.507s | 7.249s | 0.76x |
| model-lift | 10.084s | 10.194s | 0.99x |
| model-lower | 8.301s | 8.375s | 0.99x |
| class-write | 5.253s | 6.411s | 0.82x |

Compared to the Phase 3 full-jar Rust baseline (`phase3-225-rust.json`), Phase
4 regressed `class-parse` (`4.866s` -> `5.507s`), `model-lift` (`8.712s` ->
`10.084s`), and `model-lower` (`7.629s` -> `8.301s`), while slightly improving
`class-write` (`5.470s` -> `5.253s`).

## Hotspots

The first Phase 4 implementation introduced a severe `model-lower` regression
because Rust `checkpoint()` / `rollback()` copied the whole imported
constant-pool object graph on the label-lowering hot path. Reworking that seam
to use a lightweight checkpoint plus on-demand index rebuilding removed the
catastrophic regression and restored a net full-jar win over the Python
fallback.

The remaining edit-model costs are still boundary-heavy. The targeted reruns
show `ConstantPoolBuilder.from_pool()` and `resolve_utf8()` as the main Rust
lift overhead, while lowering remains dominated by Python-side label/code
assembly rather than by constant-pool allocation alone.

## Interpretation

- Phase 4 is successful enough to keep: on the representative `225.jar`
  workload, Rust remains faster than the Python fallback in all four stages.
- The editing-model win is modest. `model-lift` and `model-lower` only edge out
  the fallback on the full jar, and the focused sample loses across the board.
- Phase 4 did **not** improve on the saved Phase 3 Rust baseline. It should be
  treated as a correctness-preserving edit-model seam with a representative
  full-jar win, not as a clean throughput jump.

## Recommended Next Phase

The next experiment should move to **Phase 5 analysis/hierarchy work** while
preserving the current Phase 4 baseline:

1. Use `phase4-*.json` as the current benchmark reference.
2. Keep checking both the focused sample and full `225.jar` views.
3. Revisit edit-model boundary reduction only if later profiles keep pointing at
   `from_pool()` / `resolve_utf8()` or if a later phase turns the current slight
   full-jar win back into a regression.
