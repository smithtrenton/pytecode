# pytecode design

## What pytecode is

`pytecode` is a library for working directly with JVM `.class` files and JAR archives. It is not a Java source parser and it is not a JVM runtime. Its job is to:

- parse raw classfile bytes into a strongly typed spec-faithful tree,
- lift that tree into a safer mutable editing model with symbolic references,
- analyze and validate edited bytecode,
- lower edited models back to valid classfile bytes, and
- rewrite JARs while preserving non-class resources and ZIP metadata.

The library is designed for bytecode tooling: patchers, rewriters, validators, analysis tools, instrumentation passes, and archive transformation pipelines.

## What a reimplementation must preserve

A reimplementation in another language should preserve **behavioral contracts**, not Python-specific implementation choices.

The important contracts are:

1. **Top-level workflow**
   - parse `.class` bytes,
   - expose a raw parsed model,
   - expose a higher-level mutable model,
   - emit valid `.class` bytes,
   - rewrite JARs safely.
2. **Two-model architecture**
   - a raw spec model that mirrors the classfile format,
   - a symbolic editing model that hides constant-pool indexes and branch offsets where practical.
3. **Deterministic lowering**
   - emitted bytes are derived from live model state,
   - counts, lengths, offsets, and similar derived metadata are recomputed,
   - constant-pool allocation is deterministic.
4. **Safe bytecode editing semantics**
   - branch targets are labels, not raw offsets,
   - many constant-pool-backed instructions become symbolic wrappers,
   - callers do not manually manage WIDE encodings, switch padding, or jump widening.
5. **Verification and analysis support**
   - CFG construction, frame simulation, and structural verification are part of the public surface.
6. **Archive rewriting semantics**
   - non-class resources are preserved,
   - archive rewrites are safe/atomic,
   - signed-archive artifacts are preserved as files but modified archives are not re-signed automatically.

The following do **not** need to match exactly:

- Python dataclasses vs structs/classes/records in another language,
- Cython acceleration internals,
- private helper modules,
- exact internal module split,
- test-only utilities,
- byte-for-byte implementation details that are not part of public behavior.

## Core architecture

`pytecode` is organized around a read -> lift -> transform/analyze -> lower -> emit pipeline.

```text
.class bytes
  -> ClassReader
  -> ClassFile (raw spec tree)
  -> ClassModel (symbolic mutable model)
  -> transforms / analysis / verify / debug-info policy
  -> ClassFile
  -> ClassWriter
  -> .class bytes

JAR path
  -> JarFile
  -> parse classes + preserve other entries
  -> optional ClassModel transforms
  -> rewritten JAR
```

## Public top-level API

These four types are the primary entry points and must remain easy to discover.

| Export | Purpose | Main operations |
|---|---|---|
| `pytecode.ClassReader` | Parse `.class` bytes into a raw spec tree | constructor from bytes, `from_bytes`, `from_file`, `class_info` |
| `pytecode.ClassWriter` | Emit a raw spec tree back to bytes | `write(classfile)` |
| `pytecode.ClassModel` | Mutable symbolic editing model | `from_bytes`, `from_classfile`, `to_classfile`, `to_bytes` |
| `pytecode.JarFile` | Read, mutate, and rewrite JARs | `parse_classes`, `add_file`, `remove_file`, `rewrite` |

### Reader/writer contract

`ClassReader` eagerly parses the input class bytes and exposes a parsed `ClassFile` tree on `class_info`.

`ClassWriter.write()` serializes a `ClassFile` tree back to bytes in JVM classfile order and recomputes derived lengths/counts from the in-memory tree rather than trusting stale values.

### Editing-model contract

`ClassModel` is the main ergonomic API for callers who want to inspect or mutate classes.

Important characteristics:

- class names are exposed as JVM internal names like `java/lang/Object`,
- method and field descriptors remain JVM descriptor strings like `(I)V` or `Ljava/lang/String;`,
- constant-pool references are resolved into symbolic values where practical,
- method code becomes a `CodeModel`,
- non-Code method attributes stay in `MethodModel.attributes`,
- class/field/method collections are mutable.

