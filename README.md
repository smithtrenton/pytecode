# pytecode

`pytecode` is a Python 3.14+ library for parsing, inspecting, editing, validating, and emitting JVM class files and JAR archives.

It is built for Python tooling that needs direct access to Java bytecode: classfile readers and writers, archive rewriters, transformation pipelines, control-flow analysis, descriptor utilities, hierarchy-aware frame computation, and verification-oriented workflows.

## Why pytecode?

- Parse `.class` files into typed Python dataclasses.
- Edit classes, fields, methods, and bytecode through a mutable symbolic model.
- Rewrite JAR files while preserving non-class resources and ZIP metadata.
- Recompute `max_stack`, `max_locals`, and `StackMapTable` when requested.
- Validate parsed classfiles and edited models before emission.
- Work with descriptors, signatures, labels, symbolic operands, constant pools, and debug-info policies.

## Installation

Install from PyPI:

```bash
pip install pytecode
```

Or with `uv`:

```bash
uv add pytecode
```

`pytecode` requires Python `3.14+`.

## Quick start

### Parse and roundtrip a class file

```python
from pathlib import Path

from pytecode import ClassReader, ClassWriter

reader = ClassReader.from_file("HelloWorld.class")
classfile = reader.class_info

print(classfile.major_version)
print(classfile.methods_count)

Path("HelloWorld-copy.class").write_bytes(ClassWriter.write(classfile))
```

### Lift to the editable model

```python
from pathlib import Path

from pytecode import ClassModel

model = ClassModel.from_bytes(Path("HelloWorld.class").read_bytes())
print(model.name)

updated_bytes = model.to_bytes()
Path("HelloWorld-updated.class").write_bytes(updated_bytes)
```

Use `recompute_frames=True` when an edit changes control flow or stack/local layout.

## JAR rewriting example

`JarFile.rewrite()` can apply in-place transforms to matching classes and methods:

```python
from pytecode import JarFile
from pytecode.classfile.constants import MethodAccessFlag
from pytecode.edit.model import ClassModel, MethodModel
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

Transforms must mutate models in place and return `None`. For code-shape changes, pass `recompute_frames=True`. For an ASM-like lift path that omits debug metadata, pass `skip_debug=True`.

## Public surface

Top-level exports:

- `pytecode.ClassReader` and `pytecode.ClassWriter` for raw classfile parsing and emission.
- `pytecode.JarFile` for archive reads, mutation, and safe rewrite-to-disk.
- `pytecode.ClassModel` for mutable editing with symbolic references.

Supported submodules:

- `pytecode.transforms` for composable class, field, method, and code transforms.
- `pytecode.edit.labels` for label-aware bytecode editing helpers.
- `pytecode.edit.operands` for symbolic operand wrappers.
- `pytecode.analysis` for CFG construction, frame simulation, and recomputation helpers.
- `pytecode.analysis.verify` for structural validation and diagnostics.
- `pytecode.analysis.hierarchy` for type and override resolution helpers.
- `pytecode.descriptors` for JVM descriptors and generic signatures.
- `pytecode.edit.constant_pool_builder` for deterministic constant-pool construction.
- `pytecode.classfile.modified_utf8` for JVM Modified UTF-8 encoding and decoding.
- `pytecode.edit.debug_info` for explicit debug-info preservation and stripping policies.

## Documentation

- Development docs overview: [docs/OVERVIEW.md](https://github.com/smithtrenton/pytecode/blob/master/docs/OVERVIEW.md)
- Hosted API reference: <https://smithtrenton.github.io/pytecode/>

## Development

Create a local environment with development tools:

```powershell
uv sync --extra dev
```

Common checks:

```powershell
uv run ruff check .
uv run ruff format --check .
uv run basedpyright
uv run pytest -q
uv run python tools/generate_api_docs.py --check
```

Generate local API reference HTML with:

```powershell
uv run python tools/generate_api_docs.py
```

Build source and wheel distributions locally:

```powershell
uv build
```

Profile isolated JAR-processing stages without `run.py`'s output overhead:

```powershell
uv run python tools/profile_jar_pipeline.py path/to/jar.jar
uv run python tools/profile_jar_pipeline.py path/to/jar.jar --stages class-parse model-lift model-lower
uv run python tools/profile_jar_pipeline.py path/to/dir/with/jars --stages model-lift model-lower --summary-json output/profiles/common-libs/summary.json
```

When making runtime-performance changes, prefer checking both a focused jar such as `225.jar` and the wider common-jar corpus so regressions and wins are not judged from a single artifact. A single jar defaults to all stages; directories and multi-jar runs default to `model-lift` and `model-lower`.

The `oracle`-marked CFG tests lazily cache ASM 9.7.1 test jars under `.pytest_cache/pytecode-oracle` and also honor manually seeded jars in `tests/resources/oracle/lib`. If `java`, `javac`, or the ASM jars are unavailable, that suite skips without failing the rest of the test run.

## Release automation

PyPI releases are published from GitHub Actions by pushing an immutable `v<version>` tag that matches `project.version` in `pyproject.toml`. The same workflow can also be started manually for an existing tag by supplying a `tag` input. In both cases, the workflow checks out the tagged commit, reruns validation, builds both `sdist` and `wheel` with `uv build`, publishes from the protected `pypi` environment via PyPI Trusted Publishing, and then creates or updates a GitHub Release for the same tag with the built distributions attached.

Release procedure:

```powershell
# 1) bump project.version in pyproject.toml
uv run ruff check .
uv run ruff format --check .
uv run basedpyright
uv run pytest -q
uv run python tools/generate_api_docs.py --check

git commit -am "Bump version to X.Y.Z"
git push origin master
git tag vX.Y.Z
git push origin vX.Y.Z
```

The release workflow rejects tags that do not match `project.version`. Treat release tags as immutable: if a tag or published artifact is wrong, bump to a new version and publish a new tag instead of force-pushing the old one. For an existing tag, you can rerun the workflow directly or start it manually from Actions by providing the tag name. The workflow is safe to rerun for the same tag: PyPI uploads skip files that already exist, and the GitHub Release step updates the existing release assets in place if the release was already created.

## Repository utilities

`run.py` is a manual smoke-test helper that parses a JAR file, writes pretty-printed parsed class structures under `<jar parent>/output/<jar stem>/parsed/`, and writes class-model-derived rewritten `.class` files plus copied resources under `<jar parent>/output/<jar stem>/rewritten/`.

Example:

```powershell
uv run python ./run.py ./path/to/input.jar
```

The script prints read, parse, lift, write, and rewrite timings plus class and resource counts to stdout.
