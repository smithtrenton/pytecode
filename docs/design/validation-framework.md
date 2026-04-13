# Bytecode validation framework

This document describes the validation architecture used by pytecode's classfile
emission and transformation workflow. The active repository today centers on
Rust workspace tests, Python API tests, and `javap` semantic-diff utilities.
The four-tier model below remains the right conceptual framework, but not every
tier currently has a dedicated standalone test module.

## Validation tiers

```
                            ┌───────────────────────────────────┐
                            │  Tier 4: JVM Loading & Execution  │
                            │  (Definitive: can the JVM load    │
                            │   and run this class?)            │
                            └───────────────┬───────────────────┘
                                            │
                            ┌───────────────┴───────────────────┐
                            │  Tier 3: Semantic Comparison       │
                            │  (javac equivalence: CP ordering,  │
                            │   instruction selection, types)    │
                            └───────────────┬───────────────────┘
                                            │
                            ┌───────────────┴───────────────────┐
                            │  Tier 2: Structural Verification   │
                            │  (JVM spec compliance: format,     │
                            │   static constraints, attributes)  │
                            └───────────────┬───────────────────┘
                                            │
                            ┌───────────────┴───────────────────┐
                            │  Tier 1: Binary Roundtrip          │
                            │  (read → model → write → read,    │
                            │   byte equality or semantic eq)    │
                            └───────────────────────────────────┘
```

Each tier catches different classes of bugs and is independently testable.

The sections below describe the architecture and point at the current
repository surfaces that cover each tier.

## External tool landscape

The framework leverages existing JDK and OpenJDK tools rather than reimplementing verification logic:

| Tool | Purpose | Usage | Limitations |
|------|---------|-------|-------------|
| **javac** | Reference compiler | Gold-standard `.class` files for comparison | Only produces what source code can express |
| **javap** | Disassembler | `javap -v -p -c` for structured dump; errors on malformed class files | Text output requires parsing; format varies across JDK versions |
| **java** (JVM) | Class loader + verifier | `-Xverify:all` triggers full verification; definitive validity test | Requires a classloader harness for isolated testing |
| **AsmTools** (OpenJDK) | Structural tools | `jdis`/`jdec` are reflexive (disassemble → reassemble = identical bytes); `jdec` exposes raw CP structure | Not in Maven Central; must be built from source or vendored |
| **ASM CheckClassAdapter** | Visitor-level validation | `verify()` does basic data-flow checking via `Analyzer` + `BasicVerifier` | Not identical to JVM verification |
| **JDK ClassFile API** (JEP 457) | Standard classfile library | Additional parser to validate output (JDK 22+ preview, JDK 24 finalized) | Requires modern JDK |

AsmTools' reflexive property is key: if `jdec(our_output)` matches `jdec(javac_output)`, the files are structurally equivalent at the byte level.

For control-flow graph validation specifically, see [CFG validation research](cfg-validation-research.md) for a source-backed comparison of ASM, BCEL, and Soot and a recommended differential-testing design based on ASM's `Analyzer`.

That CFG-oracle work did not ship as part of the current default suite. Keep the
research in [CFG validation research](cfg-validation-research.md) as design
background for any future JVM-backed differential testing.

## Tier 1: Binary roundtrip

The most fundamental test: read a class file, convert to the editing model, convert back to `ClassFile`, serialize to bytes, and verify the result.

```
javac output (bytes₁)
    → ClassReader.from_bytes()       → ClassFile₁
    → ClassModel.from_bytes()        → ClassModel
    → ClassModel.to_classfile()      → ClassFile₂
    → ClassWriter.write()            → bytes₂
    → ClassReader.from_bytes()       → ClassFile₃   (verify bytes₂ is parseable)
```

Three levels of fidelity apply:

- **Level A — Byte-for-byte identity**: `bytes₁ == bytes₂`. The gold standard for no-modification roundtrips. Achievable because `ConstantPoolBuilder.from_pool()` preserves original indexes.
- **Level B — Structural equivalence**: `parse(bytes₁) ≅ parse(bytes₂)`, comparing parsed structures with CP references resolved to symbolic values.
- **Level C — Semantic equivalence**: Behavior-preserving — the two class files define the same class with the same runtime behavior.

