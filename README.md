# pytecode

`pytecode` is a Python 3.14+ library for parsing, inspecting, editing, and emitting JVM class files, bytecode, and JAR archives.

It is aimed at tools that need to work with Java bytecode directly from Python: classfile readers and writers, archive rewriters, bytecode transforms, control-flow analysis, descriptor parsing, and validation.

## Why pytecode?

- Parse `.class` files into structured Python objects.
- Edit classes, fields, methods, and instructions through a mutable model.
- Rewrite JAR files while preserving non-class resources.
- Recompute control-flow metadata such as `max_stack`, `max_locals`, and `StackMapTable`.
- Validate classfiles and edited models before emission.
- Work with descriptors, signatures, constant pools, labels, and symbolic operands.

## Installation

Install from PyPI:

```bash
pip install pytecode
```

Or with `uv`:

```bash
uv add pytecode
```

`pytecode` currently requires Python `3.14+`.

## Quick start

Read a class, inspect it, and write it back out:

```python
from pathlib import Path

from pytecode import ClassReader, ClassWriter

data = Path("HelloWorld.class").read_bytes()
classfile = ClassReader(data).read()

print(classfile.this_class.name.value)

round_tripped = ClassWriter().write(classfile)
Path("HelloWorld-copy.class").write_bytes(round_tripped)
```

For higher-level editing, use `ClassModel`:

```python
from pytecode.model import ClassModel

model = ClassModel.from_bytes(Path("HelloWorld.class").read_bytes())
print(model.name)

updated_bytes = model.to_bytes()
```

## JAR rewriting example

`JarFile.rewrite()` can apply in-place transformations to matching classes and methods:

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

## Main modules

The top-level package and companion modules cover the current public surface:

- `pytecode.ClassReader` and `pytecode.ClassWriter` for raw classfile parsing and emission.
- `pytecode.JarFile` for archive reads, writes, and class-aware rewriting.
- `pytecode.ClassModel` for mutable editing with automatic lowering back to bytes.
- `pytecode.transforms` for composable class, field, method, and code transforms.
- `pytecode.labels` for label-aware instruction editing helpers.
- `pytecode.operands` for symbolic operand wrappers.
- `pytecode.analysis` for CFG construction, frame simulation, and recomputation helpers.
- `pytecode.verify` for structural validation and diagnostics.
- `pytecode.hierarchy` for type and override resolution helpers.
- `pytecode.descriptors` for parsing and producing JVM descriptors and signatures.
- `pytecode.constant_pool_builder` for deterministic constant-pool construction.
- `pytecode.modified_utf8` for JVM Modified UTF-8 encoding and decoding.
- `pytecode.debug_info` for preserving or stripping debug metadata during lowering.

## Documentation

- Project overview and roadmap: <https://github.com/smithtrenton/pytecode/blob/main/docs/OVERVIEW.md>
- Hosted API reference: <https://smithtrenton.github.io/pytecode/>

## Development

Create a local environment with development tools:

```powershell
uv sync --extra dev
```

Common commands:

```powershell
uv run ruff check .
uv run ruff format --check .
uv run basedpyright
uv run pytest -q
uv run python tools\generate_api_docs.py --check
```

Generate local API reference HTML:

```powershell
uv run python tools\generate_api_docs.py
```

Build source and wheel distributions locally:

```powershell
uv build
```

The `oracle`-marked CFG tests lazily cache ASM 9.7.1 test jars under `.pytest_cache\pytecode-oracle` and also honor manually seeded jars in `tests\resources\oracle\lib`. If `java`, `javac`, or the ASM jars are unavailable, the oracle suite skips instead of failing the rest of the test run.

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

git commit -am "Bump version to X.Y.Z"
git push origin <default-branch>
git tag vX.Y.Z
git push origin vX.Y.Z
```

The release workflow rejects tags that do not match `project.version`. Treat release tags as immutable: if a tag or published artifact is wrong, bump to a new version and publish a new tag instead of force-pushing the old one. If the workflow fails before the publish step because of an environment approval or a transient PyPI issue, rerun the workflow for the same tag instead of moving the tag.

## Repository utilities

`run.py` is a manual smoke-test helper that parses a JAR file, pretty-prints parsed class structures, and copies non-class resources into an output directory next to the input JAR.

Example:

```powershell
uv run python .\run.py .\225.jar
```

When run against the checked-in sample, it writes extracted output under `.\output\225\` and prints timing plus class/resource counts to stdout.
