# Documentation overview

This directory contains the maintained project documentation for `pytecode`.
The docs here describe the current Rust-backed package surface, architecture,
validation model, and operational guidance.

## Start here

| Need | Document |
|------|----------|
| Installation, quick examples, development commands | [../README.md](../README.md) |
| Hosted API reference | [smithtrenton.github.io/pytecode](https://smithtrenton.github.io/pytecode/) |
| Local API reference generation | `uv run python tools/generate_api_docs.py` |

## Reference docs

| Topic | Location |
|------|----------|
| Current runtime shape, public entry points, module responsibilities, data flow, and test coverage | [architecture/current-architecture.md](architecture/current-architecture.md) |
| Intended layered architecture and extension boundaries | [architecture/target-architecture.md](architecture/target-architecture.md) |
| Benchmarking guidance and reproduction commands | [benchmarks.md](benchmarks.md) |
| Release-quality expectations and required validation checks | [project/quality-gates.md](project/quality-gates.md) |
| Roadmap status and delivered milestone summary | [project/roadmap.md](project/roadmap.md) |
| Rust JVMS 25 audit, remediation status, and scoped conformance notes | [project/rust-jvms-25-audit.md](project/rust-jvms-25-audit.md) |

## Design and research docs

| Topic | Location |
|------|----------|
| Current public surface and design contracts | [design/pytecode-design.md](design/pytecode-design.md) |
| Editing-model evaluation, alternatives considered, and extension strategy | [design/editing-model.md](design/editing-model.md) |
| Bytecode validation framework, tier breakdown, and external-tool strategy | [design/validation-framework.md](design/validation-framework.md) |
| CFG oracle research and the reasoning behind the ASM-based differential suite | [design/cfg-validation-research.md](design/cfg-validation-research.md) |

## Current package shape

The core user-facing entry points are:

- `pytecode.ClassReader` for raw `.class` parsing.
- `pytecode.ClassWriter` for deterministic classfile emission.
- `pytecode.ClassModel` for mutable symbolic editing.
- `pytecode.JarFile` for archive reads, mutation, and safe rewrite-to-disk.

Supporting submodules provide transforms, analysis, hierarchy resolution,
validation, debug-info policies, and raw classfile helpers through
`pytecode.classfile` and `pytecode.model`.
