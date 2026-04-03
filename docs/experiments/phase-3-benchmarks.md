# Phase 3 Benchmarks

This document tracks the Phase 3 parser/emitter experiment: first moving
constant-pool parsing into Rust, then moving the `Code`-attribute bytecode
instruction stream into Rust while keeping the public Python API unchanged.

**Status note:** the measurements below are the **post-code-seam Phase 3
completion benchmark**. The saved `phase2-complete-*` outputs remain the
pre-code-seam baseline for comparison.

## Scope

- `src/classfile/constant_pool.rs`
- `src/classfile/code.rs`
- `pytecode.classfile.reader.ClassReader`
- `pytecode.classfile.writer.ClassWriter`
- Benchmarks captured after the constant-pool Rust fast path landed and rerun
  again after the Rust `Code`-attribute bytecode seam landed

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

- `output/profiles/phase3-common-sample-rust.json`
- `output/profiles/phase3-common-sample-python.json`

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| class-parse | 0.682s | 0.511s | 1.34x slower |
| model-lift | 0.529s | 0.653s | 0.81x |
| model-lower | 0.414s | 0.412s | 1.00x |
| class-write | 0.763s | 0.314s | 2.43x slower |

Compared to the pre-code-seam baseline (`phase2-complete-common-sample-*`),
parse improved from `1.72x` slower to `1.34x` slower and write improved from
`2.75x` slower to `2.43x` slower. The small sample still shows fixed overhead,
but the seam clearly reduced both parser and emitter cost.

### Full `225.jar`

Saved to:

- `output/profiles/phase3-225-rust.json`
- `output/profiles/phase3-225-python.json`

| Stage | Rust-enabled | Python fallback | Ratio |
|-------|-------------:|----------------:|------:|
| class-parse | 4.866s | 7.349s | 0.66x |
| model-lift | 8.712s | 10.215s | 0.85x |
| model-lower | 7.629s | 7.746s | 0.98x |
| class-write | 5.470s | 6.357s | 0.86x |

Compared to the pre-code-seam baseline (`phase2-complete-225-*`), parse
improved from `1.25x` slower to `0.66x`, and write improved from `1.57x`
slower to `0.86x`. This is the first Phase 3 result that flips both
parser/emitter stages in Rust's favor on the representative full-jar workload.

## Hotspots

The pre-code-seam `225.jar` profiles showed that constant-pool parsing was
working but was not yet the dominant cost center; the bytecode instruction
loops around `read_code_bytes()`, `read_instruction()`, and
`_write_instruction()` were the real parser/emitter bottleneck. The post-code-
seam results above imply that moving those loops into Rust was enough to remove
that bottleneck in the representative full-jar case.

## Interpretation

- The Rust `Code`-attribute bytecode seam is the first parser/emitter slice
  that changes the direction of the experiment on a representative full jar:
  `class-parse` and `class-write` are now both faster than the Python fallback
  on `225.jar`.
- The focused sample remains slower on parse/write, which suggests there is
  still fixed Python/Rust boundary overhead on small workloads.
- The overall Phase 3 result is still strong enough to call the parser/emitter
  experiment successful: the larger full-jar run now shows a real throughput
  win while lift/lower stay favorable or neutral.

## Recommended Next Phase

This was the next-phase recommendation at the end of Phase 3. Phase 4 has since
been completed; see [Phase 4 benchmarks](phase-4-benchmarks.md) for the current
editing-model baseline and outcome.

At the end of Phase 3, the immediate next experiment was **Phase 4
editing-model work**, not more parser/emitter benchmarking:

1. Use the four `phase3-*.json` outputs above as the new parser/emitter
   baseline.
2. Start with the editing-model layer (`ConstantPoolBuilder`, labels, operands,
   and `ClassModel` mutation/lowering paths).
3. Preserve the current Python API by rebuilding existing Python-facing model
   objects at the boundary.
4. Revisit additional parser/emitter Rust seams only if later profiles reveal a
   clear remaining hotspot or a regression from this new baseline.

## Resume Guidance

- Treat `phase2-complete-*` as the pre-code-seam baseline and `phase3-*.json`
  as the completed Phase 3 parser/emitter baseline.
- Re-run both the focused sample and full `225.jar` views after substantial
  Phase 4 work so parse/write regressions remain visible.
- Prefer moving effort to the editing-model layer over reopening constant-pool
  or instruction-stream micro-optimizations unless later profiles justify it.
