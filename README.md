# pytecode

`pytecode` is a Python 3.14+ library for parsing, inspecting, manipulating, and emitting JVM class files, bytecode, and JAR files.

See [`docs/OVERVIEW.md`](docs/OVERVIEW.md) for a summary of the current classfile toolkit and the remaining roadmap work.

The hosted API reference is published to GitHub Pages at <https://smithtrenton.github.io/pytecode/>. For local work, `uv run python tools\generate_api_docs.py` writes HTML into the ignored `docs\api\` directory instead of committing generated files to the repository.

## Current public API

- `pytecode.ClassReader` parses `.class` bytes eagerly into an `info.ClassFile` tree.
- `pytecode.ClassWriter` serializes an `info.ClassFile` tree back to `.class` bytes.
- `pytecode.JarFile` reads JARs, separates `.class` entries from non-class resources, parses classes via `ClassReader`, and can add/remove entries plus rewrite archives safely.
- `pytecode.ClassModel` provides the current mutable editing model with symbolic class, field, method, and label-aware code references. Use `ClassModel.to_bytes()` for a lowering-plus-emission convenience path. `ClassModel.from_classfile()` and `ClassModel.from_bytes()` also accept `skip_debug=True` when you want an ASM-like lift path that omits debug metadata before it enters the mutable model.

For composable in-place transformations, import from `pytecode.transforms`. That module now provides callable `Pipeline` objects, `Matcher` predicates with `&` / `|` / `~` composition, `on_classes()` / `on_fields()` / `on_methods()` / `on_code()` lifting helpers, optional owner-class filtering on the field/method/code lifting helpers, name/descriptor/access matchers, regex matchers, and lightweight structural helpers for direct superclasses/interfaces, class versions, special method names, and return descriptors, plus the original `all_of()`, `any_of()`, and `not_()` combinators for callers that prefer functional composition. The `FieldTransform`, `MethodTransform`, and `CodeTransform` protocols pass owning context (`ClassModel` for field/method transforms; `MethodModel` and `ClassModel` for code transforms) so transforms can inspect their position in the class hierarchy. Pipelines remain callable and can be passed directly to `JarFile.rewrite(transform=...)`.

For instruction-level editing helpers such as `Label`, `BranchInsn`, `LookupSwitchInsn`, `TableSwitchInsn`, `ExceptionHandler`, `LineNumberEntry`, `LocalVariableEntry`, `LocalVariableTypeEntry`, the `CodeItem` type alias, `LabelResolution`, and `lower_code()`, import directly from `pytecode.labels`.

For debug-info helpers — `DebugInfoPolicy`, `DebugInfoState`, `apply_debug_info_policy()`, `strip_debug_info()`, `mark_class_debug_info_stale()`, and `mark_code_debug_info_stale()` — import from `pytecode.debug_info`. `ClassModel.to_classfile()`, `ClassModel.to_bytes()`, and `lower_code()` preserve lifted debug metadata by default and also accept `debug_info="strip"` (or `DebugInfoPolicy.STRIP`) when you want to omit `LineNumberTable`, `LocalVariableTable`, `LocalVariableTypeTable`, `SourceFile`, and `SourceDebugExtension` metadata from output. Explicitly stale class/code debug metadata is also stripped automatically during lowering, and `verify_classmodel()` warns before emission when that stale state is present.

For symbolic operand wrappers — `FieldInsn`, `MethodInsn`, `InterfaceMethodInsn`, `TypeInsn`, `VarInsn`, `IIncInsn`, `LdcInsn`, `InvokeDynamicInsn`, `MultiANewArrayInsn`, and the `LdcValue` union types — import from `pytecode.operands`. These wrappers are lifted automatically by `ClassModel.from_classfile()` and lowered automatically by `ClassModel.to_classfile()`.

For hierarchy-resolution helpers — `ClassResolver`, `MappingClassResolver`, `ResolvedClass`, `ResolvedMethod`, `InheritedMethod`, `iter_superclasses()`, `iter_supertypes()`, `is_subtype()`, `common_superclass()`, and `find_overridden_methods()` — import from `pytecode.hierarchy`. These helpers work with JVM internal class names, back the current control-flow/simulation layer, and support the current frame-recomputation and validation pipeline.

For control-flow graph construction and stack/local simulation — `build_cfg()`, `simulate()`, `ControlFlowGraph`, `BasicBlock`, `ExceptionEdge`, `SimulationResult`, `FrameState`, `initial_frame()`, helper functions `vtype_from_descriptor()`, `merge_vtypes()`, `is_category2()`, `is_reference()`, and verification types (`VType`, `VTop`, `VInteger`, `VFloat`, `VLong`, `VDouble`, `VNull`, `VObject`, `VUninitializedThis`, `VUninitialized`) — import from `pytecode.analysis`. The analysis module also provides `compute_maxs()` and `compute_frames()` for recomputing `max_stack`/`max_locals` and generating `StackMapTable` entries after bytecode editing. The module operates on `CodeModel` and accepts an optional `ClassResolver` for reference-type merging at join points.

For structural classfile validation — `verify_classfile()`, `verify_classmodel()`, `Diagnostic`, `Severity`, `Category`, `Location`, and `FailFastError` — import from `pytecode.verify`. The validation module checks magic number, version, constant-pool well-formedness, access flags, class structure, field/method constraints, Code attributes, attribute versioning, descriptors, and ClassModel label validity. Both entry points collect all diagnostics by default; pass `fail_fast=True` to raise on the first error.

