# Current architecture

The codebase is a Rust engine (`pytecode-engine`, `pytecode-archive`) with thin Python bindings via PyO3.

## Runtime and packaging

- Requires Python 3.14+ (`pyproject.toml`)
- Ships a `py.typed` marker so downstream type checkers can consume package types
- Keeps the core library runtime dependency-free
- Uses Ruff lint and format checks, basedpyright, and `pytest`-based validation in development (see `README.md`)

## Public entry points

- `pytecode.ClassReader` (Rust-backed via `pytecode._rust`)
  - `ClassReader.from_bytes()` / `ClassReader.from_file()` parse classfiles through the Rust parser
  - Produces `class_info`, a Rust-backed `ClassFile` spec model
- `pytecode.ClassWriter` (Rust-backed via `pytecode._rust`)
  - `ClassWriter.write(classfile)` serializes a Rust `ClassFile` back to bytes
- `pytecode.JarFile`
  - Reads the contents of a JAR
  - Separates `.class` entries from non-class resources
  - Parses classes via `ClassReader`
  - Supports explicit archive entry mutation via `add_file()` / `remove_file()`
  - Rewrites archives safely via `rewrite()`, delegating unchanged-on-disk archives with Rust-backed transforms to the Rust archive crate
  - Preserves signature artifacts as pass-through resources but does not re-sign modified archives
- `pytecode.ClassModel` (alias for `RustClassModel`, Rust-backed)
  - Mutable editing model for JVM class files
  - Uses symbolic (resolved) references instead of raw constant-pool indexes
  - Constructed from bytes via `ClassModel.from_bytes()` or from a parsed `ClassFile` via `ClassModel.from_classfile()`
  - Serializes directly via `to_bytes()`

These are the current public exports in `pytecode.__init__`.

Advanced transform-composition helpers live in `pytecode.transforms` rather than `pytecode.__init__`, keeping the top-level API small while still exposing a supported submodule for Rust-backed pipelines, matchers, and transforms.

## Module responsibilities

### `pytecode/_internal/bytes_utils.py`

Low-level big-endian binary I/O primitives for both reading and writing. This is the I/O foundation for class parsing and classfile emission.

**Read side**: Standalone `_read_u1/i1/u2/i2/u4/i4/_read_bytes` helper functions and a stateful `BytesReader` that tracks a cursor offset.

**Write side**: Standalone `_write_u1/i1/u2/i2/u4/i4/_write_bytes` helper functions and a stateful `BytesWriter` that appends to an internal buffer. `BytesWriter` provides `write_u1/i1/u2/i2/u4/i4/bytes` methods, `align(n)` for opcode-alignment padding, and a full set of `reserve_u1/i1/u2/i2/u4/i4` and `patch_u1/i1/u2/i2/u4/i4` methods for deferred length-prefixed structures.

### `pytecode/_rust` (Rust extension module)

`ClassReader` and `ClassWriter` are direct exports from the Rust extension module built via PyO3:
- `ClassReader.from_bytes(bytes)` and `ClassReader.from_file(path)` parse classfiles through the Rust parser and return a `ClassFile` spec model on the `class_info` property.
- `ClassWriter.write(classfile)` serializes a spec-model `ClassFile` back to bytes via Rust.
- `RustClassModel` provides the mutable editing model with symbolic references, constructed via `from_bytes()` or `from_classfile()`, serialized via `to_bytes()`.

These are the canonical implementations, not Python wrapper layers.

### `pytecode/classfile/modified_utf8.py`

Shared JVM Modified UTF-8 codec helpers for `CONSTANT_Utf8` values. This module
centralizes spec-correct encoding and decoding of:

- embedded NUL (`U+0000`) using the two-byte modified form
- supplementary characters via UTF-16 surrogate pairs
- malformed byte-sequence rejection (for example, illegal four-byte UTF-8 forms)

### `pytecode/classfile/attributes.py`

Typed dataclasses for classfile attributes and nested structures (verification types, stack map frames, annotations, type annotations, module info, record components, etc.). The Rust bindings instantiate these Python classes directly when materializing raw attribute metadata for the Python API surface.

### `pytecode/classfile/instructions.py`

