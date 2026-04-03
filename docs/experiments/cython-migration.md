# Cython migration plan

## Motivation

pytecode's hot paths — binary I/O (`bytes_utils`), constant-pool parsing (`reader`), instruction decoding, Modified UTF-8 codec, and class serialization (`writer`) — are loop-heavy, struct-packing operations that run millions of iterations when processing JARs. CPython interprets each loop iteration, function call, and type dispatch individually, making these paths prime candidates for compiled acceleration.

Cython offers a pragmatic middle ground: the source stays Python-readable, the build chain uses standard setuptools, and the fallback to pure Python is trivial. For tight struct-unpacking and byte-level loops, Cython routinely achieves 5–50× speedup over interpreted CPython.

## Strategy

### `.pyx` files alongside `.py` fallbacks

Each target module gets a Cython implementation (`.pyx`) that mirrors the public API of the original `.py` module. The `.py` file remains the canonical source for:

- Type checking (basedpyright strict mode)
- Documentation and docstrings
- Fallback execution when the compiled extension is unavailable

### Fallback import mechanism

A thin import shim (`pytecode/_internal/cython_import.py`) tries to import the compiled Cython module first. If the import fails — or if the `PYTECODE_BLOCK_CYTHON=1` environment variable is set — it falls back to the pure-Python implementation. This ensures:

- Source installs without a C compiler still work
- Tests can validate both backends
- Debugging can bypass Cython when needed

### Rollout plan

1. **Phase 1 (current)**: Fallback always available; Cython is opt-in acceleration.
2. **Phase 2 (future)**: Once benchmarks confirm stability and speedup across platforms, consider making Cython the default and dropping the fallback.

## Module priority order

Ports proceed bottom-up so each module can import its Cython dependencies directly (avoiding Python dispatch overhead on inner loops):

| Priority | Module | Rationale |
|----------|--------|-----------|
| 1 | `_internal/bytes_utils.py` | Foundation for all I/O; called millions of times; smallest module (209 lines); easy first win |
| 2 | `classfile/modified_utf8.py` | Self-contained codec; called per `CONSTANT_Utf8` entry; tight byte loop benefits from `cdef` |
| 3 | `classfile/reader.py` | Largest impact: constant-pool parsing + instruction decoding inner loop |
| 4 | `classfile/writer.py` | 17-way `isinstance()` dispatch per CP entry; pairs with reader for roundtrip validation |

Future candidates (not in initial scope):

- `edit/constant_pool_builder.py` — dict cloning and entry deduplication
- `edit/labels.py` — 35-way instruction clone dispatch
- `analysis/__init__.py` — frame simulation worklist

## Build integration

### setuptools + `cythonize()`

The project already uses setuptools as its build backend. A `setup.py` is added with conditional `cythonize()` that discovers all `.pyx` files under `pytecode/`:

```python
from setuptools import setup

try:
    from Cython.Build import cythonize
    ext_modules = cythonize("pytecode/**/*.pyx", language_level="3")
except ImportError:
    ext_modules = []

setup(ext_modules=ext_modules)
```

When Cython is not installed, the build proceeds as a pure-Python package (no compiled extensions).

### Dev dependency

Cython is added to `[project.optional-dependencies] dev` so that `uv sync --extra dev` installs it alongside the existing linting and testing tools.

### Wheel distribution

Built wheels include the compiled `.so`/`.pyd` extensions. Source distributions include the `.pyx` source so that users can compile from source if desired.

## Testing strategy

### Dual-backend validation

Every existing test must pass with both the Cython and pure-Python backends. CI runs the test suite twice:

1. Normal run (Cython extensions loaded if available)
2. `PYTECODE_BLOCK_CYTHON=1` run (pure-Python fallback forced)

### Benchmark regression gate

The profiling tool (`tools/profile_jar_pipeline.py`) is extended with a `--backend` flag to compare Cython vs pure-Python execution. A dedicated benchmark script (`tools/benchmark_cython.py`) provides a quick comparison on `225.jar`.

Benchmark results are tracked in `docs/experiments/cython-benchmarks.md` with saved JSON baselines under `output/profiles/`.

## Naming conventions

| Artifact | Pattern | Example |
|----------|---------|---------|
| Cython source | `_<module>_cy.pyx` | `_bytes_utils_cy.pyx` |
| Cython header | `_<module>_cy.pxd` | `_bytes_utils_cy.pxd` |
| Generated C | `_<module>_cy.c` (gitignored) | `_bytes_utils_cy.c` |
| Compiled ext | `_<module>_cy.*.so` / `.pyd` | `_bytes_utils_cy.cpython-314-x86_64-linux-gnu.so` |

## Related docs

- [../project/quality-gates.md](../project/quality-gates.md) — release-quality gates
- [../project/roadmap.md](../project/roadmap.md) — delivered milestones
- [cython-benchmarks.md](cython-benchmarks.md) — benchmark results tracking