Construction and lowering options that matter publicly:

| Option | Where | Meaning |
|---|---|---|
| `skip_debug=True` | lift phase | omit debug metadata when building `ClassModel` |
| `recompute_frames=True` | lower phase | recompute `max_stack`, `max_locals`, and `StackMapTable` |
| `resolver=...` | lower phase | supply class hierarchy information for frame computation |
| `debug_info="preserve"` or `debug_info="strip"` | lower phase | preserve or strip debug metadata during emission |

## The two public data models

### 1. Raw spec model

The raw model mirrors the JVM classfile format closely. It exists for fidelity, roundtripping, validation, and low-level tooling.

Main public raw-model modules:

| Module | Responsibility |
|---|---|
| `pytecode.classfile.info` | top-level `ClassFile`, `FieldInfo`, `MethodInfo` |
| `pytecode.classfile.constant_pool` | all constant-pool entry dataclasses |
| `pytecode.classfile.attributes` | all attribute and nested attribute dataclasses |
| `pytecode.classfile.instructions` | raw decoded bytecode instruction dataclasses |
| `pytecode.classfile.constants` | access flags, target enums, verification enums, magic constant |
| `pytecode.classfile.descriptors` | descriptor and generic-signature parsing/building |
| `pytecode.classfile.modified_utf8` | JVM Modified UTF-8 codec |

This layer should stay spec-faithful. It is allowed to be verbose because its purpose is exact representation of on-disk structures.

### 2. Symbolic editing model

The editing model exists to make bytecode mutation practical and safe.

Main public editing-model types:

| Type | Role |
|---|---|
| `ClassModel` | mutable class |
| `FieldModel` | mutable field |
| `MethodModel` | mutable method |
| `CodeModel` | mutable method code plus handlers/debug metadata |
| `ConstantPoolBuilder` | deterministic constant-pool allocator and resolver |

The editing model should preserve these ideas:

- symbolic class/member references instead of raw constant-pool indexes,
- labels for control-flow targets,
- debug tables bound to labels instead of offsets,
- helper wrappers for many operand kinds,
- explicit lowering back to the raw model.

## Label-based code editing

One of the key design choices is that code editing is **label-based**.

Public API in `pytecode.edit.labels`:

- `Label`
- `BranchInsn`
- `LookupSwitchInsn`
- `TableSwitchInsn`
- `ExceptionHandler`
- `LineNumberEntry`
- `LocalVariableEntry`
- `LocalVariableTypeEntry`
- `resolve_labels()`
- `lower_code()`

This is important because a reimplementation must preserve the same user experience:

- callers target labels instead of calculating branch offsets,
- exception handlers reference labels,
- line/local-variable debug entries reference labels,
- lowering computes final offsets, jump widening, and switch padding.

## Symbolic operand wrappers

Many bytecode instructions that normally point into the constant pool are exposed as symbolic wrapper types in `pytecode.edit.operands`.

Important wrapper families:

| Family | Examples | Why it matters |
|---|---|---|
| Member references | `FieldInsn`, `MethodInsn`, `InterfaceMethodInsn` | caller edits owner/name/descriptor symbolically |
| Type references | `TypeInsn`, `MultiANewArrayInsn` | caller edits class names symbolically |
| Local-variable ops | `VarInsn`, `IIncInsn` | caller edits slot numbers without dealing with short vs wide encodings |
| Constant loads | `LdcInsn`, `LdcInt`, `LdcString`, `LdcClass`, `LdcMethodHandle`, `LdcDynamic`, etc. | caller edits typed constant values, not raw CP indexes |
| Dynamic invocation | `InvokeDynamicInsn` | symbolic bootstrap/name/descriptor form |

The public contract is not that another language must use the same class names internally, but that users should get the same abstraction level.

## Constant-pool management

`pytecode.edit.constant_pool_builder.ConstantPoolBuilder` is part of the supported public API and is central to lowering.

