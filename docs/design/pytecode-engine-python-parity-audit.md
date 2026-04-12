# pytecode-engine Python parity audit

## Goal

Identify which data structures are still missing in `crates\pytecode-engine` relative to the full Python implementation, with special focus on what must exist before Python can expose Rust-native attributes directly instead of re-decoding `Unknown` payloads.

## Executive summary

`pytecode-engine` is already close to parity for:

- raw classfile container types (`ClassFile`, `FieldInfo`, `MethodInfo`),
- constant-pool entry payloads,
- symbolic edit-model core (`ClassModel`, `MethodModel`, `FieldModel`, `CodeModel`),
- hierarchy and verifier result payloads.

Main remaining gap is raw attribute coverage.

Python models essentially the full JVM attribute graph in `pytecode\classfile\attributes.py:101-854` and parses it in `pytecode\classfile\reader.py:393-736`.
Rust raw attributes in `crates\pytecode-engine\src\raw\attributes.rs:4-75` currently model only:

- `ConstantValue`,
- `Signature`,
- `SourceFile`,
- `SourceDebugExtension`,
- `Exceptions`,
- `Code`,
- `Unknown`.

`crates\pytecode-engine\src\reader.rs:150-215,252-285` and `crates\pytecode-engine\src\writer.rs:193-204` match that narrow set. Everything else is preserved as opaque bytes.

Historically that mismatch forced more Python-side interpretation around unsupported Rust `Unknown` attrs. The current bridge exposes those as `UnimplementedAttr` wrappers instead of reparsing them through a legacy Python reader, so the remaining gap is representational completeness rather than an active runtime fallback path.

## Current parity snapshot

| Area | Python surface | Rust surface | Status | Notes |
| --- | --- | --- | --- | --- |
| Raw classfile containers | `pytecode\classfile\info.py:19-70` | `crates\pytecode-engine\src\raw\info.rs:6-32` | Strong | Core container structs exist on both sides. |
| Constant-pool entries | `pytecode\classfile\constant_pool.py:31-169` | `crates\pytecode-engine\src\raw\constant_pool.rs:3-163` | Strong | Entry payloads are present; only Python base/helper types differ. |
| Raw instructions | `pytecode\classfile\instructions.py:37-482` | `crates\pytecode-engine\src\raw\instructions.rs:34-212` | Mostly equivalent | Rust uses an enum-based representation instead of one Python dataclass per operand form. |
| Raw attributes | `pytecode\classfile\attributes.py:101-854` | `crates\pytecode-engine\src\raw\attributes.rs:4-75` | Major gap | Most JVM attrs and their nested union types are missing. |
| Raw constants/enums | `pytecode\classfile\constants.py:26-209` | `crates\pytecode-engine\src\constants.rs:1-63` | Major gap | Rust only has class/field/method access flags plus `MAGIC`. |
| Edit model core | `pytecode\edit\model.py:112-186` | `crates\pytecode-engine\src\model\mod.rs:30-79` | Strong | Same core models exist. |
| Edit labels/helpers | `pytecode\edit\labels.py:215-390` | `crates\pytecode-engine\src\model\labels.rs:13-123` | Good | Core label structures exist; `LabelResolution` helper is Python-only. |
| Edit operands | `pytecode\edit\operands.py:224-656` | `crates\pytecode-engine\src\model\operands.rs:5-91` | Good but modeled differently | Rust collapses Python `Ldc*` leaf classes into `LdcValue`. |
| Hierarchy analysis | `pytecode\analysis\hierarchy.py:59-195` | `crates\pytecode-engine\src\analysis\hierarchy.rs:12-113` | Strong | Main result structures exist. |
| Verify analysis | `pytecode\analysis\verify.py:103-208` | `crates\pytecode-engine\src\analysis\verify.rs:8-40` | Strong | Main diagnostic payloads exist. |

## What is actually blocking direct Rust attribute exposure

Two engine-level gaps matter immediately.

### 1. Missing raw attribute structs/enums

Rust currently has no typed representation for most attrs that Python exposes and many tests rely on. Missing families include:

1. Stack-map data:
   - `VerificationTypeInfo`
   - `TopVariableInfo`
   - `IntegerVariableInfo`
   - `FloatVariableInfo`
   - `DoubleVariableInfo`
   - `LongVariableInfo`
   - `NullVariableInfo`
   - `UninitializedThisVariableInfo`
   - `ObjectVariableInfo`
   - `UninitializedVariableInfo`
   - `StackMapFrameInfo`
   - `SameFrameInfo`
   - `SameLocals1StackItemFrameInfo`
   - `SameLocals1StackItemFrameExtendedInfo`
   - `ChopFrameInfo`
   - `SameFrameExtendedInfo`
   - `AppendFrameInfo`
   - `FullFrameInfo`
   - `StackMapTableAttr`
