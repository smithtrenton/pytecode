# Rust Migration Experiment — Overview

## Status

**Experiment in progress** — branch `rust-experiment`.

## Goal

Evaluate whether rewriting pytecode's core in Rust (via [maturin](https://www.maturin.rs/) + [PyO3](https://pyo3.rs/)) delivers meaningful throughput improvements while preserving the existing Python API. The experiment is structured so that at every phase we can measure gains and decide whether to continue, pivot, or abandon.

## Motivation

pytecode is a pure-Python library. Profiling shows that the performance-critical hot paths — binary I/O (`bytes_utils`), Modified UTF-8 codec, constant pool construction, class reader/writer, and label lowering — are CPU-bound Python loops over byte sequences. These are ideal candidates for Rust: tight loops, no GIL contention, and no external I/O.

A successful migration would:

- Significantly reduce parse-and-emit times for large JARs (hundreds/thousands of classes).
- Keep the Python API identical — users see only a faster library.
- Maintain the existing test suite as the acceptance criterion.

## Strategy

**Incremental, bottom-up migration.**

The library has a natural layered architecture (see `docs/architecture/target-architecture.md`). We migrate each layer from the bottom up, running the full Python test suite at every phase boundary to ensure correctness.

```
Layer 7: Archive (JAR rewrite)
Layer 6: Transforms (pipeline DSL)
Layer 5: Analysis & Validation
Layer 4: Editing Model
Layer 3: Parser & Emitter
Layer 2: Spec Model (data types)
Layer 1: Core Binary I/O
Layer 0: Infrastructure (this phase)
```

Rust code is compiled into a native extension module (`pytecode._rust`) via maturin. Thin Python wrappers in the existing `pytecode/` package re-export from `_rust`, preserving the public API surface.

## Phases

| Phase | Description | Key Deliverables |
|-------|-------------|-----------------|
| 0 | Infrastructure & scaffolding | Branch, maturin build, CI, baseline benchmarks |
| 1 | Core binary I/O | `BytesReader`/`BytesWriter`, Modified UTF-8 codec |
| 2 | Spec model (data types) | Constants, CP entries, attributes, instructions, descriptors |
| 3 | Parser & emitter | `ClassReader`, `ClassWriter` |
| 4 | Editing model | `ConstantPoolBuilder`, labels, operands, `ClassModel` |
| 5 | Analysis & validation | CFG, frame simulation, hierarchy, verifier |
| 6 | Archive & transforms | `JarFile`, pipeline DSL |
| 7 | Integration & performance | Full test suite, benchmarks, go/no-go evaluation |
| 8 | Cleanup & release readiness | Remove fallbacks, update docs/CI/release |

## Evaluation Criteria

At Phase 7, we evaluate the experiment against these criteria:

1. **Correctness**: All ~1,275 existing tests pass against the Rust backend.
2. **API compatibility**: No changes to the public Python API (`test_api_docs.py` passes).
3. **Performance**: Measurable improvement on JAR processing pipeline (target: ≥5× on parse+emit hot paths).
4. **Build complexity**: maturin-based builds produce distributable wheels for Linux, macOS, and Windows.
5. **Maintainability**: Rust codebase is well-structured, tested, and documented.

## Tooling

- **Rust edition**: 2024 (rustc 1.85+)
- **PyO3**: 0.24+ with `extension-module` feature
- **Maturin**: 1.8+ as the PEP 517 build backend
- **CI**: GitHub Actions for `cargo test`, `cargo clippy`, `maturin develop`, and the Python test suite

## File Layout

```
pytecode/                    # Existing Python package (thin wrappers over _rust)
src/
├── lib.rs                   # PyO3 module root → pytecode._rust
├── binary_io.rs             # BytesReader / BytesWriter
├── classfile/
│   ├── mod.rs
│   ├── modified_utf8.rs     # JVM Modified UTF-8 codec
│   ├── constants.rs         # (future) Access flags, enums
│   ├── constant_pool.rs     # (future) CP entry types
│   ├── attributes.rs        # (future) Attribute types
│   ├── instructions.rs      # (future) Opcode enum, operand types
│   ├── descriptors.rs       # (future) Descriptor/signature parsing
│   ├── reader.rs            # (future) ClassReader
│   └── writer.rs            # (future) ClassWriter
├── edit/                    # (future) Editing model
├── analysis/                # (future) CFG, frames, hierarchy, verifier
├── archive.rs               # (future) JarFile
└── transforms.rs            # (future) Pipeline DSL
Cargo.toml                   # Rust crate configuration
pyproject.toml               # Updated: maturin as build backend
```

## Related Documents

- [Phase 1 benchmarks](phase-1-benchmarks.md) *(to be written)*
- [Phase 3 benchmarks](phase-3-benchmarks.md) *(to be written)*
- [Performance results](performance-results.md) *(to be written)*
- [Evaluation](evaluation.md) *(to be written)*
- [Retrospective](rust-migration-retrospective.md) *(to be written)*