Typed dataclasses for decoded bytecode instructions and operand shapes covering local indexes, constant-pool indexes, branches, switches, and other operand families. The Rust bindings instantiate these Python classes when materializing bytecode instructions.

### `pytecode/classfile/constants.py`

Enums and flags representing JVM constants, access flags, verification types, and target-type metadata. Used by both the Rust bindings and Python-side analysis helpers.

### Removed modules (now Rust-owned)

The following Python modules have been removed. Their functionality is now entirely owned by the Rust engine (`pytecode-engine`):

- `pytecode/edit/model.py` — `ClassModel`, `MethodModel`, `FieldModel`, `CodeModel` (replaced by `RustClassModel`)
- `pytecode/edit/labels.py` — label resolution and bytecode lowering
- `pytecode/edit/operands.py` — symbolic instruction wrappers
- `pytecode/edit/constant_pool_builder.py` — constant pool management
- `pytecode/classfile/reader.py` — classfile parsing (replaced by `ClassReader` in `_rust`)
- `pytecode/classfile/writer.py` — classfile serialization (replaced by `ClassWriter` in `_rust`)
- `pytecode/classfile/info.py` — `ClassFile`, `FieldInfo`, `MethodInfo` dataclasses (replaced by Rust `ClassFile`)
- `pytecode/classfile/constant_pool.py` — constant pool entry dataclasses
- `pytecode/classfile/descriptors.py` — descriptor and signature parsing

### `pytecode/transforms/__init__.py`

Composable transform helpers for JVM class manipulation:

- **Rust-first transform surface** — `pytecode.transforms.rust` is the canonical production API, centered on `RustPipelineBuilder`, Rust matcher factories, and Rust-backed transform factories that execute natively in Rust
- **Compatibility callback surface** — `Pipeline`, `pipeline()`, and the `on_*` lifting helpers remain available for callback-oriented extensions, but they are explicitly non-hot-path and operate on Rust-owned models

### `pytecode/analysis/hierarchy.py`

Hierarchy-resolution helpers, now primarily delegating to Rust-backed implementations:

- **Resolved snapshots** — `ResolvedClass` and `ResolvedMethod` frozen dataclasses for hierarchy-relevant class metadata, plus `InheritedMethod` for reporting matching inherited declarations
- **Pluggable interface** — `ClassResolver`, a minimal protocol that resolves an internal class name to a `ResolvedClass | None`
- **Built-in Rust-backed implementation** — `MappingClassResolver` (backed by `RustMappingClassResolver`) for in-memory hierarchy graphs, with `from_classfiles()` and `from_models()` convenience constructors
- **Query helpers** — `iter_superclasses()`, `iter_supertypes()`, `is_subtype()`, `common_superclass()`, and `find_overridden_methods()` — delegating to Rust when using a Rust resolver

### `pytecode/analysis/__init__.py`

Re-exports Rust-backed verification and hierarchy helpers. The analysis package provides:

- `verify_classfile()` and `verify_classmodel()` — thin wrappers around Rust verifier
- `MappingClassResolver` — Rust-backed hierarchy resolver
- `Diagnostic` — structured validation diagnostic type

### `pytecode/analysis/verify.py`

Thin facade over the Rust verifier. Entry points `verify_classfile()` and `verify_classmodel()` accept Rust-native classfiles, readers, and models, normalizing inputs before delegating to the Rust extension.

### `pytecode/analysis/hierarchy.py`

Hierarchy-resolution helpers, now primarily delegating to Rust-backed implementations:

- **Resolved snapshots** — `ResolvedClass` and `ResolvedMethod` frozen dataclasses for hierarchy-relevant class metadata, plus `InheritedMethod` for reporting matching inherited declarations
- **Pluggable interface** — `ClassResolver`, a minimal protocol that resolves an internal class name to a `ResolvedClass | None`
- **Built-in Rust-backed implementation** — `MappingClassResolver` (backed by `RustMappingClassResolver`) for in-memory hierarchy graphs, with `from_classfiles()` and `from_models()` convenience constructors
- **Query helpers** — `iter_superclasses()`, `iter_supertypes()`, `is_subtype()`, `common_superclass()`, and `find_overridden_methods()` — delegating to Rust when using a Rust resolver

### `pytecode/archive/__init__.py`