2. Class/member metadata attrs:
   - `InnerClassInfo`
   - `InnerClassesAttr`
   - `EnclosingMethodAttr`
   - `SyntheticAttr`
   - `DeprecatedAttr`
   - `MethodParameterInfo`
   - `MethodParametersAttr`
   - `ModuleMainClassAttr`
   - `NestHostAttr`
   - `NestMembersAttr`
   - `RecordComponentInfo`
   - `RecordAttr`
   - `PermittedSubclassesAttr`
3. Debug attrs:
   - `LineNumberInfo`
   - `LineNumberTableAttr`
   - `LocalVariableInfo`
   - `LocalVariableTableAttr`
   - `LocalVariableTypeInfo`
   - `LocalVariableTypeTableAttr`
4. Annotation attrs and element-value graph:
   - `ConstValueInfo`
   - `EnumConstantValueInfo`
   - `ClassInfoValueInfo`
   - `ArrayValueInfo`
   - `ElementValueInfo`
   - `ElementValuePairInfo`
   - `AnnotationInfo`
   - `RuntimeVisibleAnnotationsAttr`
   - `RuntimeInvisibleAnnotationsAttr`
   - `ParameterAnnotationInfo`
   - `RuntimeVisibleParameterAnnotationsAttr`
   - `RuntimeInvisibleParameterAnnotationsAttr`
   - `AnnotationDefaultAttr`
5. Type-annotation target graph:
   - `TargetInfo`
   - `TypeParameterTargetInfo`
   - `SupertypeTargetInfo`
   - `TypeParameterBoundTargetInfo`
   - `EmptyTargetInfo`
   - `FormalParameterTargetInfo`
   - `ThrowsTargetInfo`
   - `TableInfo`
   - `LocalvarTargetInfo`
   - `CatchTargetInfo`
   - `OffsetTargetInfo`
   - `TypeArgumentTargetInfo`
   - `PathInfo`
   - `TypePathInfo`
   - `TypeAnnotationInfo`
   - `RuntimeTypeAnnotationsAttr`
   - `RuntimeVisibleTypeAnnotationsAttr`
   - `RuntimeInvisibleTypeAnnotationsAttr`
6. Bootstrap and module attrs:
   - `BootstrapMethodInfo`
   - `BootstrapMethodsAttr`
   - `RequiresInfo`
   - `ExportInfo`
   - `OpensInfo`
   - `ProvidesInfo`
   - `ModuleAttr`
   - `ModulePackagesAttr`

Immediate consequence:

- `InnerClasses`, `LineNumberTable`, `StackMapTable`, runtime annotations, module attrs, record attrs, and similar data still round-trip through Rust only as `Unknown`.
- Python wrapper therefore cannot expose Rust-native objects for those attrs without losing structured access.

### 2. Missing constants/enums required by those attrs

Python defines the full supporting enum/flag set in `pytecode\classfile\constants.py:26-209`.
Rust constants currently expose only:

- `ClassAccessFlags`
- `FieldAccessFlags`
- `MethodAccessFlags`
- `MAGIC`

Missing Rust-side supporting types include:

- `NestedClassAccessFlag`
- `MethodParameterAccessFlag`
- `ModuleAccessFlag`
- `ModuleRequiresAccessFlag`
- `ModuleExportsAccessFlag`
- `ModuleOpensAccessFlag`
- `TargetType`
- `TargetInfoType`
- `TypePathKind`
- `VerificationType`

These are prerequisites for a typed Rust model of stack-map, module, nested-class, method-parameter, and type-annotation attrs.

## Reader/writer work required in pytecode-engine

Adding structs alone is not enough. The parser and emitter must learn them too.

### Reader changes

`crates\pytecode-engine\src\reader.rs:150-215,252-285` currently recognizes only a small attr subset and treats the rest as `AttributeInfo::Unknown`.

Necessary changes:

1. Add parser branches for every missing top-level attr currently implemented in Python `read_attribute(...)`.
2. Add recursive parsers for nested unions:
   - `verification_type_info`
   - stack-map frames
   - annotation `element_value`
   - type-annotation `target_info`
   - type paths
   - module tables
   - record component attrs
3. Reuse those same parsers for nested attrs inside `Code`, `Record`, and other attrs with embedded attribute lists.
4. Add structured validation errors for malformed stack-map, annotation, type-path, and module payloads instead of falling back to `Unknown`.

### Writer changes

`crates\pytecode-engine\src\writer.rs:193-204` currently emits only the same narrow attr set plus opaque unknown payloads.