Behavior to preserve:

- deterministic insertion order,
- deduplication of equivalent entries,
- support for all standard JVM constant-pool entry kinds,
- double-slot handling for `long`/`double`,
- import from an existing pool while preserving original indexes,
- export back to spec-shaped pool form,
- Modified UTF-8 handling for `Utf8`,
- value-based lookup helpers such as resolving UTF-8/class/name-and-type data.

The key design point is that callers and internal lowering code should not manually rebuild the constant pool.

## Transform pipeline DSL

`pytecode.transforms` is the main composition layer over `ClassModel`.

It exposes:

- transform protocols: `ClassTransform`, `FieldTransform`, `MethodTransform`, `CodeTransform`,
- a composable predicate wrapper: `Matcher`,
- pipeline composition: `Pipeline` and `pipeline(...)`,
- lifters: `on_classes`, `on_fields`, `on_methods`, `on_code`,
- matcher helpers for classes, fields, and methods.

### Transform semantics to preserve

1. Transforms mutate models **in place**.
2. Transforms are expected to return `None`.
3. Pipelines execute in deterministic order.
4. Field/method/code traversal uses a snapshot so callers can mutate collections during iteration without changing the current pass's visit set.
5. Matchers compose with boolean operators (`&`, `|`, `~`) and with helper combinators (`all_of`, `any_of`, `not_`).
6. Field and method transforms receive their owning `ClassModel`.
7. Code transforms receive both the owning `MethodModel` and `ClassModel`.

### Built-in matcher surface

The matcher/helper surface is intentionally broad and part of the public API. A compatible reimplementation should provide equivalent selectors for:

- class name, regex name, version, access flags, superclass, interfaces,
- field name, descriptor, access flags, and standard visibility/property predicates,
- method name, descriptor, return type, access flags, constructor/static-initializer tests, and `has_code`.

Representative examples:

- `class_named`, `class_name_matches`, `class_is_public`, `extends`, `implements`
- `field_named`, `field_descriptor`, `field_is_static`, `field_is_final`
- `method_named`, `method_name_matches`, `method_returns`, `method_is_public`, `method_is_static`, `is_constructor`, `has_code`

## Analysis layer

`pytecode.analysis` is a public advanced API, not just an implementation detail.

Its responsibilities:

- build a control-flow graph from `CodeModel`,
- model JVM verification types,
- simulate stack/local state through the CFG,
- compute `max_stack`, `max_locals`, and `StackMapTable`,
- expose structured analysis results and errors.

Main public concepts:

| Area | Public types/functions |
|---|---|
| errors | `AnalysisError`, `StackUnderflowError`, `InvalidLocalError`, `TypeMergeError` |
| verification types | `VType`, `VTop`, `VInteger`, `VFloat`, `VLong`, `VDouble`, `VNull`, `VObject`, `VUninitialized`, `VUninitializedThis` |
| CFG | `BasicBlock`, `ControlFlowGraph`, `ExceptionEdge`, `build_cfg` |
| simulation | `FrameState`, `SimulationResult`, `simulate`, `initial_frame` |
| frame computation | `FrameComputationResult`, `compute_frames`, `compute_maxs` |

This is important for another-language reimplementation because frame recomputation is not just a hidden feature; users can build analysis tooling directly on top of it.

## Hierarchy-resolution layer

`pytecode.analysis.hierarchy` supplies hierarchy-aware utilities used by frame computation and exposed directly to users.

Public responsibilities:

- resolve class metadata by internal name,
- walk superclasses and supertypes,
- answer subtype checks,
- compute common superclasses,
- identify overridden methods.

Main public types:

- `ResolvedClass`
- `ResolvedMethod`
- `InheritedMethod`
- `ClassResolver`
- `MappingClassResolver`
- `UnresolvedClassError`
- `HierarchyCycleError`
- `JAVA_LANG_OBJECT`

Design contract:

- ordinary analyses can treat `java/lang/Object` as the implicit root,
- missing required hierarchy data should surface as an error rather than be guessed silently.

