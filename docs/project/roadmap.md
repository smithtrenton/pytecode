# Roadmap status

The original roadmap established the APIs listed below. The September 2026 review
found correctness and validation gaps in those APIs; feature availability alone
does not establish conformance. Follow the [improvement plan](improvement-plan-2026-09.md)
for implementation evidence and [compatibility limits](compatibility-and-limits.md)
for the supported behavior and remaining boundaries.

## Current status

The implemented API areas are:

1. A supported API for parsing and manipulating classfiles.
2. Analysis and frame recomputation support.
3. Validation and deterministic emission back to `.class` bytes and JAR archives.

## Delivered milestones

### Parsing and representation foundation

- Parser hardening for classfile structure, attributes, and instructions.
- Full constant-pool coverage, including Modified UTF-8 handling.
- Descriptor and signature parsing utilities.
- Rust-backed binary read and write primitives in `pytecode-engine`.
- Deterministic constant-pool management via `ConstantPoolBuilder`.

### Mutable editing surface

- `ClassModel`, `FieldModel`, `MethodModel`, and `CodeModel` as the primary symbolic editing layer.
- Label-based control-flow editing and lowering.
- Symbolic operand wrappers for non-control-flow instructions.
- Composable transforms and matcher DSL in `pytecode.transforms`.
- Explicit debug-info preservation, stripping, and stale-state controls.

### Analysis and validation

- Class hierarchy resolution in `pytecode.analysis.hierarchy`.
- Control-flow graph construction and stack/local simulation in `pytecode.analysis`.
- `max_stack`, `max_locals`, and `StackMapTable` recomputation.
- Structured verification diagnostics in `pytecode.analysis.verify`.
- Validation coverage across Rust workspace tests, Python API tests, and `javap` semantic-diff utilities.
- Historical CFG-oracle research for `build_cfg()`, retained as design background rather than a shipped default suite.

### Packaging and documentation

- Deterministic classfile emission via `ClassWriter.write()` and `ClassModel.to_bytes()`.
- Archive mutation and safe rewrite-to-disk in `pytecode.archive`.
- Generated API reference coverage enforced by tests and `tools/generate_api_docs.py --check`.
- Release automation aligned with immutable `v<version>` tags and PyPI Trusted Publishing.

## Further work

Independent verification and malformed-input coverage are ongoing work. Current
tests cover selected JVM behavior, not a complete verifier implementation. The
following additional API directions depend on concrete use cases and measurements:

- an opt-in javac-style constant-pool ordering mode for from-scratch generation
- higher-level instruction pattern matching layered on top of the current matcher DSL
- an optional visitor or streaming API if real throughput or memory-pressure workloads justify a second traversal model

## Related docs

- [../architecture/current-architecture.md](../architecture/current-architecture.md) for the current runtime shape.
- [../architecture/target-architecture.md](../architecture/target-architecture.md) for the layered reference model.
- [../design/editing-model.md](../design/editing-model.md) for the design rationale behind the editing surface.
- [../design/validation-framework.md](../design/validation-framework.md) for the validation-tier breakdown.
