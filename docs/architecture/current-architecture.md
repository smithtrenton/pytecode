# Current architecture

`pytecode` is a Rust-backed Python package built from three main workspace
layers:

| Component | Responsibility |
| --- | --- |
| `pytecode-engine` | Raw classfile parsing/writing, mutable class model, transforms, analysis, and validation |
| `pytecode-archive` | JAR read/mutate/rewrite support on top of the engine |
| `pytecode-python` | PyO3 bindings that expose the Rust engine to the Python package |

## Runtime and packaging

- The published package targets Python 3.12+.
- Wheels are built for the supported platforms, with an sdist alongside them.
- The Python package ships type information (`py.typed` and `.pyi` stubs).
- The development validation set uses Ruff, basedpyright, pytest, generated API
  docs checks, and the Rust workspace checks described in `README.md`.

## Public Python surface

Top-level exports in `pytecode`:

- `ClassReader`
- `ClassWriter`
- `ClassModel`
- `JarFile`

Supported public submodules:

- `pytecode.classfile`
- `pytecode.classfile.attributes`
- `pytecode.classfile.bytecode`
- `pytecode.classfile.constants`
- `pytecode.model`
- `pytecode.transforms`
- `pytecode.analysis`
- `pytecode.analysis.verify`
- `pytecode.analysis.hierarchy`
- `pytecode.archive`

## Module responsibilities

### `pytecode._rust`

The compiled extension module exposes the Rust-owned runtime types used by the
public Python package: raw classfile objects, mutable model objects, transforms,
analysis helpers, archive helpers, and diagnostics.

### `pytecode.classfile`

This package is the raw classfile-facing surface:

- `ClassReader` and `ClassWriter` are the primary parse/write entry points
- `ClassFile`, `InsnInfo`, `ExceptionInfo`, and related types expose parsed
  classfile data
- `attributes`, `bytecode`, and `constants` provide typed helper data for raw
  inspection workflows

### `pytecode.model`

This module exposes the mutable symbolic editing layer:

- `ClassModel`, `FieldModel`, `MethodModel`, and `CodeModel`
- labels, exception handlers, line-number entries, and local-variable metadata
- typed code-item wrappers for symbolic bytecode editing

### `pytecode.transforms`

This package exposes the declarative transform system:

- matcher factories
- class and code transform factories
- `PipelineBuilder`, `Pipeline`, and `CompiledPipeline`

Built-in transforms execute through the Rust engine. Custom Python callbacks are
supported on the same public surface when a workflow needs Python-owned logic.

### `pytecode.analysis`

This package exposes Rust-backed validation and hierarchy helpers:

- `verify_classfile()` and `verify_classmodel()`
- `Diagnostic`
- `MappingClassResolver`
- hierarchy traversal and override-query helpers

### `pytecode.archive`

`JarFile` provides archive reading, in-memory entry mutation, class parsing, and
safe rewrite-to-disk behavior. Archive rewrites preserve non-class resources and
ZIP metadata, keep signed artifacts as files, and do not attempt to re-sign
modified archives.

## Data flow

### Parse and inspect a class

1. `ClassReader.from_bytes()` or `ClassReader.from_file()` parses bytes through
   the Rust engine.
2. `reader.class_info` exposes the raw classfile view.
3. Callers can inspect the raw model directly or materialize a mutable
   `ClassModel`.

### Edit and emit a class

1. `ClassModel.from_bytes()` creates a mutable symbolic model.
2. Callers mutate fields, methods, code, and debug-info settings on the model.
3. `to_bytes()` or `to_bytes_with_options()` lowers the model back to classfile
   bytes.

### Rewrite an archive

1. `JarFile` reads the current archive state into memory.
2. Callers optionally add or remove entries before rewrite.
3. `JarFile.rewrite()` writes the new archive atomically, applying transforms to
   matching class entries when requested.
4. After success, the in-memory `JarFile` state is refreshed from disk.

## Operational characteristics

- Deterministic lowering: derived sizes, offsets, and related metadata are
  recomputed from live model state.
- Explicit frame and debug-info controls: callers choose whether to preserve or
  recompute frames and whether to preserve or strip debug metadata.
- Typed public surface: raw classfile helpers and mutable-model helpers are
  exposed as concrete Python-visible types rather than ad hoc dictionaries.
- Atomic archive writes: rewrite operations replace the destination only after a
  successful write.

## Validation coverage

The repository keeps the documented surface aligned with implementation through
the Rust workspace tests plus the Python-facing suite:

- `crates/pytecode-engine/tests/raw_roundtrip.rs`
- `crates/pytecode-engine/tests/verifier.rs`
- `crates/pytecode-engine/tests/analysis.rs`
- `crates/pytecode-engine/tests/model.rs`
- `crates/pytecode-engine/tests/transform.rs`
- `crates/pytecode-archive/tests/jar.rs`
- `crates/pytecode-cli/tests/*.rs`
- `tests/test_rust_bindings.py`
- `tests/test_rust_transforms.py`
- `tests/test_jar.py`
- `tests/test_javap_parser.py`
- `tests/test_api_docs.py`
- `tools/generate_api_docs.py --check`