## Validation API

`pytecode.analysis.verify` provides structural validation of both the raw and symbolic models.

Main public surface:

- `verify_classfile(classfile, fail_fast=False) -> list[Diagnostic]`
- `verify_classmodel(model, fail_fast=False) -> list[Diagnostic]`
- `Diagnostic`
- `Location`
- `Severity`
- `Category`
- `FailFastError`

Validation contract:

- default mode collects diagnostics rather than failing immediately,
- diagnostics are structured, not plain strings,
- diagnostics carry severity, category, message, and location context,
- `fail_fast=True` is an opt-in mode.

## Descriptor and signature utilities

`pytecode.classfile.descriptors` is a public cross-cutting utility module.

A compatible reimplementation should include:

- structured representation of field types, method descriptors, and generic signatures,
- parsers from descriptor/signature strings into structured types,
- conversion back to descriptor strings,
- slot-count helpers,
- validity helpers.

This module matters because multiple other surfaces depend on it:

- transforms inspect method returns and descriptors,
- frame computation needs slot sizes,
- validation checks descriptor well-formedness,
- operand wrappers and hierarchy logic depend on symbolic descriptors.

## Modified UTF-8

`pytecode.classfile.modified_utf8` is public and important because JVM classfiles use Modified UTF-8 for `CONSTANT_Utf8`, not normal UTF-8.

A compatible reimplementation must preserve:

- embedded NUL encoding rules,
- surrogate-pair handling for supplementary characters,
- malformed-sequence rejection,
- compatibility with constant-pool string handling.

## Debug-info policy support

`pytecode.edit.debug_info` exposes explicit debug-info behavior rather than treating it as accidental fallout from edits.

Important public ideas:

- `DebugInfoPolicy.PRESERVE`
- `DebugInfoPolicy.STRIP`
- `DebugInfoState.FRESH`
- `DebugInfoState.STALE`
- helpers to strip debug info,
- helpers to mark class/code debug info stale,
- stale debug info may be stripped automatically during lowering.

This is a real semantic contract: the library distinguishes between preserving debug metadata, intentionally stripping it, and recognizing that edits may have made it semantically stale.

## JAR support

`pytecode.archive` provides archive-level tooling rather than making callers unzip JARs manually.

`JarFile` behavior to preserve:

1. Reads the archive into memory.
2. Stores each entry as `JarInfo(filename, zipinfo, bytes)`.
3. Normalizes entry paths and rejects dangerous paths such as parent-directory traversal.
4. Splits class entries from non-class resources via `parse_classes()`.
5. Supports explicit `add_file()` and `remove_file()` mutation.
6. Rewrites archives safely, including in-place rewrite.
7. Preserves entry ordering and significant ZIP metadata when rewriting.
8. Preserves signature artifacts such as `.SF` and `.RSA` files as ordinary pass-through resources.
9. Does not re-sign modified archives.
10. Leaves the original file untouched if rewrite fails partway through.

### JAR rewrite workflow

`JarFile.rewrite(...)` is one of the most important externally visible workflows. A compatible reimplementation should support:

- destination path or in-place rewrite,
- optional class transform callback/pipeline,
- optional frame recomputation,
- optional hierarchy resolver,
- debug-info preservation/stripping policy,
- `skip_debug` lift mode for class entries.

## Real usage workflows

The repository's README and tests show four main usage patterns.

### 1. Parse and roundtrip a class

```python
from pytecode import ClassReader, ClassWriter

reader = ClassReader.from_file("HelloWorld.class")
classfile = reader.class_info
copy_bytes = ClassWriter.write(classfile)
```

### 2. Lift to editable form, mutate, and emit

```python
from pytecode import ClassModel

model = ClassModel.from_bytes(class_bytes)
model.name = "example/Renamed"
updated = model.to_bytes()
```

### 3. Recompute frames after code-shape changes

```python
updated = model.to_bytes(recompute_frames=True, resolver=resolver)
```

