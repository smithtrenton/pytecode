# pytecode

`pytecode` is a Python 3.14+ library for parsing, inspecting, editing, validating, and emitting JVM class files and JAR archives.

It is built for Python tooling that needs direct access to Java bytecode: classfile readers and writers, archive rewriters, transformation pipelines, control-flow analysis, descriptor utilities, hierarchy-aware frame computation, and verification-oriented workflows.

## Why pytecode?

- Parse `.class` files into typed Python objects backed by the Rust engine.
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

Published releases ship prebuilt wheels for Windows, macOS, and Linux.
If a matching wheel is unavailable, `pip`/`uv` falls back to a source build,
which requires a working Rust toolchain.

`pytecode` requires Python `3.14+`.

## Quick start

### Parse and roundtrip a class file

```python
from pathlib import Path

from pytecode.classfile import ClassReader, ClassWriter

reader = ClassReader.from_file("HelloWorld.class")
classfile = reader.class_info

print(classfile.major_version)
print(classfile.methods_count)

Path("HelloWorld-copy.class").write_bytes(ClassWriter.write(classfile))
```

### Edit through the Rust-owned model

```python
from pathlib import Path

from pytecode.model import ClassModel

model = ClassModel.from_bytes(Path("HelloWorld.class").read_bytes())
print(model.name)

updated_bytes = model.to_bytes()
Path("HelloWorld-updated.class").write_bytes(updated_bytes)
```

Import `FrameComputationMode` from `pytecode.archive` and use `to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)` when an edit changes control flow or stack/local layout.

## JAR rewriting example

`JarFile.rewrite()` can apply in-place transforms to matching classes and methods:

```python
from pytecode import JarFile
from pytecode.classfile.constants import MethodAccessFlag
from pytecode.transforms import (
    PipelineBuilder,
    add_access_flags,
    class_named,
    method_is_public,
    method_is_static,
    method_name_matches,
)


pipeline = (
    PipelineBuilder()
    .on_methods(
        method_name_matches(r"main") & method_is_public() & method_is_static(),
        add_access_flags(int(MethodAccessFlag.FINAL)),
        owner_matcher=class_named("HelloWorld"),
    )
    .build()
)


JarFile("input.jar").rewrite(
    "output.jar",
    transform=pipeline.apply,
)
```

For code-shape changes, pass `frame_mode=FrameComputationMode.RECOMPUTE`. To strip debug metadata during rewrite, prefer `debug_info=DebugInfoPolicy.STRIP`.

`JarFile.rewrite()` uses the Rust archive layer for in-memory archive edits and
Rust-native transforms. Python-callable transforms are also supported through
the same public API when a workflow needs `ClassModel`-level mutation.

## Public surface

Top-level exports:

- `pytecode.ClassReader` / `pytecode.ClassWriter` for Rust-backed raw classfile parsing and emission.
- `pytecode.ClassModel` for Rust-owned mutable editing.
- `pytecode.JarFile` for Rust-backed archive reads and rewrite workflows.

Canonical semantic modules:

- `pytecode.classfile` for raw Rust-backed classfile reading and writing.
- `pytecode.model` for editable class models, typed code items, labels, and code metadata wrappers.

Supported submodules:

- `pytecode.transforms` for Rust-backed class, field, and method transforms.
- `pytecode.analysis` for Rust-backed verification entry points.
- `pytecode.analysis.verify` for structural validation and diagnostics.
- `pytecode.analysis.hierarchy` for type and override resolution helpers.
- `pytecode.classfile.attributes` for typed raw attribute dataclasses.
- `pytecode.classfile.bytecode` for opcode and array-type enums.
- `pytecode.classfile.constants` for access flags, opcodes, and verifier constants.
- `pytecode.model` for editable class, field, method, and code models.

## Documentation

- Development docs overview: [docs/OVERVIEW.md](docs/OVERVIEW.md)
- Hosted API reference: <https://smithtrenton.github.io/pytecode/>

## Development

Create a local environment with development tools:

```powershell
uv sync --extra dev
```

`uv sync --extra dev` now builds the local editable package through `maturin`, so a working Rust toolchain is required alongside Python.

Common checks:

