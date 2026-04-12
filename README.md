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
from pytecode.archive import FrameComputationMode
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
        add_access_flags(0x0010),
        owner_matcher=class_named("HelloWorld"),
    )
    .build()
)


JarFile("input.jar").rewrite(
    "output.jar",
    transform=pipeline.apply,
)
```

For code-shape changes, pass `frame_mode=FrameComputationMode.RECOMPUTE`. To strip debug metadata during rewrite, pass `skip_debug=True`.

`JarFile.rewrite()` now routes archive rewriting through the Rust archive layer for supported cases, including in-memory archive edits and Rust-backed transforms. Pipelines that contain Python callback steps are rejected instead of falling back to a Python archive rewrite path.

## Public surface

Top-level exports:

- `pytecode.ClassReader` / `pytecode.ClassWriter` for Rust-backed raw classfile parsing and emission.
- `pytecode.ClassModel` for Rust-owned mutable editing.
- `pytecode.JarFile` for Rust-backed archive reads and rewrite workflows.

Canonical semantic modules:

- `pytecode.classfile` for raw Rust-backed classfile reading and writing.
- `pytecode.model` for editable class models, typed code items, labels, and code metadata wrappers.

### Rust-first API map

| Removed entry point | Current replacement |
| --- | --- |
| `pytecode.classfile.reader.ClassReader(...)` | `pytecode.ClassReader.from_bytes(...)` |
| `pytecode.classfile.writer.ClassWriter.write(...)` | `pytecode.ClassWriter.write(...)` |
| `pytecode.edit.model.ClassModel.from_bytes(...)` | `pytecode.ClassModel.from_bytes(...)` |
| `pytecode.analysis.verify.verify_classfile(...)` | `pytecode.analysis.verify.verify_classfile(...)` |
| `pytecode.analysis.verify.verify_classmodel(...)` | `pytecode.analysis.verify.verify_classmodel(...)` |
| `pytecode.analysis.hierarchy.MappingClassResolver` | `pytecode.analysis.MappingClassResolver` |
| Legacy Python matcher / transform DSL | `pytecode.transforms.PipelineBuilder` plus `pytecode.transforms` helpers |

Supported submodules:

- `pytecode.transforms` for Rust-backed class, field, and method transforms.
- `pytecode.analysis` for Rust-backed verification entry points.
- `pytecode.analysis.verify` for structural validation and diagnostics.
- `pytecode.analysis.hierarchy` for type and override resolution helpers.
- `pytecode.classfile.descriptors` for JVM descriptors and generic signatures.
- `pytecode.classfile.constants` for access flags, opcodes, and verifier constants.

## Documentation

- Development docs overview: [docs/OVERVIEW.md](https://github.com/smithtrenton/pytecode/blob/master/docs/OVERVIEW.md)
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

Rust workspace smoke and comparison commands:

```powershell
cargo run -p pytecode-cli -- compat-manifest --max-release 25
cargo run --release -p pytecode-cli -- bench-smoke --iterations 5
uv run python tools\benchmark_jar_pipeline.py crates\pytecode-engine\fixtures\jars\byte-buddy-1.17.5.jar --iterations 5
uv run python tools\compare_rust_python_benchmarks.py --jar crates\pytecode-engine\fixtures\jars\byte-buddy-1.17.5.jar --iterations 5 --output output\benchmarks\rust-vs-python-byte-buddy.json
cargo run -p pytecode-cli -- class-summary --path target\pytecode-rust-javac\release-8\HelloWorld\classes\HelloWorld.class
cargo run -p pytecode-cli -- rewrite-smoke --jar input.jar --output output.jar --class-name HelloWorld
cargo run -p pytecode-cli -- patch-jar --jar input.jar --output output.jar --rules rules.json
uv run python examples\patch_jar.py --jar input.jar --output output.jar --rules rules.json
cargo run -p pytecode-cli -- deobfuscate analyze --jar injected-client-1.12.22.1.jar
cargo run -p pytecode-cli -- deobfuscate rewrite --jar injected-client-1.12.22.1.jar --output output\injected-client-cleaned.jar
uv run python examples\deobfuscate.py analyze --jar injected-client-1.12.22.1.jar
uv run python examples\deobfuscate.py rewrite --jar injected-client-1.12.22.1.jar --output output\injected-client-python-cleaned.jar
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

If you want the same workflow from the Python side, `examples\deobfuscate.py` reimplements the deobfuscation analyze/rewrite flow on top of `pytecode.JarFile`, `ClassModel`, and Rust-backed Python transforms instead of shelling out to the Rust CLI. The Python rewrite path stays intentionally conservative today: it only strips `nop` instructions from straight-line methods, while the richer control-flow cleanup still lives in the Rust CLI implementation.

