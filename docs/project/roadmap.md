# Roadmap status

The original roadmap is complete. This file now serves as a compact record of the delivered milestones and the few areas intentionally left outside the committed scope.

## Current status

`pytecode` now ships the full set of capabilities that the original roadmap was organized around:

1. A supported API for parsing and manipulating classfiles.
2. Analysis and frame recomputation support.
3. Validation and deterministic emission back to `.class` bytes and JAR archives.

## Delivered milestones

### Parsing and representation foundation

- Parser hardening for classfile structure, attributes, and instructions.
- Full constant-pool coverage, including Modified UTF-8 handling.
- Descriptor and signature parsing utilities.
- Binary read and write primitives in `pytecode._internal.bytes_utils`.
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
- Four validation tiers covering byte-for-byte roundtrip, structural verification, semantic diffing, and JVM verification.
- ASM-backed CFG differential validation for `build_cfg()`.

### Packaging and documentation

- Deterministic classfile emission via `ClassWriter.write()` and `ClassModel.to_bytes()`.
- Archive mutation and safe rewrite-to-disk in `pytecode.archive`.
- Generated API reference coverage enforced by tests and `tools/generate_api_docs.py --check`.
- Release automation aligned with immutable `v<version>` tags and PyPI Trusted Publishing.

## Intentionally uncommitted areas

These are not active roadmap items, but they remain plausible future directions if concrete use cases appear:

- an opt-in javac-compatible constant-pool ordering mode for from-scratch generation
- higher-level instruction pattern matching layered on top of the current matcher DSL
- an optional visitor or streaming API if real throughput or memory-pressure workloads justify a second traversal model

## Related docs

- [../architecture/current-architecture.md](../architecture/current-architecture.md) for the current runtime shape.
- [../architecture/target-architecture.md](../architecture/target-architecture.md) for the layered reference model.
- [../design/editing-model.md](../design/editing-model.md) for the design rationale behind the editing surface.
- [../design/validation-framework.md](../design/validation-framework.md) for the validation-tier breakdown.
- [../experiments/rust-migration-overview.md](../experiments/rust-migration-overview.md) for the active Rust-backend experiment phases, benchmarks, and next-step guidance.