### 4. Rewrite a JAR with matchers and transform pipelines

```python
from pytecode import JarFile
from pytecode.transforms import on_methods, pipeline, method_is_public, method_is_static, method_name_matches, class_named

def make_final(method, owner):
    method.access_flags |= MethodAccessFlag.FINAL

JarFile("input.jar").rewrite(
    "output.jar",
    transform=pipeline(
        on_methods(
            make_final,
            where=method_name_matches(r"main") & method_is_public() & method_is_static(),
            owner=class_named("HelloWorld"),
        )
    ),
)
```

## Public module inventory

The repository treats the module list in `tools/generate_api_docs.py` as the authoritative documented surface. A reimplementation should cover equivalent areas:

- `pytecode`
- `pytecode.analysis`
- `pytecode.analysis.hierarchy`
- `pytecode.analysis.verify`
- `pytecode.archive`
- `pytecode.classfile.attributes`
- `pytecode.classfile.constant_pool`
- `pytecode.classfile.constants`
- `pytecode.classfile.descriptors`
- `pytecode.classfile.info`
- `pytecode.classfile.instructions`
- `pytecode.classfile.modified_utf8`
- `pytecode.classfile.reader`
- `pytecode.classfile.writer`
- `pytecode.edit.constant_pool_builder`
- `pytecode.edit.debug_info`
- `pytecode.edit.labels`
- `pytecode.edit.model`
- `pytecode.edit.operands`
- `pytecode.transforms`

Additionally, `pytecode.classfile` and `pytecode.edit` re-export useful subsets and are treated as public convenience modules.

## Public API families that matter most

Not every raw-model type deserves equal weight in another-language docs. The surfaces below matter most for compatibility:

1. **Top-level entry points**: `ClassReader`, `ClassWriter`, `ClassModel`, `JarFile`
2. **Editing model**: `ClassModel`, `FieldModel`, `MethodModel`, `CodeModel`
3. **Label editing**: `Label`, branch/switch wrappers, exception/debug label entries
4. **Operand wrappers**: field/method/type/var/LDC/invokedynamic/multianewarray symbolic wrappers
5. **Transforms DSL**: `Matcher`, `Pipeline`, `on_*`, matcher helpers
6. **Analysis**: CFG, frame state, frame computation, verification types
7. **Verification**: diagnostics, severity/category, raw-model and symbolic-model validation
8. **Hierarchy**: resolver protocol and common-superclass/subtype helpers
9. **Descriptors**: structured descriptor/signature parsing and formatting
10. **Archive rewriting**: `JarFile`, `JarInfo`, rewrite controls

## Internal details that can differ

The following are implementation details and do not need to be mirrored literally:

- Cython modules and accelerators,
- internal bytes reader/writer helpers,
- exact package-private helper names,
- exact storage layout of labels or constant-pool caches,
- exact class hierarchy of raw instruction helper records,
- test helper modules,
- Python typing syntax and overload structure.

## Suggested shape for another-language reimplementation

A good equivalent design would keep these layers:

1. **binary layer** for big-endian JVM reads/writes,
2. **raw classfile model** for exact parsing/emission,
3. **constant-pool builder** for deterministic symbolic lowering,
4. **symbolic editing model** for safe mutation,
5. **label and operand abstraction layer** for bytecode editing,
6. **transform DSL** for high-level bulk rewrites,
7. **analysis + hierarchy + verification layers** for correctness,
8. **archive layer** for JAR workflows.

Whether the target language uses classes, records, tagged unions, enums, traits, or interfaces is flexible. The important part is preserving the same conceptual boundaries and externally visible behavior.

## Bottom line

`pytecode` is best understood as a **JVM bytecode engineering toolkit** with four user-facing pillars:

1. raw classfile parsing/writing,
2. symbolic class/code editing,
3. analysis/verification/frame computation,
4. archive rewriting and transform pipelines.

Recreating it in another language means preserving those pillars and the contracts between them, not reproducing every private helper or Python-specific implementation detail.
