# Bytecode validation framework

This document describes the architecture for validating pytecode's classfile emission output. The framework addresses three concerns: (1) structural validity per the JVM specification, (2) fidelity to javac-equivalent output, and (3) suitability for use in bytecode transformation toolchains. It is organized into four composable tiers.

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

That CFG-oracle work has now landed via [#17](https://github.com/smithtrenton/pytecode/issues/17). It complements rather than replaces the four emission-validation tiers in this document: the implemented oracle suite differentially validates `pytecode.analysis.build_cfg()` output before later frame, verifier, and emission layers depend on it.

## Tier 1: Binary roundtrip

The most fundamental test: read a class file, convert to the editing model, convert back to `ClassFile`, serialize to bytes, and verify the result.

```
javac output (bytes₁)
    → ClassReader.read_class()       → ClassFile₁
    → ClassModel.from_classfile()    → ClassModel
    → ClassModel.to_classfile()      → ClassFile₂
    → ClassWriter.write()            → bytes₂
    → ClassReader.read_class()       → ClassFile₃   (verify bytes₂ is parseable)
```

Three levels of fidelity apply (see [round-trip fidelity](../project/roadmap.md#9-round-trip-fidelity-and-compatibility-testing-14) in the roadmap):

- **Level A — Byte-for-byte identity**: `bytes₁ == bytes₂`. The gold standard for no-modification roundtrips. Achievable because `ConstantPoolBuilder.from_pool()` preserves original indexes.
- **Level B — Structural equivalence**: `parse(bytes₁) ≅ parse(bytes₂)`, comparing parsed structures with CP references resolved to symbolic values.
- **Level C — Semantic equivalence**: Behavior-preserving — the two class files define the same class with the same runtime behavior.

Apply roundtrip tests to every Java fixture in `tests/resources/` using parametrized pytest tests. Level A is the default expectation for unmodified roundtrips.

## Tier 2: Structural verification

Verify that emitted output satisfies JVM spec format checking (§4.8) and static constraints (§4.9.1).

**Internal verifier** (`pytecode/verify.py`): A pure-Python `ClassFileVerifier` that checks spec constraints without external tools. Key checks include:

- Magic number, version bounds, access-flag mutual exclusions
- Constant-pool well-formedness: all index references point to valid entries of the correct type
- Every non-native, non-abstract method has exactly one `Code` attribute
- `code_length > 0` and `< 65536`; all branch targets are valid instruction offsets
- All CP references in instructions are valid indexes to correct entry types
- Exception handler ranges are valid; `max_stack`/`max_locals` are consistent
- Attribute versioning constraints (e.g., type annotations require version 52+)

The verifier returns a list of structured `VerificationError` diagnostics (category, message, location) rather than raising on the first problem.

**External cross-check** via `javap -v`: Writing emitted bytes to a temp `.class` file and running `javap -v -p -c` on it provides an independent format check — `javap` errors on malformed class files.

**Optional AsmTools `jdec`**: For the deepest structural inspection, `jdec` exposes raw constant-pool structure and byte offsets. Comparing `jdec` output between our emission and javac's provides byte-level structural equivalence beyond what `javap` checks.

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

The lowering code in `pytecode/labels.py` already handles VarInsn normalization and LDC size selection. This tier verifies completeness.

**Semantic diff severities**: Differences are categorized as `error` (wrong content), `warning` (valid but non-idiomatic), or `info` (CP ordering difference). The goal is zero errors; warnings indicate optimization opportunities.

## Tier 4: JVM loading and execution

The definitive validity test: can the JVM actually load and use the class? This catches StackMapTable errors, type verification failures, and linking issues that no static checker can find.

**Verification harness**: A small `VerifierHarness.java` under `tests/resources/verifier/` that reads a `.class` file, defines it via a custom `ClassLoader` (`defineClass()`), and reports `VERIFY_OK`, `VERIFY_FAIL` (with the `VerifyError` message), or `FORMAT_FAIL` (with the `ClassFormatError` message). Run with `-Xverify:all` for strictest verification.

**Execution testing**: For fixtures with known behavior (e.g., `HelloWorld.main()` prints "Hello, World!"), verify that roundtripped output produces the correct runtime output — not just that it loads.

**StackMapTable dependency**: Tier 4 for *generated* (non-roundtrip) classes requires valid StackMapTable attributes for class files with version ≥ 50.0. This depends on Issue #10 (stack frame computation). For roundtrip classes, the original StackMapTable is preserved if code is not modified.

## Constant pool ordering strategy

CP ordering is one of the most nuanced aspects of javac compatibility. The framework uses two modes:

**Mode 1 — Preserve-on-roundtrip (default)**: When reading an existing class file and writing it back, preserve the original CP ordering. New entries are appended at the end. This is what `ConstantPoolBuilder.from_pool()` already supports. This mode is essential for bytecode transformation pipelines (ProGuard, R8, ByteBuddy all follow this pattern).

**Mode 2 — javac-compatible ordering (opt-in)**: When generating a class file from scratch, use an ordering that matches javac's allocation pattern:

1. This-class name Utf8 → `CONSTANT_Class` for `this_class`
2. Super-class name Utf8 → `CONSTANT_Class` for `super_class`
3. Interface name Utf8s → `CONSTANT_Class` entries
4. For each field (in source order): name Utf8, descriptor Utf8 → `NameAndType` → `Fieldref`
5. For each method (in source order): name Utf8, descriptor Utf8 → `NameAndType` → `Methodref`; then referenced classes/fields/methods from instructions (in instruction order)
6. Attribute-related entries (`SourceFile`, `Signature`, etc.)

This ordering is implemented in the CP builder's allocation strategy, not as a post-hoc sort. Note that javac's exact CP ordering rules are reverse-engineered from observation, not formally documented by the JVM specification.

## Validation test infrastructure

Validation tests integrate with the existing `tests/helpers.py` caching infrastructure and use pytest markers for tier-based test selection:

```
tests/
├── validation/
│   ├── __init__.py
│   ├── conftest.py              # Validation-specific fixtures and markers
│   ├── roundtrip.py             # Roundtrip utilities (Tier 1)
│   ├── verifier.py              # Internal verifier (Tier 2)
│   ├── javap_parser.py          # javap output parser
│   ├── semantic_diff.py         # Semantic comparison engine (Tier 3)
│   ├── jvm_harness.py           # JVM loading harness (Tier 4)
│   ├── test_roundtrip.py        # Tier 1 tests
│   ├── test_structural.py       # Tier 2 tests
│   ├── test_javac_comparison.py # Tier 3 tests
│   └── test_jvm_loading.py      # Tier 4 tests
├── resources/
│   ├── validation/              # Validation-specific Java fixtures
│   └── verifier/
│       └── VerifierHarness.java
└── ...existing test files...
```

Pytest markers (`@pytest.mark.tier1` through `@pytest.mark.tier4`) enable running fast roundtrip tests independently of slow subprocess-based tests. Tier 1 tests run in CI on every commit; Tiers 2–4 run on a slower schedule or on-demand.

## Validation data flow

```
 .java ──javac──▶ .class (gold) ──┐
                                   ├──▶ Tier 3 compare
 .class ──pytecode──▶ .class (ours)┘
                          │
                          ├──javap──▶ text ──▶ Tier 2 verify
                          ├──jvm────▶ load ──▶ Tier 4 verify
                          └──pytecode──▶ parse ──▶ Tier 1 roundtrip
```

## Prerequisites and phasing

- **Issue #12 (ClassWriter)**: Binary emission must exist before any validation tier can be tested.
- **Issue #10 (StackMapTable computation)**: Required for Tier 4 JVM verification of *generated* (non-roundtrip) classes.
- **Phase 1**: Implement Tier 1 (roundtrip) — cheapest, fastest, catches most serialization bugs.
- **Phase 2**: Implement Tier 2 (structural verifier + javap cross-check).
- **Phase 3**: Implement Tier 3 (javap parser + semantic diff engine).
- **Phase 4**: Implement Tier 4 (VerifierHarness + execution tests).
- **Phase 5**: CI integration with tier-based markers and caching.

## Open questions

1. **javap output stability**: `javap -v` output format varies across JDK versions. The parser should be resilient to minor format changes. Testing against a minimum JDK version (e.g., 17) is prudent.
2. **AsmTools availability**: Should AsmTools be vendored in `tools/` or remain an optional dependency? It is not in Maven Central.
3. **WIDE instruction promotion**: When `lower_code()` promotes `GOTO` to `GOTO_W` or introduces compensation branches, the output will differ from javac. These should be flagged as intentional deviations, not errors.
4. **Debug info in roundtrips**: LineNumberTable and LocalVariableTable are technically optional, but javac always emits them. Recommendation: test preservation for roundtrip; allow omission for generated classes.
5. **CP duplicate preservation**: If an input class has intentional CP duplicates (e.g., from obfuscation), roundtrip should preserve them for fidelity.