Rust-owned fixtures now live under `crates\pytecode-engine\fixtures\`. Rust crate tests only read those copied fixture sources and do not invoke Python. The Rust harness lazily compiles `crates\pytecode-engine\fixtures\java\*.java` into `target\pytecode-rust-javac\...` and only reruns `javac` when the source bytes, required `--release`, or `javac` identity changes.

`bench-smoke` now reports isolated-stage timing samples with median+spread summaries instead of only one accumulated elapsed total. `tools\benchmark_jar_pipeline.py` mirrors that isolated-stage reporting for the wrapper-inclusive Python path, and `tools\compare_rust_python_benchmarks.py` frames the result as native Rust timings versus Python wrapper overhead on the same jar.

Small Rust examples now live under `crates\pytecode-engine\examples\finalize_main.rs` and `crates\pytecode-archive\examples\rewrite_jar.rs` to show direct transform and archive rewrite workflows without going through the CLI.

The recommended long-term Rust workspace shape is intentionally small:

- `pytecode-engine` for the main classfile/edit/analysis/transform engine,
- `pytecode-archive` for JAR handling,
- `pytecode-cli` for compatibility and benchmark tooling,
- `pytecode-python` for PyO3 bindings and wheel packaging.

That keeps the important seams while avoiding unnecessary crate churn during the early implementation phases.

Current Rust implementation status:

- phases 0, 1, 2, 3, 4, 5, 6, and 7 are complete,
- `pytecode-engine` now has the raw classfile parse/write core plus a real symbolic model, hierarchy/CFG/verification analysis surface, constant-pool builder, symbolic operands, labels, debug-info handling, raw <-> symbolic lift/lower, and a Rust transform layer with `Pipeline`, `pipeline!`, lifted field/method/code helpers, and matcher composition,
- `pytecode-archive` now provides in-memory JAR state with add/remove operations plus safe rewrite-to-disk that can pass classes through unchanged or re-lower them through transforms and Phase 4 frame recomputation,
- `pytecode-cli` now has rewrite smoke coverage plus isolated benchmark reporting with median+spread summaries instead of only cumulative elapsed totals,
- release-ready Cargo metadata, release automation, and crate-level examples now make the Rust workspace much closer to a standalone deliverable,
- the Rust verifier now enforces strict Java SE 25 classfile version rules, chapter-4 structure/access/attribute checks, bootstrap-linked `MethodHandle` / `Dynamic` / `InvokeDynamic` validation, and generic-signature syntax checks for `Signature` and `LocalVariableTypeTable`,
- legacy `jsr` / `jsr_w` / `ret` bytecode is now supported through CFG/frame recomputation and symbolic lift/lower; recomputed lowering preserves those methods on pre-50 classfile versions without emitting invalid `StackMapTable` state,
- benchmark `model-lift` and `model-lower` stages now exercise the real symbolic pipeline, and focused Rust tests cover exact roundtrip fidelity, debug-info strip/stale behavior, interface-backed method references, conditional branch widening, switch-layout edits, hierarchy queries, CFG construction, structured verifier diagnostics, bootstrap/signature validation, legacy subroutine handling, transform pipeline behavior, archive rewrite behavior, and end-to-end `StackMapTable` recomputation for edited methods,
- phase 6 added benchmark-report tooling, examples, public-surface polish, and packaging polish on top of the completed lowering and analysis stack,
- phase 7 added `pytecode-python`, `maturin`-built source/platform distributions, a Rust-backed `pytecode._rust` extension module, and compatibility bridges so `ClassModel.from_classfile`, `ResolvedClass.from_classfile`, `verify_classfile`, and `ClassWriter.write` all accept Rust-backed classfiles,
- the default `ClassReader` parse path now always goes through the Rust extension,
- `ClassModel.from_bytes()` now single-parses through Rust, preserves original bytes for byte-exact clean roundtrips, and routes the normal code-mutation serialization path back through Rust,
- the canonical top-level Python surface is now Rust-owned objects (`ClassModel`, Rust-backed verification, and Rust-backed resolver types); the older Python-owned model/edit APIs have been removed.

Generate local API reference HTML with:

```powershell
uv run python tools/generate_api_docs.py
```

Build source and wheel distributions locally:

```powershell
uv build
```

`uv build` now produces a mixed Python/Rust wheel (for example `pytecode-0.0.3-cp311-abi3-win_amd64.whl`) instead of a pure-Python `py3-none-any` wheel.

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

The `oracle`-marked CFG tests lazily cache ASM 9.7.1 test jars under `.pytest_cache/pytecode-oracle` and also honor manually seeded jars in `tests/resources/oracle/lib`. If `java`, `javac`, or the ASM jars are unavailable, that suite skips without failing the rest of the test run.

## Release automation

PyPI releases are published from GitHub Actions by pushing an immutable `v<version>` tag that matches `project.version` in `pyproject.toml`. The same workflow can also be started manually for an existing tag by supplying a `tag` input. In both cases, the workflow checks out the tagged commit, reruns validation, builds both `sdist` and the `maturin`-backed extension wheel with `uv build`, publishes from the protected `pypi` environment via PyPI Trusted Publishing, and then creates or updates a GitHub Release for the same tag with the built distributions attached.

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
