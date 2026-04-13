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
uv run python tools/generate_api_docs.py --check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv build --wheel --sdist
```

## What the current suite covers

- `crates/pytecode-engine/tests/raw_roundtrip.rs`, `verifier.rs`, `analysis.rs`, `model.rs`, and `transform.rs` cover parser/emitter fidelity, diagnostics, lowering, analysis, and transform invariants in the Rust engine.
- `crates/pytecode-archive/tests/jar.rs` plus `crates/pytecode-cli/tests/*.rs` cover archive rewrite behavior and CLI-facing smoke workflows.
- `tests/test_rust_bindings.py`, `tests/test_rust_transforms.py`, and `tests/test_jar.py` cover the Python-facing bindings, transform composition, and archive rewrite semantics.
- `tests/javap_parser.py` and `tests/test_javap_parser.py` cover the `javap` parser and semantic-diff utilities.
- `tests/test_api_docs.py`, `tests/test_validate_release_tag.py`, and `tools/generate_api_docs.py --check` enforce the documented public surface and release-tag expectations.

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

`pytecode` maintains a release-quality bar for a Rust-backed JVM bytecode toolkit with a Python API: deterministic classfile emission, a mutable symbolic editing model, transform composition, hierarchy-aware queries, structured validation diagnostics, archive rewriting, semantic-diff utilities, and generated API reference coverage.
