# Phase 5 Benchmarks

This document tracks the completed Phase 5 analysis/validation baseline: the
Rust-backed `pytecode.analysis.hierarchy` seam, the Rust-backed
`pytecode.analysis.verify` code-attribute verifier seam, and a repository-wide
`PYTECODE_BLOCK_RUST=1` import gate so Rust/Python A/B comparisons exercise the
same public API surface.

## Scope

- `src/analysis/mod.rs`
- `src/analysis/hierarchy.rs`
- `src/analysis/verify.rs`
- `pytecode.analysis.hierarchy`
- `pytecode.analysis.verify`
- `tools/profile_jar_pipeline.py`
- `tests/test_rust_phase5_hierarchy.py`
- `tests/test_rust_phase5_verify.py`

## Method

Two workload views were measured:

1. **Two-JAR corpus** on `client-1.12.5.2.jar` and
   `injected-client-1.12.22.1.jar`.
2. **Full `225.jar`** profile on the same benchmark stages.

Modes compared:

1. **Rust-enabled**: default imports with `pytecode._rust` available.
2. **Python fallback**: `PYTECODE_BLOCK_RUST=1` forces the same public API
   through the pure-Python implementations.

Three opt-in **analysis** stages were added to `tools/profile_jar_pipeline.py`:

- `analysis-resolver`: `MappingClassResolver.from_classfiles()`
- `analysis-frames`: `compute_frames()` over every code-bearing method in the
  selected jar(s), including `resolve_labels()` setup and `StackMapTable`
  generation
- `analysis-verify`: `verify_classfile()` over every parsed classfile

The existing four **pipeline** stages were rerun as a regression guardrail:

- `class-parse`: `ClassReader.from_bytes`
- `model-lift`: `ClassModel.from_classfile`
- `model-lower`: `ClassModel.to_classfile`
- `class-write`: `ClassWriter.write`

Saved outputs:

- `output/profiles/phase5-analysis-common-rust.json`
- `output/profiles/phase5-analysis-common-python.json`
- `output/profiles/phase5-analysis-225-rust.json`
- `output/profiles/phase5-analysis-225-python.json`
- `output/profiles/phase5-pipeline-common-rust.json`
- `output/profiles/phase5-pipeline-common-python.json`
- `output/profiles/phase5-pipeline-225-rust.json`
- `output/profiles/phase5-pipeline-225-python.json`
- `output/profiles/phase5-verify-common-rust.json`
- `output/profiles/phase5-verify-common-python.json`
- `output/profiles/phase5-verify-225-rust.json`
- `output/profiles/phase5-verify-225-python.json`

## Results

### Analysis stages

#### Two-JAR corpus

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| analysis-resolver | 1.419s | 2.257s | 0.63x |
| analysis-frames | 33.235s | 35.813s | 0.93x |
| analysis-verify | 6.944s | 9.748s | 0.71x |

Per-jar frame results:

- `client-1.12.5.2.jar`: `13.234s` Rust vs `18.109s` Python (**0.73x / faster**)
- `injected-client-1.12.22.1.jar`: `53.236s` Rust vs `53.517s` Python
  (**0.99x / effectively flat**)
- `client-1.12.5.2.jar` verifier: `4.392s` Rust vs `8.862s` Python
  (**0.50x / faster**)
- `injected-client-1.12.22.1.jar` verifier: `9.496s` Rust vs `10.634s` Python
  (**0.89x / faster**)

#### Full `225.jar`

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| analysis-resolver | 1.066s | 1.402s | 0.76x |
| analysis-frames | 46.887s | 50.104s | 0.94x |
| analysis-verify | 8.732s | 11.382s | 0.77x |

### Pipeline regression check

#### Two-JAR corpus

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| class-parse | 7.519s | 8.328s | 0.90x |
| model-lift | 9.311s | 10.059s | 0.93x |
| model-lower | 7.239s | 7.221s | 1.00x slower |
| class-write | 8.945s | 8.063s | 1.11x slower |

#### Full `225.jar`

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| class-parse | 5.315s | 6.893s | 0.77x |
| model-lift | 9.312s | 9.531s | 0.98x |
| model-lower | 8.161s | 7.552s | 1.08x slower |
| class-write | 5.118s | 5.984s | 0.86x |

Compared to the saved Phase 4 Rust full-jar baseline (`phase4-225-rust.json`),
the current Phase 5 rerun improved all four 225 pipeline stages:
`class-parse` (`5.507s` -> `5.315s`), `model-lift` (`10.084s` -> `9.312s`),
`model-lower` (`8.301s` -> `8.161s`), and `class-write` (`5.253s` -> `5.118s`).

### Rejected frame-seam attempt

Phase 5 also tried moving `merge_vtypes()` / frame-merge wrapper logic behind a
Rust seam. With the corrected `PYTECODE_BLOCK_RUST` gate in place, that
experiment regressed the representative frame workload (`225.jar`
`analysis-frames`: `52.338s` Rust vs `49.812s` Python), so the seam was backed
out instead of being carried forward as the new baseline.

## Interpretation

- The completed Phase 5 baseline now has two shipped Rust seams at the
  analysis/validation layer: hierarchy resolution and per-`CodeAttr`
  structural verification.
- Those shipped seams are **real wins** on both benchmark views:
  `analysis-resolver`, `analysis-frames`, and `analysis-verify` all improved on
  `225.jar` and on the two-JAR mean once the Python fallback is measured
  through the real import gate.
- The earlier small-corpus concern is now narrower: `client-1.12.5.2.jar`
  clearly improves, and even the previously touchier
  `injected-client-1.12.22.1.jar` now improves on verifier work while remaining
  effectively flat on frames.
- The attempted frame-merge seam is **not** the right next slice. The boundary
  cost of calling into Rust for `merge_vtypes()` / `_merge_frames()` outweighed
  the saved Python work, so that code was reverted.
- The pipeline guardrail is still mixed on the common corpus, but the full
  `225.jar` pipeline remains healthy and all four full-jar stages improved
  versus the saved Phase 4 Rust baseline.

## Recommended Next Step

Treat Phase 5 as complete and carry its saved baselines forward:

1. Treat `phase5-analysis-*.json` plus `phase5-verify-*.json` as the completed
   analysis/validation baseline.
2. Keep `phase5-pipeline-*.json` around as the regression guardrail for the
   broader pipeline.
3. Move on to Phase 6 work instead of reviving the rejected
   `merge_vtypes()` / `_merge_frames()` wrapper seam without a meaningfully
   coarser design.
