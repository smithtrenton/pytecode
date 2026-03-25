# Design Options for Issue #6: Mutable Editing Model and Public Transformation API

## Executive Summary

Issue [#6](https://github.com/smithtrenton/pytecode/issues/6) asks pytecode to choose an API design pattern for its classfile manipulation layer — the core missing capability that elevates the project from a parser into a Python alternative to ASM/BCEL. Four candidate designs are identified in the issue and ARCHITECTURE.md: **(A)** direct mutable dataclasses, **(B)** builder objects (BCEL-style), **(C)** visitor/transformer pattern (ASM-style), and **(D)** pass pipelines. A fifth hybrid option — **(E)** dual tree+visitor — is also called out as worth considering. This document analyzes each design against pytecode's existing codebase, its planned feature roadmap (labels, constant-pool management, frame computation, emission), and practical Python ergonomics.

---

## Context: What Exists Today

pytecode currently has a **read-only, spec-faithful dataclass model**[^1]. The parsed output is a tree of frozen-style (though not actually frozen) `@dataclass` objects:

- `ClassFile` holds lists of `FieldInfo`, `MethodInfo`, and `AttributeInfo`[^2]
- Instructions are typed `InsnInfo` subclasses carrying raw `bytecode_offset` and constant-pool indexes[^3]
- The constant pool is `list[ConstantPoolInfo | None]` indexed by position[^4]
- Branch targets are raw byte offsets, not labels[^5]
- Exception handler ranges (`ExceptionInfo`) use `start_pc`/`end_pc`/`handler_pc` byte offsets[^6]

The read model is **tightly spec-shaped**: field names mirror the JVM classfile specification, and values are raw indexes into the constant pool. This is excellent for lossless parsing but hostile to safe editing — users would have to manually manage constant-pool indexes, recalculate branch offsets, and handle WIDE instruction expansion.

The planned features that the editing model must support include:

| Feature | Issue | Interaction with editing model |
|---------|-------|-------------------------------|
| Constant-pool management | [#5](https://github.com/smithtrenton/pytecode/issues/5) | Must create/deduplicate/reindex CP entries during edits |
| Label-based branches | [#7](https://github.com/smithtrenton/pytecode/issues/7) | Editing model must use labels, not raw offsets |
| Class hierarchy resolution | [#8](https://github.com/smithtrenton/pytecode/issues/8) | Frame computation needs hierarchy info during transformations |
| Control-flow analysis | [#9](https://github.com/smithtrenton/pytecode/issues/9) | Stack simulation and CFG depend on instruction representation |
| Frame recomputation | [#10](https://github.com/smithtrenton/pytecode/issues/10) | Must run after instruction edits |
| Validation | [#11](https://github.com/smithtrenton/pytecode/issues/11) | Validates the editing model's output before emission |
| Emission | [#12](https://github.com/smithtrenton/pytecode/issues/12) | Serializes the editing model back to bytes |
| Debug info management | [#13](https://github.com/smithtrenton/pytecode/issues/13) | Debug attrs must track label positions through edits |

---

## Design A: Direct Mutable Dataclasses

### Description

Extend the existing `info.py` dataclasses (or create parallel ones) to be directly mutable. Users edit classes by mutating fields in-place: appending to `methods`, modifying `access_flags`, replacing instructions in the `CodeAttr.code` list, etc. The constant pool, instruction list, and attributes are all plain mutable Python collections.

A "symbolic" layer would replace raw indexes with resolved references (e.g., `method.name: str` instead of `method.name_index: int`), and a lowering step would convert back to indexed form before emission.

### Sketch

```python
@dataclass
class EditableClass:
    version: tuple[int, int]
    access_flags: ClassAccessFlag
    name: str                          # resolved, not an index
    super_name: str | None
    interfaces: list[str]
    fields: list[EditableField]
    methods: list[EditableMethod]
    attributes: list[AttributeInfo]
    constant_pool: ConstantPoolBuilder  # managed CP

@dataclass
class EditableMethod:
    access_flags: MethodAccessFlag
    name: str
    descriptor: str
    code: InstructionList | None       # label-aware mutable list
    exception_handlers: list[ExceptionHandler]  # label-based ranges
    attributes: list[AttributeInfo]

# Usage:
cls = EditableClass.from_classfile(reader.class_info)
cls.methods.append(EditableMethod(...))
cls.methods[0].code.insert_before(label, Insn.ALOAD(0))
raw = cls.to_bytes()
```

### Advantages

| # | Advantage | Detail |
|---|-----------|--------|
| 1 | **Pythonic and discoverable** | Python developers expect mutable objects with tab-completable attributes. No need to learn visitor or builder patterns. |
| 2 | **Minimal conceptual overhead** | The model is "a class has methods, methods have code" — the same mental model as the JVM spec. |
| 3 | **Natural fit for the existing codebase** | pytecode already uses `@dataclass` throughout[^2][^3][^4]. Mutable dataclasses are a direct evolution of the existing read model. |
| 4 | **Easy random access** | Users can index into `methods[2].code[15]` directly. Finding and replacing specific instructions is trivial list manipulation. |
| 5 | **Straightforward serialization** | A `to_bytes()` method walks the tree and emits. No intermediate event stream needed. |
| 6 | **Low barrier to entry** | New contributors only need to understand Python dataclasses. |
| 7 | **Good for small, surgical edits** | Changing one field, adding one method, or patching one instruction doesn't require a full pipeline. |

### Disadvantages

| # | Disadvantage | Detail |
|---|--------------|--------|
| 1 | **Full materialization required** | The entire class must be in memory to edit. For large JARs with many classes, this is expensive if you only need to touch a few methods. |
| 2 | **No streaming capability** | Cannot process a classfile in a single pass without building the full tree. ASM's visitor model is specifically designed to avoid this. |
| 3 | **Invariant maintenance is the user's problem** | After mutation, the tree may be in an inconsistent state (e.g., a branch target label that doesn't exist, a CP index that's stale). Validation must be deferred to emission or an explicit `validate()` call. |
| 4 | **Instruction list coherence** | Inserting/removing instructions invalidates offsets for every subsequent instruction and all branch targets. An `InstructionList` abstraction is needed to manage this (essentially reinventing a linked list or gap buffer). |
| 5 | **Difficult to compose transformations** | Applying multiple independent transformations to the same class requires careful ordering and conflict resolution. No built-in pipeline mechanism. |
| 6 | **Risk of partial edits** | Users can leave the tree in a half-modified state. Unlike a builder (which produces a finished product) or a visitor (which processes sequentially), there's no "done" signal. |
| 7 | **Two model problem** | Either the read model (`info.ClassFile`) and edit model (`EditableClass`) are separate (requiring conversion), or the read model is modified to be editable (coupling parsing to editing concerns). |

### Best suited for

Projects where the primary use case is **interactive, exploratory editing** — load a class, poke at it, save it. Also good when **simplicity is paramount** and the user base is Python-centric.

---

## Design B: Builder Objects (BCEL-style)

### Description

Provide `ClassBuilder`, `MethodBuilder`, `FieldBuilder`, and `InstructionListBuilder` objects that accumulate state and produce immutable or finalized output. Builders manage constant-pool entries, labels, and offsets internally. The user calls methods like `add_method()`, `append_instruction()`, and `build()`.

This is the pattern used by Apache BCEL's `ClassGen`, `MethodGen`, `InstructionFactory`, and `InstructionList`.

### Sketch

```python
cb = ClassBuilder(name="com/example/Hello", super_name="java/lang/Object")
cb.set_access(ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER)

mb = cb.add_method("greet", "()V", MethodAccessFlag.PUBLIC)
il = mb.instruction_list()
il.append(Insn.GETSTATIC("java/lang/System", "out", "Ljava/io/PrintStream;"))
il.append(Insn.LDC("Hello, world!"))
il.append(Insn.INVOKEVIRTUAL("java/io/PrintStream", "println", "(Ljava/lang/String;)V"))
il.append(Insn.RETURN())
mb.set_max_stack(2)
mb.set_max_locals(1)

class_bytes = cb.build()
```

### Advantages

| # | Advantage | Detail |
|---|-----------|--------|
| 1 | **Encapsulated invariant management** | The builder manages constant-pool allocation, offset computation, and WIDE expansion internally. Users never see raw indexes. |
| 2 | **Clear lifecycle** | `build()` is an explicit finalization point. The builder can validate completeness before emitting. |
| 3 | **Good for class generation** | Building new classes from scratch is natural — builders are designed for incremental construction. |
| 4 | **Constant pool is automatic** | Methods like `Insn.GETSTATIC("java/lang/System", "out", ...)` can automatically allocate CP entries. |
| 5 | **Instruction list as first-class object** | BCEL's `InstructionList` (a doubly-linked list with `InstructionHandle` cursors) enables efficient insertion, deletion, and label management. |
| 6 | **Familiar to Java ecosystem users** | Developers migrating from BCEL will recognize the pattern immediately. |

### Disadvantages

| # | Disadvantage | Detail |
|---|--------------|--------|
| 1 | **Verbose API** | Every operation requires calling a method on a builder object. What is `cls.name = "Foo"` in the mutable model becomes `cb.set_name("Foo")`. |
| 2 | **Not Pythonic** | Python culture prefers direct attribute access and duck typing over builder chains. The Java-isms can feel foreign. |
| 3 | **Transformation of existing classes is awkward** | To modify an existing class, you must either (a) read it into a builder (reconstructing state), (b) copy fields one-by-one, or (c) maintain a parallel "edit" mode. BCEL's `ClassGen(JavaClass)` constructor does option (a), but it's an extra step. |
| 4 | **No streaming** | Like the mutable dataclass model, builders require full materialization. |
| 5 | **Large API surface** | Each builder needs methods for every possible mutation. This tends to produce large, feature-laden classes. |
| 6 | **Dual representation** | You end up with `ClassFile` (parsed) and `ClassBuilder` (editing) — two representations of the same concept. Users must understand when to use which. |
| 7 | **InstructionList complexity** | BCEL's `InstructionList` is a ~1500-line class with intricate handle management. Replicating this in Python is significant work. |

### Best suited for

Projects that primarily need **class generation from scratch** (e.g., dynamic proxy generation, bytecode compilers). Also appropriate when **strong invariant guarantees** are more important than API brevity.

---

## Design C: Visitor/Transformer Pattern (ASM-style)

### Description

Define abstract visitor classes (`ClassVisitor`, `MethodVisitor`, `FieldVisitor`) that receive events as a classfile is read. To transform a class, users subclass a visitor, override specific `visit_*` methods, and delegate to a downstream visitor (typically a `ClassWriter`). The visitor chain processes the classfile in a single streaming pass.

This is the pattern used by [ASM](https://asm.ow2.io/)'s core API.

### Sketch

```python
class ClassVisitor:
    def __init__(self, delegate: ClassVisitor | None = None): ...
    def visit(self, version, access, name, signature, super_name, interfaces): ...
    def visit_field(self, access, name, descriptor, signature, value) -> FieldVisitor | None: ...
    def visit_method(self, access, name, descriptor, signature, exceptions) -> MethodVisitor | None: ...
    def visit_end(self): ...

class MethodVisitor:
    def visit_insn(self, opcode): ...
    def visit_var_insn(self, opcode, var): ...
    def visit_field_insn(self, opcode, owner, name, descriptor): ...
    def visit_label(self, label): ...
    def visit_max(self, max_stack, max_locals): ...
    def visit_end(self): ...

# Usage: add a field to every class
class AddFieldTransformer(ClassVisitor):
    def visit_end(self):
        fv = self.visit_field(FieldAccessFlag.PRIVATE, "__tag", "I", None, None)
        if fv: fv.visit_end()
        super().visit_end()

writer = ClassWriter()
transformer = AddFieldTransformer(writer)
reader = ClassReader(class_bytes)
reader.accept(transformer)
result = writer.to_bytes()
```

### Advantages

| # | Advantage | Detail |
|---|-----------|--------|
| 1 | **Streaming / low memory** | Only one element is in flight at a time. Can process enormous classfiles without materializing the full tree. |
| 2 | **Composable transformations** | Visitors chain naturally: `Reader → Transform1 → Transform2 → Writer`. Each transformer only overrides what it changes. |
| 3 | **Efficient for bulk transformations** | Processing every class in a large JAR is fast because you never build a full in-memory tree. |
| 4 | **Proven at scale** | ASM is the de facto standard for JVM bytecode manipulation, used by virtually every major Java framework (Spring, Hibernate, Gradle, IntelliJ). The pattern is battle-tested. |
| 5 | **Separation of concerns** | Reader, transformer, and writer are independent. You can swap writers, chain transformers, or add analysis passes without touching existing code. |
| 6 | **Simple transformations are simple** | Adding a field, renaming a method, or filtering annotations requires overriding only 1-2 methods. |

### Disadvantages

| # | Disadvantage | Detail |
|---|--------------|--------|
| 1 | **Steep learning curve** | The visitor pattern is notoriously hard to learn. Users must understand event ordering, delegation, and the callback lifecycle. |
| 2 | **Un-Pythonic** | Deep class hierarchies, method-per-event APIs, and mandatory delegation feel like Java in Python clothing. Python developers expect simple iteration and data manipulation, not `visit_insn` callbacks. |
| 3 | **Random access is impossible** | You cannot "look at method 3, then go back to method 1" in a streaming visitor. Complex transformations that need cross-method or cross-class analysis require buffering (which defeats the streaming benefit). |
| 4 | **Difficult debugging** | Errors propagate through visitor chains. A bug in Transform2 may manifest as invalid output from the Writer, with no clear stack trace pointing to the root cause. |
| 5 | **State management is manual** | Visitors that need to accumulate state (e.g., "count all INVOKESPECIAL calls, then add a summary field") must manually manage instance variables across callbacks. |
| 6 | **Poor for class generation** | Building a class from scratch with a visitor requires calling `visit_*` methods in exact spec order. A builder is far more natural for this. |
| 7 | **Massive API surface** | ASM's `MethodVisitor` has ~30 `visit_*` methods. Each transformation must decide which to override and which to delegate. |
| 8 | **Doesn't leverage existing codebase** | pytecode's entire model is tree-based dataclasses[^1][^2][^3]. Adopting a visitor pattern would require either (a) rewriting the reader to emit events instead of building a tree, or (b) adding a tree-walking event emitter on top of the existing tree — adding a layer rather than replacing one. |

### Best suited for

Projects that need **high-throughput, memory-efficient bulk transformations** — processing thousands of classes in a build pipeline, instrumentation agents, or framework bytecode weaving.

---

## Design D: Pass Pipelines

### Description

Define transformations as composable "passes" — functions or objects that take a class representation and return a modified copy. Passes are pure functions (or near-pure) that can be composed into a pipeline. Each pass sees the full class tree but produces a new tree (or mutates a copy).

This pattern is common in compiler infrastructure (LLVM's pass manager, Rust's MIR passes) and functional programming.

### Sketch

```python
# A pass is a callable: ClassTree -> ClassTree
Pass = Callable[[EditableClass], EditableClass]

def add_tag_field(cls: EditableClass) -> EditableClass:
    cls = copy.deepcopy(cls)
    cls.fields.append(EditableField(
        access_flags=FieldAccessFlag.PRIVATE,
        name="__tag",
        descriptor="I",
    ))
    return cls

def rename_method(old: str, new: str) -> Pass:
    def transform(cls: EditableClass) -> EditableClass:
        cls = copy.deepcopy(cls)
        for m in cls.methods:
            if m.name == old:
                m.name = new
        return cls
    return transform

# Pipeline composition
pipeline = compose(add_tag_field, rename_method("foo", "bar"))
result = pipeline(editable_class)
```

### Advantages

| # | Advantage | Detail |
|---|-----------|--------|
| 1 | **Highly composable** | Passes are functions. Composition is just function composition. Easy to build, test, and reuse. |
| 2 | **Testable in isolation** | Each pass can be unit-tested with a synthetic class tree. No need to set up visitor chains or builders. |
| 3 | **Pythonic** | Functions and closures are natural Python. No class hierarchies or abstract methods. |
| 4 | **Immutability-friendly** | If passes return new trees, intermediate states are preserved for debugging or rollback. |
| 5 | **Easy to order and schedule** | A pass manager can sort passes by declared dependencies, run them in parallel if independent, or skip unnecessary passes. |
| 6 | **Clear data flow** | Input → Pass → Output. No hidden mutable state shared between stages. |

### Disadvantages

| # | Disadvantage | Detail |
|---|--------------|--------|
| 1 | **Deep-copy overhead** | If passes produce new trees, every pass copies the entire class. For instruction-level transformations, this is expensive. |
| 2 | **No streaming** | Like mutable dataclasses and builders, the full tree must be in memory. |
| 3 | **Requires a tree model anyway** | Passes operate on a tree, so you still need to design the tree model (Design A). Passes are an *addition* to the tree model, not a replacement. |
| 4 | **Overkill for simple edits** | A user who just wants to change one access flag doesn't need a pipeline. The ceremony of defining a pass, composing it, and running it is unnecessary. |
| 5 | **Less familiar for bytecode manipulation** | No major JVM bytecode library uses this pattern. Users coming from ASM/BCEL won't recognize it. |
| 6 | **Pass ordering complexity** | As the number of passes grows, managing dependencies between them becomes its own problem (e.g., "label resolution must run after instruction editing but before frame computation"). |
| 7 | **Not a complete design** | Passes describe *how transformations compose*, but not *how the data model looks*. You still need to design the tree (Design A) or builder (Design B) underneath. |

### Best suited for

Projects with **compiler-like pipelines** where multiple independent transformations are applied in sequence, and where testability and composability are more important than raw performance.

---

## Design E: Dual Approach (Tree + Optional Visitor)

### Description

Provide a **tree model** (Design A) as the primary, user-facing API, and an **optional visitor/event model** (Design C) for streaming use cases. The tree model is the default; users who need streaming performance can opt into the visitor API. A bridge layer converts between the two.

This is ASM's actual architecture: the `org.objectweb.asm.tree` package provides `ClassNode`/`MethodNode` (tree model) on top of the core visitor API. Most users interact with the tree API; performance-critical tools use visitors directly.

### Sketch

```python
# Tree API (primary)
cls = EditableClass.from_bytes(class_bytes)
cls.methods[0].code.insert_before(label, Insn.ALOAD(0))
raw = cls.to_bytes()

# Visitor API (opt-in, for streaming)
class MyTransformer(ClassVisitor):
    def visit_method(self, access, name, desc, sig, exc) -> MethodVisitor:
        mv = super().visit_method(access, name, desc, sig, exc)
        if name == "target":
            return InstrumentingMethodVisitor(mv)
        return mv

# Bridge: tree → visitor events
cls.accept(MyTransformer(ClassWriter()))

# Bridge: visitor events → tree
node = ClassNode()
reader.accept(node)  # ClassNode is a ClassVisitor that builds a tree
```

### Advantages

| # | Advantage | Detail |
|---|-----------|--------|
| 1 | **Best of both worlds** | Simple edits use the tree. Bulk JAR processing uses streaming visitors. Users choose based on their needs. |
| 2 | **Incremental implementation** | Build the tree model first (it's needed for everything else). Add the visitor layer later when streaming is actually needed. |
| 3 | **Proven architecture** | ASM's dual approach is the gold standard in the JVM ecosystem. It handles everything from IDE refactoring to runtime bytecode weaving. |
| 4 | **Composability via visitors, ergonomics via tree** | Complex pipelines chain visitors. One-off edits use the tree. Neither use case suffers. |
| 5 | **Tree model serves as visitor reference** | The tree's `accept()` method documents the exact event ordering for visitor implementors. |

### Disadvantages

| # | Disadvantage | Detail |
|---|--------------|--------|
| 1 | **Largest implementation effort** | Two APIs, a bridge layer, and documentation for both. Roughly 1.5–2× the work of either approach alone. |
| 2 | **Cognitive overhead** | Users must understand which API to use and when. Documentation must clearly guide this choice. |
| 3 | **Consistency burden** | Every feature must work in both models, or the library must clearly document which features are tree-only or visitor-only. |
| 4 | **Visitor API may never be needed** | If pytecode's use cases are primarily interactive/scripting (Python is not typically used for high-throughput build pipelines), the visitor API may be dead code. |
| 5 | **Bridge complexity** | Converting between tree and visitor representations introduces subtle ordering and lifecycle bugs. |

### Best suited for

Projects that aspire to be a **general-purpose bytecode toolkit** serving both interactive/scripting users and pipeline/tooling users.

---

## Comparative Analysis

### Feature Matrix

| Criterion | A: Mutable DC | B: Builder | C: Visitor | D: Passes | E: Dual |
|-----------|:---:|:---:|:---:|:---:|:---:|
| Pythonic feel | ★★★★★ | ★★☆☆☆ | ★☆☆☆☆ | ★★★★☆ | ★★★★☆ |
| Learning curve | Low | Medium | High | Low | Medium |
| Streaming support | ✗ | ✗ | ✓ | ✗ | ✓ |
| Random access | ✓ | Partial | ✗ | ✓ | ✓ |
| Class generation | Medium | Excellent | Poor | Medium | Good |
| Class transformation | Good | Fair | Excellent | Good | Excellent |
| Composability | Poor | Poor | Excellent | Excellent | Excellent |
| Implementation effort | Low | Medium | High | Low+ | High |
| Fit with existing codebase | Excellent | Fair | Poor | Good | Good |
| Invariant safety | Low | High | Medium | Medium | Medium |
| Memory efficiency | Low | Low | High | Low | Varies |

### Interaction with Planned Features

| Planned Feature | A: Mutable DC | B: Builder | C: Visitor | D: Passes | E: Dual |
|----------------|---|---|---|---|---|
| **CP management (#5)** | `ConstantPoolBuilder` on the class; resolved references in fields | CP embedded in builder, auto-allocated | CP entries emitted as events; writer manages pool | CP builder in tree, passes use it | Tree uses CP builder; visitor writer manages its own |
| **Labels (#7)** | `InstructionList` with label nodes interspersed | `InstructionListBuilder` manages labels | Labels are events in the visitor stream | Passes transform label-aware tree | Tree has labels; visitor emits label events |
| **Frame computation (#10)** | Run as a method on the class/method after edits | Builder computes frames at `build()` time | Dedicated `FrameComputingVisitor` in the chain | Frame computation is a pass | Tree: method call. Visitor: chain element. |
| **Emission (#12)** | `cls.to_bytes()` walks the tree | `builder.build()` returns bytes | `ClassWriter` visitor collects events and emits | Final pass serializes | Tree has `to_bytes()`; visitor has `ClassWriter` |
| **Debug info (#13)** | Labels in tree keep debug info aligned | Builder rebinds debug info at build time | Debug info events flow through chain | Debug rebinding is a pass | Both paths handle it |

### Recommendation for pytecode

Given pytecode's characteristics:

1. **Python-first audience** — Users chose Python over Java deliberately. A Java-flavored visitor API would be friction.
2. **Existing tree-based read model** — The codebase is already `@dataclass` trees[^1][^2]. Evolving to mutable trees is the smallest conceptual leap.
3. **Interactive/scripting primary use case** — Python is typically used for scripting, analysis, and tooling, not high-throughput build pipelines.
4. **Small team / solo maintainer** — Implementation effort matters. The dual approach is the most work.
5. **Roadmap has many other features** — Labels, CP management, frame computation, validation, and emission are all ahead. The editing model should enable these without being a bottleneck.

**The strongest starting point is Design A (Mutable Dataclasses)**, with the tree model designed so that **Design D (Pass Pipelines) can be layered on top** for composability, and **Design E (Visitor layer) can be added later** if streaming becomes necessary.

Concretely this means:

1. **Phase 1**: Build `EditableClass`/`EditableMethod`/`EditableField` as mutable dataclasses with symbolic references, plus `InstructionList` with label support and `ConstantPoolBuilder`. This is the core tree model.
2. **Phase 2**: Add pass-style composition helpers (`Pipeline`, `Pass` protocol) for users who want to chain transformations. This is lightweight — just function composition on top of the tree.
3. **Phase 3 (if needed)**: Add a visitor layer for streaming. This can be deferred until there's an actual use case. The tree model's `accept()` method provides the bridge.

This incremental path delivers immediate value (Phase 1 alone is a usable editing API), avoids premature complexity, and leaves the door open for streaming support without committing to it upfront.

---

## Assessment of Other Bytecode Manipulation Libraries

The designs A–E above are drawn primarily from ASM and BCEL. A broader survey of JVM bytecode manipulation libraries reveals additional design patterns that inform pytecode's choices. This section catalogs the key libraries not yet discussed, the new design patterns they introduce, and their relevance to pytecode.

### Surveyed Libraries

| Library | Repository | Core Design Pattern |
|---------|-----------|-------------------|
| **Javassist** | [jboss-javassist/javassist](https://github.com/jboss-javassist/javassist) | Source-level abstraction + mutable tree |
| **Byte Buddy** | [raphw/byte-buddy](https://github.com/raphw/byte-buddy) | Fluent declarative DSL over ASM |
| **Soot / SootUp** | [soot-oss/soot](https://github.com/soot-oss/soot), [soot-oss/SootUp](https://github.com/soot-oss/SootUp) | Intermediate representation lifting (Jimple, Shimple, Baf, Grimp) |
| **ProGuardCORE** | [Guardsquare/proguard-core](https://github.com/Guardsquare/proguard-core) | Visitor + instruction pattern matching engine |
| **WALA Shrike** | [wala/WALA](https://github.com/wala/WALA) ([Shrike wiki](https://github.com/wala/WALA/wiki/Shrike)) | Patch-based instrumentation |
| **JDK Class-File API** | [JEP 457](https://openjdk.org/jeps/457) / [JEP 484](https://openjdk.org/jeps/484) | Immutable elements + builders + composable transforms |
| **Krakatau** | [Storyyeller/Krakatau](https://github.com/Storyyeller/Krakatau) | Text-based assembly/disassembly |
| **CafeDude** | [Col-E/CAFED00D](https://github.com/Col-E/CAFED00D) | Simple read/write tree (obfuscation-resilient) |

### Design F: Source-Level Abstraction (Javassist)

Javassist's signature feature is its **source-level API**: users modify classes by inserting Java source code strings that the library compiles to bytecode on the fly[^7]. Underneath, `CtClass`/`CtMethod`/`CtField` form a mutable object tree — essentially Design A with a source-code overlay.

**Key idea**: `method.insertBefore("{ System.out.println(\"entering\"); }")` compiles and prepends bytecode without the user ever seeing opcodes.

**Relevance to pytecode**: The source-level compilation is **not portable** (it requires embedding a Java compiler, and pytecode's users are in Python, not Java). However, the mutable `CtClass` tree underneath validates Design A, and the `insertBefore`/`insertAfter` convenience methods on methods are a pattern pytecode should adopt as ergonomic helpers on `EditableMethod`.

### Design G: Fluent Declarative DSL (Byte Buddy)

Byte Buddy provides a high-level fluent builder where users *describe* the desired class rather than constructing bytecode[^8]. It is built on ASM internally but presents a declarative API with composable `ElementMatcher` predicates (e.g., `named("toString").and(returns(String.class))`) and pluggable `Implementation` objects (e.g., `FixedValue`, `MethodDelegation`, `Advice`).

**Key idea**: Matching (what to transform) is separated from implementation (how), enabling powerful bulk transformations.

**Relevance to pytecode**: The *matcher-based selection* pattern is worth borrowing — composable predicates for filtering methods/fields/instructions would be a natural Phase 2 addition. The fluent builder style is less Pythonic (Python prefers keyword arguments and context managers over long method chains). Byte Buddy's runtime JVM focus (agents, class loading) does not apply to pytecode.

### Design H: IR Lifting (Soot / SootUp)

Soot lifts JVM bytecode into one of four intermediate representations[^9]:
- **Baf**: simplified stack-based bytecode (no WIDE, named locals)
- **Jimple**: typed 3-address IR (stack eliminated, explicit assignments)
- **Shimple**: SSA form of Jimple
- **Grimp**: aggregated Jimple for decompilation

Users transform the IR; the framework lowers it back to bytecode. SootUp (the successor) uses **immutable IR objects** and a `BodyInterceptor` pattern where transformations are applied as interceptors when retrieving a method body[^10].

**Relevance to pytecode**: IR lifting is **out of scope** — it requires class hierarchy resolution, dataflow analysis infrastructure, and enormous implementation effort. However, two ideas transfer:
1. pytecode's `EditableMethod` with symbolic references and labels is effectively a "Baf-like" simplified representation of bytecode.
2. SootUp's `BodyInterceptor` aligns with pytecode's planned Phase 2 pass pipelines.

### Design I: Patch-Based Instrumentation (WALA Shrike)

Shrike represents method bytecode as an **immutable instruction array**. Modifications are described as **patches** (insertions, replacements) that reference positions in the *original* array. All patches are applied in a single pass, producing a new instruction array with branch targets and exception handlers automatically updated[^11].

**Key idea**: Because patches reference original positions, multiple independent edits to the same method don't interfere with each other — inserting code at position N doesn't shift the positions that other patches reference.

**Relevance to pytecode**: This is a **strong alternative** to the mutable `InstructionList` planned for Design A. For users who need to apply several independent instrumentation patches to the same method, a patch-based API avoids the invalidation problems inherent in mutable list editing. pytecode could offer both modes: direct `InstructionList` mutation for simple edits, and a `MethodEditor` with patch-based editing for complex multi-edit scenarios.

### Design J: Immutable Elements + Composable Transforms (JDK Class-File API)

The JDK's new Class-File API (JEP 457, finalized in JDK 24 as JEP 484) is the most modern design in this space[^12]. It introduces three abstractions:
- **Element**: immutable description of a classfile part (instruction, field, method, attribute)
- **Builder**: consumes elements and has convenience methods (e.g., `CodeBuilder.aload(n)`)
- **Transform**: a function `(Builder, Element) → void` that mediates how elements pass through

The critical innovation is **transform lifting**: a code-level transform can be lifted to a method-level transform and then to a class-level transform:

```java
CodeTransform ct = (codeBuilder, e) -> switch (e) {
    case InvokeInstruction i when i.owner().equals("Foo") ->
        codeBuilder.invoke(i.opcode(), ClassDesc.of("Bar"), i.name(), i.type());
    default -> codeBuilder.with(e);
};
ClassTransform classTransform = ClassTransform.transformingMethods(
    MethodTransform.transformingCode(ct));
byte[] result = ClassFile.of().transform(classModel, classTransform);
```

Because builders accept lambdas (not imperative calls), the library can **replay** the lambda with different parameters (e.g., retrying with long branch offsets if short offsets overflow)[^12]. Pattern matching on sealed element types replaces the visitor pattern entirely.

**Relevance to pytecode**: This design is **the most architecturally relevant** new pattern. It validates pytecode's planned Design A + D approach and shows how to make transforms composable at every level without a full visitor hierarchy. In Python, this would look like:

```python
CodeTransform = Callable[[CodeBuilder, CodeElement], None]

def rename_owner(builder: CodeBuilder, element: CodeElement) -> None:
    match element:
        case InvokeInsn(opcode=op, owner="Foo", name=n, desc=d):
            builder.invoke(op, "Bar", n, d)
        case _:
            builder.accept(element)

class_transform = lift_to_class(lift_to_method(rename_owner))
result = transform(class_model, class_transform)
```

### Instruction Pattern Matching (ProGuardCORE)

ProGuardCORE combines ASM-style visitors with a unique **instruction sequence matching and replacement engine**[^13]. Users define bytecode patterns with wildcards and replacement sequences, and the engine automatically finds and replaces matching sequences across all methods. This is a powerful feature that could be layered on top of pytecode's tree model as a Phase 2+ addition.

### Summary of New Patterns

| New Pattern | Source | Applicable? | When |
|-------------|--------|:-----------:|------|
| **Source-level abstraction** (F) | Javassist | ✗ (requires Java compiler) | — |
| **Fluent declarative DSL** (G) | Byte Buddy | Partially (matchers) | Phase 2 |
| **IR lifting** (H) | Soot/SootUp | ✗ (massive scope) | — |
| **Patch-based editing** (I) | WALA Shrike | ✓ (alternative to mutable InstructionList) | Phase 1–2 |
| **Immutable element + transform lifting** (J) | JDK Class-File API | ✓ (informs Phase 2 transform design) | Phase 2 |
| **Instruction pattern matching** | ProGuardCORE | ✓ (declarative find-and-replace) | Phase 2+ |

### Impact on Recommendation

None of the surveyed designs displace Design A (Mutable Dataclasses) as the Phase 1 recommendation. Every library surveyed either uses a mutable tree internally (Javassist, CafeDude, BCEL), builds on a visitor/tree substrate (Byte Buddy, ProGuardCORE), or uses an IR that is a different kind of tree (Soot). The mutable tree is the universal substrate.

The survey **reinforces the phased approach** and adds specificity:

| Phase | Original Scope | Additions from Survey |
|-------|---------------|----------------------|
| **Phase 1** | Mutable tree + InstructionList + ConstantPoolBuilder | Consider **patch-based editing mode** alongside mutable list (Shrike). Add **insertBefore/insertAfter helpers** on methods (Javassist). |
| **Phase 2** | Pass composition | Model transforms as `(builder, element) → None` functions with **transform lifting** (JDK Class-File API). Add **matcher-based selection** (Byte Buddy). |
| **Phase 2+** | — | Add **instruction pattern matching** engine (ProGuardCORE) for declarative find-and-replace. |
| **Phase 3** | Streaming visitor (if needed) | JDK Class-File API shows streaming and tree can share the same element vocabulary, reducing the "two API" burden. |

---

## Confidence Assessment

- **High confidence**: The analysis of each design's trade-offs is well-grounded in the actual ASM and BCEL architectures and in pytecode's existing codebase structure.
- **High confidence**: The interaction with planned features (#5, #7, #10, #12, #13) is based on the detailed ARCHITECTURE.md roadmap and issue descriptions.
- **Medium confidence**: The recommendation to start with Design A is based on the assumption that pytecode's primary audience is Python scripters/analysts, not high-throughput build-pipeline authors. If the latter becomes important, the visitor layer (Design E) should be prioritized sooner.
- **Medium confidence**: Shrike's patch-based editing model is a strong alternative for instruction editing, but the actual ergonomics in Python would need prototyping before committing to it as a primary editing mode.
- **Assumption**: The `InstructionList` abstraction (needed by both Design A and B) will be a substantial piece of work regardless of the top-level API design choice. This is covered by issue [#7](https://github.com/smithtrenton/pytecode/issues/7).

---

## Footnotes

[^1]: `pytecode/info.py:1-46` — The top-level `ClassFile`, `FieldInfo`, and `MethodInfo` dataclasses form the current read model.
[^2]: `pytecode/info.py:29-45` — `ClassFile` references `ConstantPoolInfo`, `FieldInfo`, `MethodInfo`, and `AttributeInfo` by list.
[^3]: `pytecode/instructions.py:7-11` — `InsnInfo` base class carries `type` and `bytecode_offset`; subclasses like `LocalIndex`, `ConstPoolIndex`, `Branch` carry raw operands.
[^4]: `pytecode/info.py:34` — `constant_pool: list[ConstantPoolInfo | None]` is indexed by position, with `None` for Long/Double second slots.
[^5]: `pytecode/instructions.py:39-45` — `Branch` and `BranchW` carry raw `offset: int` byte offsets.
[^6]: `pytecode/attributes.py:27-32` — `ExceptionInfo` uses `start_pc`, `end_pc`, `handler_pc` as raw byte offsets.
[^7]: [javassist.org](https://www.javassist.org/) — "Javassist provides two levels of API: source level and bytecode level."
[^8]: [github.com/raphw/byte-buddy](https://github.com/raphw/byte-buddy) — Fluent DSL built on ASM's visitor API; downloaded 75M+ times/year.
[^9]: [github.com/soot-oss/soot](https://github.com/soot-oss/soot) — "Soot provides four intermediate representations: Baf, Jimple, Shimple, Grimp."
[^10]: [github.com/soot-oss/SootUp](https://github.com/soot-oss/SootUp) — "Immutable Jimple IR Objects and Graphs", `BodyInterceptor` pattern.
[^11]: [github.com/wala/WALA/wiki/Shrike](https://github.com/wala/WALA/wiki/Shrike) — "Bytecode modification is done by adding patches to a MethodEditor and then applying those patches."
[^12]: [JEP 457](https://openjdk.org/jeps/457) — JDK Class-File API: immutable elements, builders, composable transforms, transform lifting.
[^13]: [github.com/Guardsquare/proguard-core](https://github.com/Guardsquare/proguard-core) — Instruction sequence pattern matching and replacement engine.
