# Phase 6 Benchmarks

## Scope

Phase 6 currently targets `pytecode.archive.JarFile.rewrite()` rather than the
matcher DSL itself.

This first archive seam ships:

1. A guarded Rust `zip`-backed writer under `JarFile.rewrite()`.
2. A metadata-safety gate that keeps the existing Python `zipfile` path for
   archives whose rewrite metadata would drift.
3. An opt-in `archive-rewrite` stage in `tools\profile_jar_pipeline.py` so the
   seam can be benchmarked through the public API with the real
   `PYTECODE_BLOCK_RUST=1` fallback.

## Saved outputs

- `output\profiles\phase6-archive-225-rust.json`
- `output\profiles\phase6-archive-225-python.json`
- `output\profiles\phase6-archive-common-rust.json`
- `output\profiles\phase6-archive-common-python.json`

## Results

### Focused jar: `225.jar`

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|--------------|-----------------|-------|
| `archive-rewrite` | `0.519s` | `0.513s` | `1.01x` slower |

### Two-jar corpus mean

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|--------------|-----------------|-------|
| `archive-rewrite` | `0.685s` | `0.653s` | `1.05x` slower |

Per jar:

| Jar | Rust-enabled | Python fallback | Ratio |
|-----|--------------|-----------------|-------|
| `client-1.12.5.2.jar` | `0.976s` | `0.941s` | `1.04x` slower |
| `injected-client-1.12.22.1.jar` | `0.395s` | `0.364s` | `1.08x` slower |

## Interpretation

The guarded archive seam is now **behavior-safe** against the current rewrite
contract, but it is **not yet a throughput win** on the representative Phase 6
workloads.

The important result here is negative but useful:

1. Pinning Rust to stable `1.94.1` and adopting `zip` was viable.
2. A metadata-safe archive seam can be shipped without regressing
   `tests\test_jar.py`.
3. The current gate leaves too much work on the Python path, so the seam does
   not justify marking Phase 6 complete.

## Recommended next step

Keep the guarded archive seam and `phase6-archive-*.json` as the current Phase 6
baseline, then continue only with designs that widen exact-metadata coverage or
move the seam to a coarser archive/transform boundary.