Level A is the default expectation for unmodified roundtrips.

The current repository covers Tier 1 primarily through
`crates/pytecode-engine/tests/raw_roundtrip.rs`, with Python-facing roundtrip
coverage in `tests/test_rust_bindings.py`.

## Tier 2: Structural verification

Verify that emitted output satisfies JVM spec format checking (§4.8) and static constraints (§4.9.1).

**Internal verifier** (`pytecode.analysis.verify`): `verify_classfile()` and `verify_classmodel()` perform spec checks through the Rust-backed validation layer without external tools. Key checks include:

- Magic number, version bounds, access-flag mutual exclusions
- Constant-pool well-formedness: all index references point to valid entries of the correct type
- Every non-native, non-abstract method has exactly one `Code` attribute
- `code_length > 0` and `< 65536`; all branch targets are valid instruction offsets
- All CP references in instructions are valid indexes to correct entry types
- Exception handler ranges are valid; `max_stack`/`max_locals` are consistent
- Attribute versioning constraints (e.g., type annotations require version 52+)

The verifier returns a list of structured `Diagnostic` findings (severity, category, message, location) rather than raising on the first problem.

**External cross-check** via `javap -v`: Writing emitted bytes to a temp `.class` file and running `javap -v -p -c` on it provides an independent format check — `javap` errors on malformed class files.

**Optional AsmTools `jdec`**: For the deepest structural inspection, `jdec` exposes raw constant-pool structure and byte offsets. Comparing `jdec` output between our emission and javac's provides byte-level structural equivalence beyond what `javap` checks.

Today the most direct Tier 2 coverage lives in
`crates/pytecode-engine/tests/verifier.rs` plus the Python-facing verifier
checks in `tests/test_rust_bindings.py` and `tests/test_jar.py`.

## Tier 3: Semantic comparison with javac

Compare emitted output against javac's output at a semantic level, identifying differences in CP ordering, instruction selection, and encoding choices.

**javap structured diff**: Parse `javap -v -p -c` output from both our class file and javac's class file into structured records (class info, constant pool, fields, methods with disassembled code), then perform a CP-aware semantic diff. Two class files are semantically equivalent even if their constant pools have different ordering, as long as all references resolve to the same values.

**Instruction selection analysis**: javac makes specific encoding choices that pytecode should match:

| Pattern | Idiomatic (javac) | Non-idiomatic |
|---------|-------------------|---------------|
| Push int −1..5 | `iconst_m1`..`iconst_5` | `bipush`, `sipush`, `ldc` |
| Push int −128..127 | `bipush` | `sipush`, `ldc` |
| Push int −32768..32767 | `sipush` | `ldc` |
| Load/store local 0..3 | `iload_0`..`astore_3` | `iload 0`..`astore 3` |
| CP index ≤ 255 | `ldc` | `ldc_w` |
| Long 0, 1 | `lconst_0`, `lconst_1` | `ldc2_w` |
| Float 0.0, 1.0, 2.0 | `fconst_0/1/2` | `ldc` |
| Double 0.0, 1.0 | `dconst_0/1` | `ldc2_w` |

The lowering pipeline already handles VarInsn normalization and LDC size selection. This tier verifies completeness.

**Semantic diff severities**: Differences are categorized as `error` (wrong content), `warning` (valid but non-idiomatic), or `info` (CP ordering difference). The goal is zero errors; warnings indicate optimization opportunities.

The current Tier 3 tooling lives in `tests/javap_parser.py` and
`tests/test_javap_parser.py`.

## Tier 4: JVM loading and execution

The definitive validity test: can the JVM actually load and use the class? This catches StackMapTable errors, type verification failures, and linking issues that no static checker can find.

The current repository does not ship a dedicated JVM verification harness as
part of the default test suite. Treat this tier as an optional extension point
when future changes justify an explicit load-and-execute lane.

**Execution testing**: For fixtures with known behavior (for example a simple
`HelloWorld.main()`), verify that roundtripped output produces the correct
runtime output — not just that it loads.

**StackMapTable dependency**: Tier 4 for *generated* (non-roundtrip) classes requires valid StackMapTable attributes for class files with version ≥ 50.0. This depends on Issue #10 (stack frame computation). For roundtrip classes, the original StackMapTable is preserved if code is not modified.

