# Quality gates, non-goals, and maintenance bar

## Release-quality gates

A change is ready to merge or tag when it:

- passes lint, format, type-checking, and test validation
- keeps the documented public surface in sync with generated API docs
- preserves roundtrip and verifier behavior for existing fixtures
- documents any intentional behavior change in parsing, lowering, validation, or archive rewrite semantics
- adds or updates tests when the supported surface changes

Run the standard validation set with:

```powershell
uv run ruff check .
uv run ruff format --check .
uv run basedpyright
uv run pytest -q
uv run python tools\generate_api_docs.py --check
```

## What the current suite covers

- `tests/test_class_writer.py` exercises Tier 1 byte-for-byte roundtrip coverage for `ClassWriter.write()` and `ClassModel.to_bytes()`.
- `tests/test_validation.py` exercises the fixture and release matrix for Tier 1 roundtrip, Tier 2 structural verification, and Tier 4 JVM verification.
- `tests/javap_parser.py` and `tests/test_javap_parser.py` provide the Tier 3 semantic-diff engine for `javap`-level comparisons.
- `tests/test_cfg_oracle.py` differentially validates `pytecode.analysis.build_cfg()` against an ASM-backed JVM oracle when the required Java tooling is available.
- `tests/test_api_docs.py` and `tools/generate_api_docs.py --check` enforce the documented public-surface manifest and docstring coverage.

The `oracle`-marked CFG suite skips cleanly when Java or ASM dependencies are unavailable, so ordinary development workflows are not blocked by optional external tooling.

## Change expectations by area

- Parser or emitter changes should preserve existing roundtrip behavior unless the change is explicitly intended and documented.
- Editing-model, transform, or lowering changes should keep symbolic references, label behavior, and debug-info policies internally consistent.
- Validation changes should produce actionable diagnostics and avoid regressing the existing fixture matrix.
- New public API should ship with docstrings, tests, and README or docs updates when user-facing behavior changes.

## Non-goals

To keep scope focused, the project is not trying to become:

- a Java source parser
- a decompiler
- a full JVM runtime
- a bytecode optimizer unless optimization becomes an explicit product goal

## Summary

`pytecode` already meets the original quality bar for a Python bytecode manipulation toolkit: deterministic classfile emission, a mutable symbolic editing model, transform composition, hierarchy-aware analysis, structured validation diagnostics, optional JAR rewriting, JVM-backed CFG differential checks, multi-release validation tiers, and generated API reference coverage.