Necessary changes:

1. Add emission support for every newly added attr variant.
2. Serialize nested union payloads in spec order.
3. Preserve exact byte stability for unchanged classfiles, including:
   - stack-map tables,
   - debug tables,
   - annotations and type annotations,
   - module metadata,
   - record components,
   - permitted-subclass data.
4. Extend round-trip tests so newly typed attrs do not regress into byte drift.

## Areas that are not true engine gaps, but still matter for wrapper parity

These differences exist, but they are mostly representation mismatches rather than missing engine capability.

### Raw instruction layer

Python now exposes the live raw reader shape directly: `_rust.InsnInfo` is re-exported from `pytecode.classfile`, and its opcode helpers live in `pytecode\classfile\bytecode.py`.
Rust stores instructions in one enum with specialized payload structs in `crates\pytecode-engine\src\raw\instructions.rs:34-212`.

This is now mostly aligned: the raw binding surface is one instruction object plus enums, rather than a parallel Python-only dataclass hierarchy.

### Edit operand layer

Python has distinct `LdcInt`, `LdcFloat`, `LdcLong`, `LdcDouble`, `LdcString`, `LdcClass`, `LdcMethodType`, `LdcMethodHandle`, and `LdcDynamic` leaf classes in `pytecode\edit\operands.py:224-342`.
Rust collapses those into `LdcValue` plus `LdcInsn` in `crates\pytecode-engine\src\model\operands.rs:60-91`.

Again:

- engine already has semantic coverage,
- but direct Python wrapper parity would either need:
  - Rust-side mirror types, or
  - a consciously non-1:1 Python facade.

### Edit labels helper

Python has `LabelResolution` in `pytecode\edit\labels.py:389-390`.
Rust `crates\pytecode-engine\src\model\labels.rs:13-123` has label-bearing code items but no equivalent public helper type.

This is low priority unless the Python wrapper intends to expose that exact helper.

### Analysis exceptions/helpers

Rust has the core hierarchy and verifier payload structs:

- `ResolvedMethod`
- `ResolvedClass`
- `InheritedMethod`
- `ClassResolver`
- `MappingClassResolver`
- `Severity`
- `Category`
- `Location`
- `Diagnostic`

Python additionally has facade-only exception/helper types such as:

- `HierarchyError`
- `UnresolvedClassError`
- `HierarchyCycleError`
- `FailFastError`

Those are wrapper/facade parity concerns, not blockers for attribute exposure.

## Recommended implementation order

If goal is "Python can expose Rust attrs directly and delete Python-side attr re-decoding", engine work should happen in this order:

1. **Add missing constants/enums in `crates\pytecode-engine\src\constants.rs`.**
   - This unlocks typed stack-map, nested-class, module, method-parameter, and type-annotation payloads.
2. **Expand `crates\pytecode-engine\src\raw\attributes.rs` to full JVM attr coverage.**
   - Prefer spec-shaped enums/structs over generic maps.
3. **Teach `crates\pytecode-engine\src\reader.rs` to parse all those attrs recursively.**
4. **Teach `crates\pytecode-engine\src\writer.rs` to emit all those attrs.**
5. **Add raw round-trip tests using existing Python-backed fixtures for each new attr family.**
6. **Only then simplify `pytecode\classfile\_rust_bridge.py` and `crates\pytecode-python\src\lib.rs`.**
   - At that point Python can bind typed Rust attrs instead of invoking Python re-decode on `Unknown`.

## Recommended test matrix for the engine work

At minimum add focused Rust tests covering:

1. `StackMapTable` round-trip with non-empty frames.
2. `InnerClasses` round-trip.
3. `LineNumberTable`, `LocalVariableTable`, and `LocalVariableTypeTable` round-trip.
4. Runtime-visible/invisible annotations on class, field, method, and parameter targets.
5. Runtime-visible/invisible type annotations for each supported `target_info` family.
6. `BootstrapMethods` round-trip with `InvokeDynamic` and `Dynamic` constant-pool references.
7. Module attrs (`Module`, `ModulePackages`, `ModuleMainClass`) round-trip.
8. `Record` and `PermittedSubclasses` round-trip.
9. Invalid nested payloads producing structured parse errors, not silent `Unknown`.

## Bottom line

For direct Rust-native Python attr exposure, `pytecode-engine` does **not** need a wholesale rewrite of its classfile core.
It mainly needs:

- full raw attribute data structures,
- the supporting constants/enums those attrs depend on,
- matching parser/writer support.

Everything else is secondary.

The bridge/re-decode path exists today because Rust currently has a strong classfile shell and instruction/core model, but only a thin typed attribute interior.