## Constant pool ordering strategy

CP ordering is one of the most nuanced aspects of deterministic emission. The framework uses two modes:

**Mode 1 — Preserve-on-roundtrip (default)**: When reading an existing class file and writing it back, preserve the original CP ordering. New entries are appended at the end. This is what `ConstantPoolBuilder.from_pool()` already supports. This mode is essential for bytecode transformation pipelines (ProGuard, R8, ByteBuddy all follow this pattern).

**Mode 2 — javac-style ordering (not currently implemented)**: A later mode could generate a class file from scratch using an ordering that matches javac's allocation pattern:

1. This-class name Utf8 → `CONSTANT_Class` for `this_class`
2. Super-class name Utf8 → `CONSTANT_Class` for `super_class`
3. Interface name Utf8s → `CONSTANT_Class` entries
4. For each field (in source order): name Utf8, descriptor Utf8 → `NameAndType` → `Fieldref`
5. For each method (in source order): name Utf8, descriptor Utf8 → `NameAndType` → `Methodref`; then referenced classes/fields/methods from instructions (in instruction order)
6. Attribute-related entries (`SourceFile`, `Signature`, etc.)

This mode is not implemented today; current from-scratch generation uses the builder's deterministic insertion order. Note that javac's exact CP ordering rules are reverse-engineered from observation, not formally documented by the JVM specification.

## Validation test infrastructure

Validation coverage is split across focused Rust and Python modules:

```
crates/pytecode-engine/tests/raw_roundtrip.rs
crates/pytecode-engine/tests/verifier.rs
crates/pytecode-engine/tests/analysis.rs
crates/pytecode-engine/tests/model.rs
crates/pytecode-engine/tests/transform.rs
crates/pytecode-archive/tests/jar.rs
crates/pytecode-cli/tests/*.rs
tests/test_rust_bindings.py
tests/test_rust_transforms.py
tests/test_jar.py
tests/javap_parser.py
tests/test_javap_parser.py
tests/test_api_docs.py
```

`tests/helpers.py` still provides shared fixture compilation and caching for the
Python-facing suite.

## Validation data flow

```
 .java ──javac──▶ .class (gold) ──┐
                                   ├──▶ Tier 3 compare
  .class ──pytecode──▶ .class (ours)┘
                           │
                           ├──javap──▶ text ──▶ Tier 2 verify
                           ├──jvm────▶ load ──▶ Tier 4 verify (optional)
                           └──pytecode──▶ parse ──▶ Tier 1 roundtrip
```

## Implementation status

- **Issue #12 (ClassWriter)**: Implemented.
- **Issue #10 (StackMapTable computation)**: Implemented.
- **Issue #14 (round-trip and verifier-focused validation)**: Landed as a mix of Rust workspace coverage, Python API tests, and `javap` semantic-diff tooling rather than the older Python-only module split.
- **Current module split**: Tier 1 and Tier 2 behavior are covered primarily in `crates/pytecode-engine/tests/*.rs`, archive/CLI workflows in `crates/pytecode-archive/tests/jar.rs` and `crates/pytecode-cli/tests/*.rs`, and Python-facing behavior in `tests/test_rust_bindings.py`, `tests/test_rust_transforms.py`, `tests/test_jar.py`, `tests/javap_parser.py`, and `tests/test_javap_parser.py`.
- **Fixture selection**: shared fixture compilation and caching live in `tests/helpers.py` and the Rust fixture helpers under `crates/pytecode-engine/src/fixtures.rs`.

## Known considerations

1. **javap output stability**: `javap -v` output format varies across JDK versions, so the parser should stay resilient to minor formatting changes.
2. **AsmTools availability**: AsmTools remains optional because it is not distributed through Maven Central and is not required for the default test flow.
3. **WIDE instruction promotion**: When `lower_code()` promotes `GOTO` to `GOTO_W` or introduces compensation branches, the output will diverge from javac intentionally and should not be treated as a correctness failure.
4. **Debug info in roundtrips**: LineNumberTable and LocalVariableTable are technically optional, but roundtrip flows should preserve them when present unless callers explicitly choose a stripping policy.
5. **CP duplicate preservation**: If an input class has intentional constant-pool duplicates, roundtrip behavior should preserve them for fidelity.
