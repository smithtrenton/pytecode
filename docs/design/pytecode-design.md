# pytecode design

## Purpose

`pytecode` is a library for reading, editing, validating, and rewriting JVM
classfiles and JAR archives. It is aimed at bytecode tooling such as patchers,
rewriters, validators, analysis tools, and transformation pipelines.

## Core workflows

The package supports four primary workflows:

1. Parse raw `.class` bytes into a typed classfile view
2. Materialize a mutable symbolic editing model
3. Analyze or validate the parsed or edited result
4. Rewrite `.class` files or JAR archives back to disk safely

## Public contracts

These top-level exports are the primary entry points:

| Export | Purpose | Main operations |
| --- | --- | --- |
| `pytecode.ClassReader` | Parse `.class` bytes into a raw classfile model | `from_bytes`, `from_file`, `class_info` |
| `pytecode.ClassWriter` | Emit raw classfile models back to bytes | `write(classfile)` |
| `pytecode.ClassModel` | Mutable symbolic editing model | `from_bytes`, `to_classfile`, `to_bytes`, `to_bytes_with_options` |
| `pytecode.JarFile` | Read, mutate, and rewrite JAR archives | `parse_classes`, `add_file`, `remove_file`, `rewrite` |

## Raw classfile surface

The raw surface mirrors the on-disk JVM format closely and exists for:

- exact parsing and inspection
- deterministic emission
- low-level validation
- tooling that needs direct access to classfile structures

The raw helper modules are:

- `pytecode.classfile`
- `pytecode.classfile.attributes`
- `pytecode.classfile.bytecode`
- `pytecode.classfile.constants`

## Mutable editing model

`ClassModel` is the main editing API.

Key properties:

- class names use JVM internal names such as `java/lang/Object`
- method and field descriptors remain JVM descriptor strings
- many constant-pool-backed instruction operands become symbolic objects
- method bodies are exposed through `CodeModel`
- code-related metadata is editable through typed wrappers

Public options that matter:

| Option | Where | Meaning |
| --- | --- | --- |
| `debug_info=DebugInfoPolicy.STRIP` | lower or archive-rewrite phase | strip debug metadata during emission or archive rewrite |
| `frame_mode=FrameComputationMode.RECOMPUTE` | lower phase | recompute `max_stack`, `max_locals`, and `StackMapTable` |
| `resolver=...` | lower phase | supply hierarchy information for frame computation |
| `debug_info="preserve"` / `"strip"` | lower phase | preserve or strip debug metadata during emission |

## Transform system

`pytecode.transforms` provides the supported transform surface:

- matcher factories for classes, fields, methods, and instructions
- built-in class and code transforms
- `PipelineBuilder` for declarative multi-step pipelines
- `Pipeline` and `CompiledPipeline` for repeated application

Built-in transforms execute through Rust-backed implementations. When a use case
needs custom logic, the same pipeline surface also accepts Python callbacks that
mutate a `ClassModel`.

## Analysis and validation

`pytecode.analysis` is part of the public package contract, not an internal
detail. The current public surface includes:

- hierarchy resolution via `MappingClassResolver`
- `verify_classfile()` and `verify_classmodel()`
- structured diagnostics through `Diagnostic`

## Archive rewrite semantics

`JarFile.rewrite()` is one of the main externally visible workflows. Its public
behavior is:

- preserve non-class resources
- preserve ZIP metadata for existing entries
- rewrite archives atomically
- preserve signature-related files as files, without re-signing modified output
- allow callers to choose transform, frame, resolver, and debug-info behavior

## Documentation and packaging contracts

The repository treats the generated API-doc manifest in
`tools/generate_api_docs.py` as the authoritative documented module surface.

The package also commits to:

- shipping typed Python metadata
- publishing wheels plus an sdist
- keeping README examples, module docs, and generated API docs aligned with the
  implementation
