# Rust Migration Experiment — Overview

## Current Status

**Experiment in progress** on branch `rust-experiment`.

- **Phase 0 is complete.** maturin/PyO3 scaffolding is in place, `pytecode._rust` builds, public Python wrappers route through Rust-backed modules where implemented, and CI/release workflows install Rust.
- **Phase 1 is complete.** `src/binary_io.rs` and `src/classfile/modified_utf8.rs` exist, the Python-facing `BytesReader` / `BytesWriter` path now uses persistent Rust-backed wrappers instead of rebuilding Rust objects on every call, and `phase-1-benchmarks.md` captures that seam as the current Core Binary I/O baseline.
- **Phase 2 is complete.** Rust spec-model modules now exist for `constants`, `constant_pool`, `attributes`, `instructions`, and `descriptors`, and the public descriptor/signature helpers dispatch through the Rust backend while preserving the existing Python dataclasses and enums.
- **Phase 3 is complete.** Constant-pool parsing and the `Code`-attribute bytecode instruction stream now have Rust fast paths, and the Phase 3 benchmark rerun flipped both `class-parse` and `class-write` in Rust's favor while preserving the existing Python API.
- **Phase 4 is complete.** `pytecode.edit.constant_pool_builder.ConstantPoolBuilder` now prefers a Rust backend with lightweight checkpoint/rollback semantics, and the latest full `225.jar` rerun keeps Rust ahead of the Python fallback in all four measured pipeline stages.
- **Phase 5 is complete.** `pytecode.analysis.hierarchy` now dispatches through a Rust backend, `pytecode.analysis.verify` now uses a Rust-backed per-`CodeAttr` verifier seam, `tools\profile_jar_pipeline.py` exposes opt-in `analysis-resolver` / `analysis-frames` / `analysis-verify` stages, and `phase-5-benchmarks.md` captures the completed analysis-layer baseline plus the rejected frame-seam experiment.
- **Phase 6 is in progress.** The repo now pins stable Rust `1.94.1`, `pytecode.archive.JarFile.rewrite()` has a guarded Rust `zip` seam plus Python fallback when ZIP metadata would drift, `tools\profile_jar_pipeline.py` exposes an opt-in `archive-rewrite` stage, and `phase-6-benchmarks.md` records the current archive baseline.
- **Local validation is green** for: `cargo fmt --check`, `cargo clippy --all-targets --features extension-module -- -D warnings -A dead_code`, `uv run pytest -q`, `uv run ruff check .`, `uv run ruff format --check .`, `uv run basedpyright`, `uv run python tools\generate_api_docs.py --check`, `cargo test` (with the CPython DLL directory prepended to `PATH` on Windows), and `uv build`.
- **Current branch test count:** 1,691 passing pytest tests.

### Latest Benchmark Finding

The latest Phase 6 rerun measures a guarded `JarFile.rewrite()` seam through the
new `archive-rewrite` stage, again using `PYTECODE_BLOCK_RUST=1` to force the
same public API through the pure-Python fallback.

On the full `225.jar` archive rewrite rerun, Rust-enabled is effectively flat
but slightly slower: `0.519s` vs `0.513s` (**1.01x slower**).

On the two-JAR corpus archive rewrite rerun, the mean is also slower:
`0.685s` vs `0.653s` (**1.05x slower**). Per jar, `client-1.12.5.2.jar` lands
at `1.04x` slower and `injected-client-1.12.22.1.jar` lands at `1.08x`
slower.

**Current conclusion:** Phase 6 is not ready to be marked complete. The guarded
archive seam is valuable as a correctness-preserving baseline, but the current
metadata gate leaves too much work on the Python path to deliver a meaningful
throughput win.

### Immediate Next Step

The immediate next step is to **continue Phase 6** from the guarded archive
baseline while keeping the completed Phase 5 baselines as guardrails.

1. Use `phase5-analysis-*.json`, `phase5-verify-*.json`, and
   `phase5-pipeline-*.json` as the completed analysis/pipeline baseline.
2. Use `phase6-archive-*.json` as the current archive rewrite baseline.
3. Only widen the archive seam if exact ZIP metadata compatibility remains
   intact for `tests\test_jar.py`.
4. Keep the public Python API stable as archive/transforms work continues.

## Goal

Evaluate whether rewriting pytecode's core in Rust (via [maturin](https://www.maturin.rs/) + [PyO3](https://pyo3.rs/)) delivers meaningful throughput improvements while preserving the existing Python API. The experiment is structured so that at every phase we can measure gains and decide whether to continue, pivot, or abandon.

## Implementation Principle

This experiment should **not** be a line-by-line Python-to-Rust transliteration.

- The **external contract** is the existing Python API, behavior, and test suite.
- The **internal implementation** should be Rust-native: structs/enums, explicit error types, slice-based parsing, ownership/borrowing, and data layouts chosen for safety and performance.
- The PyO3 layer should stay **thin**. Python-shaped wrapper logic inside Rust should be minimized unless the public API requires it.

## Strategy

**Incremental, bottom-up migration.**

The library has a natural layered architecture (see `docs/architecture/target-architecture.md`). We migrate each layer from the bottom up, running the Python test suite at every phase boundary to ensure correctness and benchmarking at each performance-sensitive step.

```
Layer 7: Archive (JAR rewrite)
Layer 6: Transforms (pipeline DSL)
Layer 5: Analysis & Validation
Layer 4: Editing Model
Layer 3: Parser & Emitter
Layer 2: Spec Model (data types)
Layer 1: Core Binary I/O
Layer 0: Infrastructure
```