For descriptor and signature parsing — `parse_field_descriptor()`, `parse_method_descriptor()`, `parse_class_signature()`, `parse_method_signature()`, `parse_field_signature()`, `to_descriptor()`, `slot_size()`, `parameter_slot_count()`, `is_valid_field_descriptor()`, `is_valid_method_descriptor()`, and key structured types such as `BaseType`, `VoidType`, `ObjectType`, `ArrayType`, `MethodDescriptor`, `ClassSignature`, and `MethodSignature` — import from `pytecode.descriptors`.

For constant-pool construction and management — `ConstantPoolBuilder` with deduplication, symbol-table lookups, compound-entry auto-creation, and deterministic ordering — import from `pytecode.constant_pool_builder`.

For JVM Modified UTF-8 encoding and decoding of `CONSTANT_Utf8` values — `decode_modified_utf8()` and `encode_modified_utf8()` — import from `pytecode.modified_utf8`.

If you call `resolve_labels()` directly on code that contains single-slot `LdcInsn` values, pass the current `ConstantPoolBuilder` so `LDC` vs `LDC_W` sizing stays exact. `ClassModel.to_classfile()` and `lower_code()` handle that automatically.

For direct classfile emission, call `ClassWriter.write(classfile)`. `ClassModel.to_bytes()` is a thin convenience wrapper over `to_classfile()` plus `ClassWriter.write()`.

For archive-level edits, use `JarFile.add_file()`, `JarFile.remove_file()`, and `JarFile.rewrite()`. `rewrite()` can copy entries verbatim, or lift `.class` entries through `ClassModel` for in-place transforms plus the usual `recompute_frames`, `resolver`, and `debug_info` lowering controls. Pass `skip_debug=True` for an ASM-like lift path that omits `SourceFile`, `SourceDebugExtension`, `LineNumberTable`, `LocalVariableTable`, `LocalVariableTypeTable`, and `MethodParameters` before transformation. Signature-related files are preserved as raw resources and are not re-signed automatically, so rewritten signed JARs may no longer verify as signed.

A minimal transform pipeline looks like:

```python
from pytecode import JarFile
from pytecode.constants import MethodAccessFlag
from pytecode.model import ClassModel, MethodModel
from pytecode.transforms import (
    class_named,
    method_is_public,
    method_is_static,
    method_name_matches,
    on_methods,
    pipeline,
)


def make_final(method: MethodModel, _owner: ClassModel) -> None:
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

Run the test suite:

```powershell
uv run pytest -q
```

Run the full validation pass:

```powershell
uv run ruff check .
uv run ruff format --check .
uv run basedpyright
uv run pytest -q
uv run python tools\generate_api_docs.py --check
```

Run the JVM-backed CFG oracle suite only:

```powershell
uv run pytest -q -m oracle
```

CI runs the `oracle`-marked CFG tests as a dedicated required job. If you want to mirror that split locally, run the main suite and oracle suite separately:

```powershell
uv run pytest -q -m "not oracle"
uv run pytest -q -m oracle
```

Validate docstring coverage across the public API surface (this is also a dedicated CI gate):

```powershell
uv run python tools\generate_api_docs.py --check
```

Generate local API reference HTML in the ignored `docs\api\` directory:

```powershell
uv run python tools\generate_api_docs.py
```

Contributors should generally rely on the hosted GitHub Pages site for browsing the API reference rather than committing generated HTML.

The `oracle`-marked CFG tests lazily cache ASM 9.7.1 test jars under `.pytest_cache\pytecode-oracle` and also honor manually seeded jars in `tests\resources\oracle\lib`. If `java`, `javac`, or the ASM jars are unavailable, the oracle suite skips instead of failing the rest of the test run.

Build source and wheel distributions locally:

```powershell
uv build
```

## Release automation

PyPI releases are published from GitHub Actions by pushing an immutable `v<version>` tag that matches `project.version` in `pyproject.toml`. The release workflow reruns validation on the tagged commit, builds both `sdist` and `wheel` with `uv build`, and publishes from the protected `pypi` environment via PyPI Trusted Publishing.

One-time setup for maintainers:

1. In the PyPI project settings, add a Trusted Publisher for this repository.
2. Authorize the `release.yml` workflow file (`.github/workflows/release.yml`).
3. Set the GitHub Actions environment to `pypi`, and add any desired environment protection rules in GitHub before enabling publication.

Release procedure:

```powershell
# 1) bump project.version in pyproject.toml
uv run ruff check .
uv run ruff format --check .
uv run basedpyright
uv run pytest -q
uv run python tools\generate_api_docs.py --check

git commit -am "Bump version to 0.0.2"
git push origin <default-branch>
git tag v0.0.2
git push origin v0.0.2
```

The release workflow rejects tags that do not match `project.version`. Treat release tags as immutable: if a tag or published artifact is wrong, bump to a new version and publish a new tag instead of force-pushing the old one. If the workflow fails before the publish step because of an environment approval or a transient PyPI issue, rerun the workflow for the same tag instead of moving the tag.

## Script validation

`run.py` is a manual smoke-test helper for the checked-in `225.jar` sample. Running it writes extracted output under `output\225` for inspection or comparison.

`tools\parse_wiki_instructions.py` supports deterministic generation from a local HTML file you provide:

```powershell
uv run python .\tools\parse_wiki_instructions.py --input-html .\path\to\wiki_instructions.html --output .\instruction_dump.txt
```

If `--input-html` is omitted, the script fetches the Wikipedia source page and prints the generated mapping to stdout.