```powershell
uv run ruff check .
uv run ruff format --check .
uv run basedpyright
uv run pytest -q
uv run python tools/generate_api_docs.py --check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Workspace smoke and comparison commands:

```powershell
cargo run --release -p pytecode-cli -- bench-smoke --iterations 5
uv run python tools\benchmark_jar_pipeline.py crates\pytecode-engine\fixtures\jars\byte-buddy-1.17.5.jar --iterations 5
uv run python tools\compare_rust_python_benchmarks.py --jar crates\pytecode-engine\fixtures\jars\byte-buddy-1.17.5.jar --iterations 5 --output output\benchmarks\rust-vs-python-byte-buddy.json
cargo run -p pytecode-cli -- class-summary --path crates\pytecode-engine\fixtures\classes\HelloWorld\HelloWorld.class
cargo run -p pytecode-cli -- rewrite-smoke --jar input.jar --output output.jar --class-name HelloWorld
cargo run -p pytecode-cli -- patch-jar --jar input.jar --output output.jar --rules rules.json
uv run python examples\patch_jar.py --jar input.jar --output output.jar --rules rules.json
cargo run -p pytecode-cli -- deobfuscate analyze --jar path\to\obfuscated-client.jar
cargo run -p pytecode-cli -- deobfuscate rewrite --jar path\to\obfuscated-client.jar --output output\obfuscated-client-cleaned.jar
uv run python examples\deobfuscate.py analyze --jar path\to\obfuscated-client.jar
uv run python examples\deobfuscate.py rewrite --jar path\to\obfuscated-client.jar --output output\obfuscated-client-python-cleaned.jar
```

`patch-jar` is the first real config-driven consumer of the Rust archive + transform stack. It reads a JSON rule file, rewrites matching classes in a JAR, and prints a JSON report with per-rule match/change counts.

`examples\patch_jar.py` mirrors that workflow from Python and accepts the same `rules.json` shape, but applies it through the Python API.

Example `rules.json`:

```json
{
  "options": {
    "debug_info": "preserve",
    "frame_mode": "preserve"
  },
  "rules": [
    {
      "name": "finalize-main",
      "kind": "method",
      "owner": {
        "name": "HelloWorld"
      },
      "matcher": {
        "name": "main",
        "access_all": ["public", "static"],
        "has_code": true
      },
      "action": {
        "type": "add-access-flags",
        "flags": ["final"]
      }
    }
  ]
}
```

The first version supports:

- class rules for access-flag edits, `set-super-class`, `add-interface`, and `remove-interface`
- field rules for access-flag edits, `rename`, and `remove`
- method rules for access-flag edits, `rename`, and `remove`
- method `code_actions` for string replacement, method-call redirection, field-access redirection, opcode-based instruction removal, single-instruction replacement, before/after insertion, contiguous sequence replacement/removal, and grouped `sequence` action blocks

Example sequence rewrite inside `code_actions`:

```json
{
  "type": "replace-sequence",
  "pattern": [
    { "ldc_string": "Hello from fixture" },
    {
      "method_owner": "java/io/PrintStream",
      "method_name": "println",
      "method_descriptor": "(Ljava/lang/String;)V"
    }
  ],
  "replacement": [
    { "type": "ldc-string", "value": "patched via sequence action" },
    {
      "type": "method",
      "opcode": 182,
      "owner": "java/io/PrintStream",
      "name": "print",
      "descriptor": "(Ljava/lang/String;)V"
    }
  ]
}
```

Sequence patterns use matcher objects (`opcode`, `opcode_any`, `method_owner`, `method_name`, `method_descriptor`, `field_owner`, `field_name`, `field_descriptor`, `ldc_string`, `var_slot`, `type_descriptor`, and the `is_*` category flags). Replacement items support symbolic instructions like `raw`, `ldc-string`, `field`, `method`, `type`, `var`, and `iinc`, plus label-based control-flow items.

Replacement items can also express symbolic control flow with `label`, `branch`, `lookup-switch`, and `table-switch`. Branch and switch targets must reference labels declared inside the same replacement block, so malformed control-flow edits fail fast during plan loading instead of producing broken classfiles.

Example grouped rewrite that patches one instruction and brackets another with inserted NOPs:

```json
[
  {
    "type": "replace-insn",
    "matcher": { "ldc_string": "Hello from fixture" },
    "replacement": [
      { "type": "ldc-string", "value": "patched via replace-insn" }
    ]
  },
  {
    "type": "sequence",
    "actions": [
      {
        "type": "insert-before",
        "matcher": {
          "method_owner": "java/io/PrintStream",
          "method_name": "println",
          "method_descriptor": "(Ljava/lang/String;)V"
        },
        "items": [{ "type": "raw", "opcode": 0 }]
      },
      {
        "type": "insert-after",
        "matcher": {
          "method_owner": "java/io/PrintStream",
          "method_name": "println",
          "method_descriptor": "(Ljava/lang/String;)V"
        },
        "items": [{ "type": "raw", "opcode": 0 }]
      }
    ]
  }
]
```

`deobfuscate` is the higher-level workflow tool built on top of the same archive/model stack. It is aimed at jars like `injected-client`, where the goal is to inspect obfuscation signals first and then apply safe cleanup passes without hand-authoring JSON rules.

`deobfuscate analyze` currently reports:

- suspicious short-name class counts and samples
- package concentration (`<root>` versus named packages)
- `compilercontrol.json` JIT exclusion hints
- hotspot classes by size/method count
- classes with readable string constants that can anchor reverse-engineering work

`deobfuscate rewrite` currently applies conservative bytecode cleanup:

- remove `nop` instructions
- remove unconditional `goto` instructions that already target the immediate fallthrough label
- collapse unconditional `goto` chains to their terminal target

Use `patch-jar` when you already know the exact bytecode rewrite you want and want a declarative rule file. Use `deobfuscate` when you want a product-shaped inspection/cleanup pass over an obfuscated jar, especially `injected-client`.

If you want the same workflow from the Python side, `examples\deobfuscate.py`
provides the same analyze/rewrite flow on top of `pytecode.JarFile`,
`ClassModel`, and Rust-backed Python transforms.

Rust-owned fixtures now live under `crates\pytecode-engine\fixtures\`. Rust crate tests only read those copied fixture sources and do not invoke Python. The Rust harness lazily compiles `crates\pytecode-engine\fixtures\java\*.java` into `target\pytecode-rust-javac\...` and only reruns `javac` when the source bytes, required `--release`, or `javac` identity changes.

`bench-smoke` now reports isolated-stage timing samples with median+spread summaries instead of only one accumulated elapsed total. `tools\benchmark_jar_pipeline.py` mirrors that isolated-stage reporting for the wrapper-inclusive Python path, and `tools\compare_rust_python_benchmarks.py` frames the result as native Rust timings versus Python wrapper overhead on the same jar.

Small Rust examples live under
`crates\pytecode-engine\examples\finalize_main.rs` and
`crates\pytecode-archive\examples\rewrite_jar.rs` to show direct transform and
archive rewrite workflows without going through the CLI.

Workspace layout:

- `pytecode-engine` for classfile parsing, editing, transforms, analysis, and validation
- `pytecode-archive` for JAR handling
- `pytecode-cli` for CLI workflows and benchmarks
- `pytecode-python` for PyO3 bindings and wheel packaging

Generate local API reference HTML with:

```powershell
uv run python tools/generate_api_docs.py
```

Build source and wheel distributions locally:

```powershell
uv build
```

`uv build` produces both an sdist and a platform wheel for the current machine.

Profile isolated JAR-processing stages without `run.py`'s output overhead:

```powershell
uv run python tools/profile_jar_pipeline.py path/to/jar.jar
uv run python tools/profile_jar_pipeline.py path/to/jar.jar --stages class-parse model-lift model-lower
uv run python tools/profile_jar_pipeline.py path/to/dir/with/jars --stages model-lift model-lower --summary-json output/profiles/common-libs/summary.json
```

Compare native Rust vs Python-via-Rust stage timings with the Python extension rebuilt in release mode by default:

```powershell
uv run python tools/bench_full_comparison.py
uv run python tools/bench_full_comparison.py --extension-build installed
```

When making runtime-performance changes, prefer checking both a focused jar such as `crates\pytecode-engine\fixtures\jars\byte-buddy-1.17.5.jar` and the wider common-jar corpus so regressions and wins are not judged from a single artifact. Byte Buddy is the default focused Rust benchmark fixture because it is a common JVM library, carries a much larger class corpus than the old `225.jar` fixture, and also includes newer multi-release classes than Guava. A single jar defaults to all stages; directories and multi-jar runs default to `model-lift` and `model-lower`.

## Release automation

PyPI releases are published from GitHub Actions by pushing an immutable
`v<version>` tag that matches `project.version` in `pyproject.toml`. The same
workflow can also be started manually for an existing tag by supplying a `tag`
input. In both cases, the workflow checks out the tagged commit, reruns
validation, builds the source distribution plus platform wheels, publishes from
the protected `pypi` environment via PyPI Trusted Publishing, and then creates
or updates a GitHub Release for the same tag with the built distributions
attached.

Release procedure:

```powershell
# 1) bump project.version in pyproject.toml
uv run ruff check .
uv run ruff format --check .
uv run basedpyright
uv run pytest -q
uv run python tools/generate_api_docs.py --check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

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
