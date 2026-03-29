# pytecode

`pytecode` is a Python 3.14+ library for parsing, inspecting, and beginning to manipulate JVM class files, bytecode, and JAR files.

See [`docs/OVERVIEW.md`](docs/OVERVIEW.md) for a summary of the current parser + label-aware editing model and the roadmap toward a full classfile manipulation library.

## Current public API

- `pytecode.ClassReader` parses `.class` bytes eagerly into an `info.ClassFile` tree.
- `pytecode.JarFile` reads JARs, separates `.class` entries from non-class resources, and parses classes via `ClassReader`.
- `pytecode.ClassModel` provides the current mutable editing model with symbolic class, field, method, and label-aware code references.

For instruction-level editing helpers such as `Label`, `BranchInsn`, `LookupSwitchInsn`, `TableSwitchInsn`, `ExceptionHandler`, `LineNumberEntry`, `LocalVariableEntry`, `LocalVariableTypeEntry`, the `CodeItem` type alias, and `LabelResolution`, import directly from `pytecode.labels`.

For symbolic operand wrappers — `FieldInsn`, `MethodInsn`, `InterfaceMethodInsn`, `TypeInsn`, `VarInsn`, `IIncInsn`, `LdcInsn`, `InvokeDynamicInsn`, `MultiANewArrayInsn`, and the `LdcValue` union types — import from `pytecode.operands`. These wrappers are lifted automatically by `ClassModel.from_classfile()` and lowered automatically by `ClassModel.to_classfile()`.

For hierarchy-resolution helpers — `ClassResolver`, `MappingClassResolver`, `ResolvedClass`, `ResolvedMethod`, `InheritedMethod`, `iter_superclasses()`, `iter_supertypes()`, `is_subtype()`, `common_superclass()`, and `find_overridden_methods()` — import from `pytecode.hierarchy`. These helpers work with JVM internal class names and provide the hierarchy foundation for later control-flow and frame work.

For control-flow graph construction and stack/local simulation — `build_cfg()`, `simulate()`, `ControlFlowGraph`, `BasicBlock`, `SimulationResult`, `FrameState`, `initial_frame()`, and verification types (`VType`, `VTop`, `VInteger`, `VFloat`, `VLong`, `VDouble`, `VNull`, `VObject`, `VUninitializedThis`, `VUninitialized`) — import from `pytecode.analysis`. The analysis module operates on `CodeModel` and accepts an optional `ClassResolver` for reference-type merging at join points.

For descriptor and signature parsing — `parse_field_descriptor()`, `parse_method_descriptor()`, `parse_class_signature()`, `parse_method_signature()`, `parse_field_signature()`, `to_descriptor()`, `slot_size()`, `parameter_slot_count()`, `is_valid_field_descriptor()`, `is_valid_method_descriptor()`, and the structured types `BaseType`, `ObjectType`, `ArrayType`, `MethodDescriptor`, `ClassSignature`, `MethodSignature` — import from `pytecode.descriptors`.

For constant-pool construction and management — `ConstantPoolBuilder` with deduplication, symbol-table lookups, compound-entry auto-creation, and deterministic ordering — import from `pytecode.constant_pool_builder`.

For JVM Modified UTF-8 encoding and decoding of `CONSTANT_Utf8` values — `decode_modified_utf8()` and `encode_modified_utf8()` — import from `pytecode.modified_utf8`.

If you call `resolve_labels()` directly on code that contains single-slot `LdcInsn` values, pass the current `ConstantPoolBuilder` so `LDC` vs `LDC_W` sizing stays exact. `ClassModel.to_classfile()` and `lower_code()` handle that automatically.

## Requirements

- Python `3.14+`
- `uv` for development workflows

This repository keeps broad dependency ranges in `pyproject.toml` and commits `uv.lock` for reproducible development environments.

## Getting started

Create and sync a local environment with the development tools:

```powershell
uv sync --extra dev
```

Run the sample parser against the checked-in JAR:

```powershell
uv run python .\run.py .\225.jar
```

The script writes extracted output under `.\output\225\`.

## Development commands

Lint and check import ordering:

```powershell
uv run ruff check .
```

Verify formatting:

```powershell
uv run ruff format --check .
```

Run type checking:

```powershell
uv run basedpyright
```

Run the validation suite:

```powershell
uv run pytest -q
```

Run the JVM-backed CFG oracle suite only:

```powershell
uv run pytest -q -m oracle
```

The `oracle`-marked CFG tests lazily cache ASM 9.7.1 test jars under `.pytest_cache\pytecode-oracle` and also honor manually seeded jars in `tests\resources\oracle\lib`. If `java`, `javac`, or the ASM jars are unavailable, the oracle suite skips instead of failing the rest of the test run.

## Script validation

`run.py` is a manual smoke-test helper for the checked-in `225.jar` sample. Running it writes extracted output under `output\225` for inspection or comparison.

`tools\parse_wiki_instructions.py` supports deterministic generation from a local HTML file you provide:

```powershell
uv run python .\tools\parse_wiki_instructions.py --input-html .\path\to\wiki_instructions.html --output .\instruction_dump.txt
```

If `--input-html` is omitted, the script fetches the Wikipedia source page and prints the generated mapping to stdout.