Rust code is compiled into a native extension module (`pytecode._rust`) via maturin. Thin Python wrappers in the existing `pytecode/` package re-export from `_rust`, preserving the public API surface.

## Phase Status

| Phase | Status | Notes |
|-------|--------|-------|
| 0 | Complete | Branch, maturin build, CI, baseline benchmarks, public wrapper routing for current Rust-backed modules |
| 1 | Complete | Persistent Rust-backed `BytesReader` / `BytesWriter` and Modified UTF-8 path landed, and `phase-1-benchmarks.md` records the remaining parse/write overhead as the current Core Binary I/O baseline |
| 2 | Complete | Rust spec-model datatypes landed for `constants`, `constant_pool`, `attributes`, `instructions`, and `descriptors`; Python descriptor/signature APIs now dispatch through the Rust backend |
| 3 | Complete | Constant-pool parsing and `Code`-attribute instruction parsing/writing now have Rust fast paths; the latest `phase3-*.json` rerun flips `class-parse` and `class-write` in Rust's favor on `225.jar` |
| 4 | Complete | Rust-backed `ConstantPoolBuilder` seam landed with lightweight checkpoint/rollback; `phase4-*.json` keeps Rust ahead on full `225.jar` while the focused sample still shows boundary overhead |
| 5 | Complete | Rust-backed hierarchy seam and per-`CodeAttr` verifier seam landed; accurate A/B reruns now show wins on `analysis-resolver`, `analysis-frames`, and `analysis-verify`, while the fine-grained frame wrapper remains rejected |
| 6 | In progress | Guarded `JarFile.rewrite()` seam landed, but `archive-rewrite` is still slower than fallback |
| 7 | Pending | Integration, benchmarking, go/no-go evaluation |
| 8 | Pending | Cleanup and release readiness |

## Success Criteria

At the later integration/evaluation phase, the experiment should satisfy:

1. **Correctness:** all current tests pass against the Rust backend.
2. **API compatibility:** no public Python API breakage.
3. **Performance:** meaningful throughput improvement on the JAR processing pipeline.
4. **Build viability:** maturin-based builds produce distributable wheels across supported platforms.
5. **Maintainability:** the Rust code stays structured, tested, and well documented.

## Tooling

- **Rust edition:** 2024
- **Pinned stable Rust toolchain:** 1.94.1 (`rust-toolchain.toml`)
- **PyO3:** 0.28.x with `extension-module`
- **Maturin:** 1.8+ as the PEP 517 build backend
- **CI:** GitHub Actions for `cargo test`, `cargo clippy`, `maturin develop`, and the Python validation stack
- **Windows note:** local `cargo test` may need the CPython 3.14 DLL directory prepended to `PATH`

## File Layout

```
pytecode/                    # Existing Python package (thin wrappers over _rust)
src/
├── lib.rs                   # PyO3 module root → pytecode._rust
├── binary_io.rs             # BytesReader / BytesWriter
├── classfile/
│   ├── mod.rs
│   ├── modified_utf8.rs     # JVM Modified UTF-8 codec
│   ├── constants.rs         # Access flags, enums, and spec constants
│   ├── constant_pool.rs     # CP entry types and constant-pool parsing seam
│   ├── attributes.rs        # Attribute/body model groundwork
│   ├── instructions.rs      # Opcode and operand model groundwork
│   ├── descriptors.rs       # Descriptor/signature parsing and Python bridging
│   ├── reader.rs            # (future) ClassReader
│   └── writer.rs            # (future) ClassWriter
├── edit/                    # (future) Editing model
├── analysis/                # (future) CFG, frames, hierarchy, verifier
├── archive.rs               # (future) JarFile
└── transforms.rs            # (future) Pipeline DSL
Cargo.toml                   # Rust crate configuration
pyproject.toml               # maturin build backend
```

## Next Session Handoff

1. Treat **Phase 4 as complete**: the Rust-backed `ConstantPoolBuilder` seam is landed, validated, and benchmarked in `output/profiles/phase4-*.json`.
2. Keep the constant-pool seam, Rust `Code`-attribute instruction-stream seam, and lightweight checkpoint/rollback design as the current parser/emitter/edit-model foundation.
3. Treat the Rust-backed hierarchy seam as the current Phase 5 baseline: `output/profiles/phase5-analysis-*.json` for analysis work and `output/profiles/phase5-pipeline-*.json` for pipeline regressions.
4. Preserve the existing Python API by reconstructing existing Python-facing structures at the boundary rather than changing public dataclasses.
5. Treat Phase 5 as complete: preserve the hierarchy and verifier seams, the `analysis-verify` benchmark stage, and the rejected frame-wrapper conclusion as the analysis/validation handoff.
6. Treat the guarded archive seam and `phase6-archive-*.json` as the current Phase 6 archive baseline, not as a completed phase win.
7. Continue Phase 6 from the current pipeline guardrail instead of reopening the rejected fine-grained frame seam.

## Related Documents

- [Baseline performance](baseline-performance.md)
- [Phase 1 benchmarks](phase-1-benchmarks.md)
- [Phase 3 benchmarks](phase-3-benchmarks.md)
- [Phase 4 benchmarks](phase-4-benchmarks.md)
- [Phase 5 benchmarks](phase-5-benchmarks.md)
- [Phase 6 benchmarks](phase-6-benchmarks.md)
