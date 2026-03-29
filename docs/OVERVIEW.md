# pytecode documentation

## Purpose

`pytecode` is a Python library for parsing, inspecting, and eventually manipulating JVM class files and bytecode.

The project goal is to provide a Python alternative to Java libraries such as ASM and BCEL. Today, the library is focused on parsing raw classfile bytes into typed Python objects. The longer-term goal is to support safe classfile transformation, verification, and emission of new `.class` files.

## Current status

### Implemented today

- Reading classfile bytes into an in-memory object model
- Parsing all 17 constant-pool entry types
- Parsing fields, methods, interfaces, and class-level metadata
- Parsing `Code` attributes and decoding all 205 standard JVM opcodes (0x00–0xCA) plus WIDE variants into typed instruction records
- Parsing 30 standard attribute types including annotations, stack map tables, module metadata, records, and permitted subclasses
- Preserving unknown or unrecognized attributes as raw bytes through `UnimplementedAttr`
- Reading JAR files and parsing every `.class` entry in them
- Descriptor and signature parsing utilities: structured field/method descriptor types, generic signature parsing (class, method, and field signatures), round-trip construction, slot-size helpers, and stricter validation of malformed internal names and signature segments — see `pytecode/descriptors.py`
- Binary writer foundation: big-endian write primitives, stateful `BytesWriter` with alignment, reserve/patch helpers for length-prefixed structures — see `pytecode/bytes_utils.py`
- Shared JVM Modified UTF-8 codec for `CONSTANT_Utf8` values — see `pytecode/modified_utf8.py`
- Unit test coverage for all attribute types, instruction operand shapes, constant-pool entries, byte utilities, class reader, JAR handling, descriptor/signature parsing, Modified UTF-8 handling, constant-pool builder hardening, mutable editing model, hierarchy resolution, and control-flow/stack simulation (1008 tests)
- Constant-pool management: `ConstantPoolBuilder` with Modified UTF-8 handling, deduplication, symbol-table lookups, compound-entry auto-creation, MethodHandle/import validation, double-slot handling, defensive-copy reads/exports, and deterministic ordering — see `pytecode/constant_pool_builder.py`
- Mutable editing model: `ClassModel`, `MethodModel`, `FieldModel`, `CodeModel` — mutable dataclasses with symbolic class/field/method references, bidirectional conversion to/from the parsed `ClassFile` model, `ConstantPoolBuilder` integration for raw operand/index passthrough, and label-aware code editing surfaces — see `pytecode/model.py`
- Label-based instruction editing ([#7](https://github.com/smithtrenton/pytecode/issues/7)): `pytecode/labels.py` introduces `Label`, symbolic branch/switch wrappers, lifted exception/debug metadata, automatic offset recalculation, switch padding recomputation, and wide-branch promotion during lowering
- Symbolic instruction operand wrappers ([#16](https://github.com/smithtrenton/pytecode/issues/16)): `pytecode/operands.py` introduces nine editing-model wrappers (`FieldInsn`, `MethodInsn`, `InterfaceMethodInsn`, `TypeInsn`, `VarInsn`, `IIncInsn`, `LdcInsn`, `InvokeDynamicInsn`, `MultiANewArrayInsn`) that replace raw constant-pool indexes and local-variable slot encodings. All wrapper types lift automatically in `model.py` during `from_classfile()` and lower back to raw instructions in `labels.py` during `to_classfile()`.
- Class hierarchy resolution ([#8](https://github.com/smithtrenton/pytecode/issues/8)): `pytecode/hierarchy.py` introduces a pluggable `ClassResolver` protocol, in-memory `MappingClassResolver`, typed resolved hierarchy snapshots (`ResolvedClass`, `ResolvedMethod`, `InheritedMethod`), and helper queries for superclass walks, supertype traversal, subtype checks, common-superclass lookup, and method-override detection.
- Control-flow analysis ([#9](https://github.com/smithtrenton/pytecode/issues/9)): `pytecode/analysis.py` introduces control-flow graph construction, verification-type-based stack/local simulation, structured merge/locals diagnostics, and optional `ClassResolver`-driven reference merging for future frame and validation work.

### Not implemented yet

- Automatic `max_stack`/`max_locals` recomputation during lowering and `StackMapTable` generation ([#10](https://github.com/smithtrenton/pytecode/issues/10))
- Full debug-info and stack-map maintenance after mutation (label rebinding is now implemented, but `StackMapTable` recomputation and higher-level policies remain future work) ([#10](https://github.com/smithtrenton/pytecode/issues/10), [#13](https://github.com/smithtrenton/pytecode/issues/13))
- Structured validation/diagnostics and binary classfile emission ([#11](https://github.com/smithtrenton/pytecode/issues/11), [#12](https://github.com/smithtrenton/pytecode/issues/12))
- Archive rewrite support for writing transformed JARs back to disk ([#15](https://github.com/smithtrenton/pytecode/issues/15))

## Documentation guide

| Topic | Location |
|-------|----------|
| Current architecture: runtime, entry points, modules, data flow, design characteristics, test coverage | [architecture/current-architecture.md](architecture/current-architecture.md) |
| Recommended target architecture: layered design, cross-cutting concerns | [architecture/target-architecture.md](architecture/target-architecture.md) |
| Editing model design rationale: candidate designs, comparison matrix, library survey | [design/editing-model.md](design/editing-model.md) |
| Bytecode validation framework: 4-tier validation, tools, CP ordering, test infrastructure | [design/validation-framework.md](design/validation-framework.md) |
| CFG validation research: ASM/BCEL/Soot comparison and recommended differential oracle design | [design/cfg-validation-research.md](design/cfg-validation-research.md) |
| Project roadmap and implementation order | [project/roadmap.md](project/roadmap.md) |
| Quality gates, non-goals, and project summary | [project/quality-gates.md](project/quality-gates.md) |