JAR container support. `JarFile` reads archive contents, separates `.class` entries from non-class resources, and supports safe rewrite workflows. `JarFile.rewrite()` delegates unchanged-on-disk archives with Rust-backed transforms to the Rust archive crate (`pytecode-archive`), keeping the hot rewrite loop in Rust. The Python fallback path handles in-memory archive edits and Python callback transforms.

### `run.py`

A repository smoke-test script for ad hoc inspection of real archives during development.

## Current data flow

### Class parsing

1. `ClassReader.from_bytes()` (Rust-backed) parses classfile bytes through the Rust engine.
2. The parsed result is a Rust-backed `ClassFile` spec model on `ClassReader.class_info`.
3. For editing, `ClassModel.from_bytes()` or `ClassModel.from_classfile()` produces a Rust-backed mutable model.

### JAR parsing

1. `JarFile` reads each ZIP entry into memory.
2. Entries ending in `.class` are parsed with `ClassReader` (Rust-backed).
3. Non-class resources are preserved as raw bytes.

### JAR rewriting

1. Callers optionally mutate the in-memory archive state with `JarFile.add_file()` / `remove_file()`.
2. `JarFile.rewrite()` delegates to the Rust archive crate for unchanged-on-disk archives with Rust transforms.
3. The Rust archive crate iterates entries, applies transforms, and writes the output archive.
4. After a successful rewrite, `JarFile` refreshes itself from disk so its in-memory state matches the written archive metadata.

## Design characteristics

### Strengths

- Spec-shaped datamodels make the parser output explicit and inspectable
- The parser retains structural detail rather than flattening everything into dictionaries
- Unknown attributes can still be carried as bytes
- Full opcode coverage with no gaps in the standard instruction set
- JAR-level parsing and rewrite support give the project a practical integration point
- The codebase is dependency-light and easy to run locally
- Enum-driven dispatch in both `ConstantPoolInfoType` and `AttributeInfoType` makes adding new types mechanical

### Current constraints

- Parsing is eager and tightly coupled to `ClassReader`
- Most parser behavior lives in one large module (`class_reader.py`), so parsing remains a maintenance hotspot even though emission now lives in `class_writer.py`
- The editing model now uses labels for control flow, exception ranges, and debug scopes, and symbolic operand wrappers for all major non-control-flow instruction families; only raw pass-through instructions (`BIPUSH`, `SIPUSH`, `NEWARRAY`, and zero-operand `InsnInfo`) remain in their spec-shaped form
- Signed-JAR artifacts are preserved as pass-through bytes during rewrite, but `pytecode` does not generate replacement signatures for modified archives
- Binary classfile emission, archive rewrite support, transform composition via `pytecode.transforms`, and the four validation tiers are implemented via `ClassWriter.write()`, `ClassModel.to_bytes()`, `JarFile.rewrite()`, the current pipeline/matcher helpers with context-passing transform protocols, and the validation suite

## Test coverage

The test suite provides both integration-level and unit-level coverage:

**Unit tests** ([#2](https://github.com/smithtrenton/pytecode/issues/2) — done):

- `test_attributes.py` — per-attribute-type parsing for the standard attribute families, stack map frame variants, verification types, annotation element values, type annotations, and the `UnimplementedAttr` fallback.
- `test_instructions.py` — instruction decoding for all operand shapes including no-operand, local variable index, constant-pool index, bipush/sipush, branch16/branch32, iinc, invokedynamic, invokeinterface, newarray, multianewarray, lookupswitch, tableswitch, and wide variants.
- `test_constant_pool.py` — all 17 constant-pool entry types, Long/Double double-slot handling, Modified UTF-8 `CONSTANT_Utf8` cases, mixed pool parsing, and unknown tag errors.
- `test_class_reader.py` — classfile parsing including magic number validation, version field validation, constant-pool indexing, access flags, interfaces, fields, methods, Code attributes, and error paths for invalid/truncated classfiles.
- `test_bytes_utils.py` — all primitive byte readers and writers (`_read_*`/`_write_*`), `BytesReader` stateful cursor, rewind, and buffer overrun, `BytesWriter` sequential writes, alignment padding, reserve/patch, and round-trip read↔write.
- `test_jar.py` — JAR reading, class/non-class separation, path normalization, explicit entry mutation, safe rewrite behavior, metadata/resource preservation, signed-artifact pass-through, atomic failure handling, and compiled JAR integration.
- `test_transforms.py` — pipeline ordering, matcher composition, regex/semantic/access helper coverage, owner-filtered lifting, snapshot traversal semantics, abstract/native/no-code method handling, runtime guardrails, and `JarFile.rewrite()` transform interop.
- `test_descriptors.py` — all 8 base types, object and array types, method descriptors, slot counting (long/double = 2 slots), round-trip parse → construct → parse, malformed descriptor error handling, generic class/method/field signatures with type parameters, wildcards (`+`/`-`/`*`), inner classes, type variables, throws clauses, and invalid internal-name edge cases.
- `test_constant_pool_builder.py` — builder deduplication, Modified UTF-8 handling, MethodHandle validation, import/export behavior, overflow guards, and defensive-copy semantics.
- `test_modified_utf8.py` — direct Modified UTF-8 codec coverage for NUL, supplementary characters, round-tripping, and malformed byte rejection.
- `test_model.py` — mutable editing model: from-scratch creation of `ClassModel`/`MethodModel`/`FieldModel`/`CodeModel`, `from_classfile()` symbolic resolution with error handling for malformed constant-pool references, `from_bytes()` convenience, round-trip `ClassFile → ClassModel → to_classfile()` equivalence across every compiled Java source fixture under `tests/resources/` (including multi-class outputs such as `Outer$Inner.class` and the helper/interface classes generated from `HierarchyFixture.java`), in-place mutation (add/remove fields and methods, rename class, change access flags), and ownership-boundary tests confirming the model does not share mutable state with the source or lowered `ClassFile`.
- `test_labels.py` — label/layout lowering coverage: offset resolution for linear, forward, backward, and multi-target branches; adjacent/terminal/dangling labels; duplicate label rejection; byte-size verification for every instruction subclass (including switch padding at offsets 0–3); automatic `GOTO_W`/`JSR_W` promotion for both forward and backward overflow; cascading promotion; all 16 conditional-branch inversions (parametrized); editing workflows showing offset recalculation after instruction insertion and removal; dynamic addition of exception handlers and debug entries; code-length boundary enforcement (65535 passes, 65536 raises); lifted exception/debug metadata reconstruction; and symbolic lifting from both manual raw `CodeAttr` fixtures and compiled control-flow bytecode
- `test_operands.py` — symbolic operand wrapper coverage: constructor validation (opcode rejection, JVM `u1`/`u2`/`i2` bounds, bootstrap-index validation, MethodHandle reference-kind validation), mapping-table sanity (roundtrips for `_IMPLICIT_VAR_SLOTS`/`_VAR_SHORTCUTS`/`_WIDE_TO_BASE`/`_BASE_TO_WIDE`), lifting tests for all 9 wrapper families (FieldInsn/MethodInsn/InterfaceMethodInsn/TypeInsn/LdcInsn/MultiANewArrayInsn/VarInsn/IIncInsn/InvokeDynamicInsn) from compiled `InstructionShowcase.java`, VarInsn normalisation (implicit → VarInsn, no raw implicit opcode survives lifting), LDC value-type discrimination (including `LdcMethodHandle` and `LdcDynamic` lowering coverage), lowering encoding-selection tests (implicit/explicit/WIDE for VarInsn; narrow/wide for IIncInsn; LDC/LDC_W/LDC2_W for LdcInsn based on CP index range; InterfaceMethodref vs Methodref for MethodInsn.is_interface; auto-computed count for InterfaceMethodInsn), mutation-time validation during lowering for mutable wrappers, edit-from-scratch tests (FieldInsn CP entry creation, deduplication of identical LdcInsn, mixed symbolic + raw instruction lists), and InstructionShowcase round-trip verification
- `test_hierarchy.py` — hierarchy-resolution coverage: adapters from parsed `ClassFile` and `ClassModel`, linear superclass walks with an implicit `java/lang/Object` root, supertype traversal through superclass and interface edges, subtype checks, common-superclass lookup, explicit missing-class and cycle failures, and method-override detection across same-package package-private methods, protected/public inheritance, interface methods, and non-overridable final/static/private declarations
- `test_helpers.py` — persistent Java fixture-cache coverage for `tests/helpers.py`, including cache hits across separate temp directories, invalidation when source contents change, and invalidation when `javac --release` changes.
- `test_analysis.py` — control-flow graph and simulation coverage: verification type helpers (`vtype_from_descriptor`, `is_category2`, `is_reference`, `merge_vtypes` with and without a resolver), `FrameState` operations (push/pop for category-1 and category-2 types, set_local/get_local with two-slot expansion, stack underflow detection), `initial_frame` for static methods, instance methods, `<init>`, and multi-parameter signatures (including long/double slots), CFG construction (single block, if/else branching, tableswitch, lookupswitch, try-catch exception edges, unconditional GOTO, loops, terminal ATHROW/return blocks), stack simulation for all major opcode families (constants, loads/stores, arithmetic, conversions, comparisons, stack manipulation including DUP/DUP_X1/DUP_X2/DUP2/DUP2_X1/DUP2_X2/SWAP/POP/POP2, field access, method invocations, object creation with NEW→`<init>` uninitialized tracking, type checks, array operations, monitors, IINC, LDC variants), max_stack/max_locals computation, type merging at branch join points, loop convergence via fixed-point iteration, exception-handler pre-instruction state, incompatible join-point merge failures, precise `AALOAD` reference typing, error paths (stack underflow, invalid locals), and integration tests against compiled `CfgFixture.java` methods covering all control-flow patterns.
- `test_verify.py` — structural validation coverage (122 tests): magic number, version range, constant-pool well-formedness, access flag mutual exclusions, class structure, field and method constraints, Code attribute validation (branches, exception handlers, CP reference validity), attribute versioning, descriptor validation, ClassModel label validity, `fail_fast` mode, and diagnostic severity/category/location accuracy.
- `test_validation.py` — multi-release validation-matrix tests: parametrizes the compiled Java fixture corpus across `--release 8, 11, 17, 21, 25`, filtered by each fixture's minimum supported release. For each (fixture, release) pair, it runs byte-for-byte roundtrip (T1), `verify_classfile()` + `javap` structural checks (T2), and JVM loading via `VerifierHarness.java` with `-Xverify:all` (T4), plus execution tests for selected roundtripped fixtures. The Tier 3 CP-aware semantic-diff engine lives in `tests/javap_parser.py` and is unit-tested in `test_javap_parser.py`.
- `test_javap_parser.py` — unit tests for the `javap` output parser and CP-aware semantic diff engine: parsing edge cases, member extraction, instruction operand resolution, and diff severity classification.

Test fixtures are generated from Java source in `tests/resources/` rather than relying on large binary artifacts. Helper utilities in `tests/helpers.py` compile focused fixtures and small JARs with `javac`, persist those outputs in a content-addressed cache under `.pytest_cache/pytecode-javac`, and only re-run `javac` when the ordered source list, source contents, `--release`, or `javac` identity changes. The `FIXTURE_MIN_RELEASES` mapping in `helpers.py` tracks which fixtures require `--release` > 8, and `list_java_resources(max_release=N)` filters accordingly.

Additional validation infrastructure modules:

- `tests/javap_parser.py` — structured parser for `javap -v -p -c` output and CP-aware semantic diff engine. Parses class headers, constant pool, fields, methods with code sections, and attributes into typed dataclasses. The semantic diff resolves all CP index references before comparison, ignoring index ordering differences, and classifies diffs as `error`, `warning`, or `info` severity.
- `tests/jvm_harness.py` — typed Python wrapper for `VerifierHarness.java`. Provides `verify_class()` and `execute_class()` entry points with `VerifyResult`/`ExecutionResult` dataclasses.
- `tests/validation_fixtures.py` — fixture registry mapping each fixture to its minimum release, plus `validation_matrix()` generator producing all valid `(fixture, release)` pairs.
- `tests/resources/VerifierHarness.java` — standalone Java program that reads `.class` bytes from a file, extracts the class name from the constant pool, defines the class via a custom `ClassLoader.defineClass()`, and reports structured JSON output. Run with `-Xverify:all` for strictest verification.
