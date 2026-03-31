# Documentation overview

This directory contains the maintained project documentation for `pytecode`.

The original implementation roadmap is complete, so the docs are organized around the current shipped surface, architecture reference, and the design rationale behind the major subsystems.

## Start here

| Need | Document |
|------|----------|
| Installation, quick examples, development commands | [../README.md](../README.md) |
| Hosted API reference | [smithtrenton.github.io/pytecode](https://smithtrenton.github.io/pytecode/) |
| Local API reference generation | `uv run python tools\generate_api_docs.py` |

## Reference docs

| Topic | Location |
|------|----------|
| Current runtime shape, public entry points, module responsibilities, data flow, and test coverage | [architecture/current-architecture.md](architecture/current-architecture.md) |
| Intended layered architecture and extension boundaries | [architecture/target-architecture.md](architecture/target-architecture.md) |
| Release-quality expectations and required validation checks | [project/quality-gates.md](project/quality-gates.md) |
| Roadmap status and delivered milestone summary | [project/roadmap.md](project/roadmap.md) |

## Design and research docs

| Topic | Location |
|------|----------|
| Editing-model evaluation, alternatives considered, and extension strategy | [design/editing-model.md](design/editing-model.md) |
| Bytecode validation framework, tier breakdown, and external-tool strategy | [design/validation-framework.md](design/validation-framework.md) |
| CFG oracle research and the reasoning behind the ASM-based differential suite | [design/cfg-validation-research.md](design/cfg-validation-research.md) |

## Current package shape

The core user-facing entry points are:

- `pytecode.ClassReader` for raw `.class` parsing.
- `pytecode.ClassWriter` for deterministic classfile emission.
- `pytecode.ClassModel` for mutable symbolic editing.
- `pytecode.JarFile` for archive reads, mutation, and safe rewrite-to-disk.

Supporting submodules provide transforms, labels, operands, analysis, hierarchy resolution, validation, descriptors, debug-info policies, and deterministic constant-pool management.

## Notes on historical docs

Some design documents intentionally preserve the reasoning used to choose the current architecture. When those docs refer to phases, issues, or alternatives, treat them as historical rationale rather than an active backlog.
