# Compatibility and resource limits

## Java versions

The inspection reader supports major versions 45–70 (through Java 26). For major
versions 56–70, minor versions 0 and 65535 are accepted. Historical preview files
remain inspectable; this does not imply they run on newer JVMs. The Rust
`class_version_compatible_with_runtime` helper separately checks version
compatibility, including matching the preview major to the runtime and requiring
preview enablement. Bytecode verification remains a separate check.

The format audit compared the class structure, constant-pool tags, predefined
attributes, and version rules in [JVMS 25 chapter 4](https://docs.oracle.com/javase/specs/jvms/se25/html/jvms-4.html)
and [JVMS 26 chapter 4](https://docs.oracle.com/javase/specs/jvms/se26/html/jvms-4.html).
Their class structure, constant-pool tag inventory, and predefined attribute
inventory match. Support is tested with `javac --release 26`, recomputation, and
`java -Xverify:all`, including flexible constructors and a primitive-pattern
preview case. This is targeted compatibility coverage, not a complete JVM
conformance claim.

## Java strings

Java string values contain UTF-16 code units. `LdcValue::String` stores a
`modified_utf8::JavaString`, including unmatched high and low surrogates. Rust
callers can construct it with `JavaString::from_utf16` or `"text".into()`.
`to_unicode()` is a fallible conversion to Rust `String`; it never substitutes
replacement characters. Existing `ConstantPoolBuilder::add_string(&str)` remains
available; `add_java_string`, `add_utf16`, and `resolve_java_string` handle arbitrary
Java string content.

Python `LdcInsn.string`, `ConstantPoolBuilder.add_string`, `add_utf8`, and
`resolve_utf8` preserve unmatched surrogates. Valid surrogate pairs decode to a
single Python supplementary Unicode character; UTF-16 code units remain unchanged.
Class/member identifiers and descriptors continue to use Rust Unicode strings.
Built-in text matchers accept Unicode scalar text; callbacks can inspect arbitrary
Java string values through Python.

## Classfile boundaries

Whole-class readers reject trailing input. Descriptors accept at most 255 array
dimensions and 255 explicit parameter slots; structural method validation includes
the receiver in that slot limit. Reserved JVM opcodes cannot occur in classfiles.
Switch ranges and lookup key ordering are checked during reading and writing.

Serialization rejects lengths exceeding their classfile integer width, including
the modified-UTF-8 **encoded byte** limit of 65,535. Code bodies must contain
1–65,535 bytes. Attribute/annotation nesting and generic-signature nesting are
limited to 128 parser levels. Array signatures are read iteratively up to 255
dimensions. These are library resource limits, distinct from JVM type verification.

## Member lists

`ClassModel.fields` and `ClassModel.methods` are live, read-only views: they index,
slice, and iterate, but do not support item assignment or `del`. Edit a member list
by assigning a new list, which may reorder members, drop them, or add new
`FieldModel` / `MethodModel` objects:

```python
model.methods = [method for method in model.methods if method.name != "unused"]
model.fields = sorted(model.fields, key=lambda field: field.name)
```

Assignment copies the members into the class, so `FieldModel` / `MethodModel`
handles taken from the view before the assignment become stale (using one raises
`RuntimeError`); read the view again afterwards. Each method keeps its own code,
frames, and attributes, so reordering needs no frame recomputation, and
reassigning unchanged lists round-trips byte-for-byte. Deleting a member does not
touch references to it: removing a field or method that code still uses still
lowers, but the class fails at run time (`NoSuchFieldError` /
`NoSuchMethodError`), so check for references first.

## Exception-handler tables

`CodeModel.exception_handlers` (Rust: `CodeModel::exception_handlers`) is the
method's exception table in symbolic form: each `ExceptionHandler` covers the
instructions from its `start` label up to (not including) its `end` label and
jumps to its `handler` label, for exceptions assignable to `catch_type` (an internal
class name, or `None` for a catch-all, as `finally` blocks compile to).

Entries are immutable values, so edit the table by assigning a new list, the same
pattern as `ClassModel.bootstrap_methods`:

```python
code.exception_handlers = [
    ExceptionHandler(entry.start, entry.end, entry.handler, rename(entry.catch_type))
    for entry in code.exception_handlers
    if keep(entry)
]
```

Rebuilt entries reuse the existing label objects, which must still occur in the
instruction list (lowering rejects a handler anchored to a missing label). Order
matters: the JVM uses the first entry whose range and type match. Deleting an entry
can leave its handler code unreachable; with `FrameComputationMode.RECOMPUTE`
(needed whenever the table changes in a method with StackMapTable frames) the
unreachable code is normalized. Reassigning an unchanged table round-trips
byte-for-byte.

## Constant-pool layout

Lowering keeps a class's original constant pool by default
(`ConstantPoolPolicy.PRESERVE`) and appends whatever an edit needs. That is what
makes an unchanged class round-trip byte-for-byte, but it also makes the pool a
record of the class's history: unreferenced entries left by earlier edits stay, and
two classes with identical content but differently built pools (edits applied in a
different order, or an obfuscator that shuffles the pool on every build) lower to
different bytes.

`constant_pool=ConstantPoolPolicy.CANONICAL` (on `to_bytes_with_options`,
`to_classfile_with_options`, and the `rust_lower_classmodels*` functions; Rust:
`ClassModel::to_bytes_with_pool_policy` / `to_classfile_with_pool_policy`, or
`canonicalize_constant_pool` on a `ClassFile`) emits a pool that depends only on
the class's content:

- Only entries the class references are kept, transitively (a `Class` keeps its
  `Utf8`, a member reference its `NameAndType`, a method handle its member
  reference), and equal constants are merged.
- Entries are ordered by resolved content, never by index: constants loaded by
  `ldc`/`ldc_w` first, so they get the indices a narrow `ldc` can address, then
  everything else; within each group by entry kind, then by value.
- What the layout decides is settled the same way: every `ldc` whose constant
  fits a one-byte index is narrow, and preserved StackMapTable frames are moved to
  the resulting instruction offsets and written in their most compact form.

Ordering of fields, methods, attributes, interfaces, and instructions is out of
scope: it is part of the class's content and is emitted as-is. Two classes that
differ only in member order still lower to different bytes.

Canonical lowering runs the normal lowering first (frames, debug-info policy, and
warnings apply as usual), then re-lifts that result and lowers it again against the
canonical pool, so it costs roughly two extra lift/lower passes per class. It
refuses classes carrying an attribute pytecode does not parse (for example the
CLDC `StackMap` attribute), since their payloads may hold constant-pool indices it
cannot remap; strip such attributes first or keep the default. Code that the
default policy cannot lower with preserved frames fails the same way in canonical
mode.

## Frame recomputation

Frame recomputation tracks constructor initialization independently of local
variables. Overwriting local zero does not permit an early return. Calls that
initialize a receiver replace all its aliases on the normal path and invalidate
saved aliases on the exception path. A pending constructor flag that cannot be
represented in a modern StackMapTable produces an explicit error. During
`FrameComputationMode.RECOMPUTE` lowering, unreachable bytecode regions are
normalized (`nop...athrow` with synthetic frames) and excluded from exception-table
ranges (wholly dead handlers are dropped and partially covered handlers are trimmed
or split around unreachable gaps, preserving handler priority and remapping `Catch`
type annotations); pre-Java-7 (`major < 51`) `jsr`/`ret` subroutines are analyzed
polymorphically. `verify_classfile` and `verify_classmodel` (on prebuilt or
unchanged lifted code carrying a `StackMapTable`) also decode `StackMapTable`
frames and check exception-handler frame compatibility against protected ranges.

Model-based recomputation checks the actual direct superclass and declared fields:
before initialization, `putfield` must reference a field declared by the current
class. The standalone Rust analysis functions take a class name rather than a
complete model, so they cannot establish those missing declarations. The root
`java/lang/Object` constructor is handled separately. These rules follow the
[JVMS constructor and field instruction checks](https://docs.oracle.com/javase/specs/jvms/se26/html/jvms-4.html#jvms-4.10.1.9).

### Hierarchy at join points

Merging two reference types at a join needs their common superclass. The
recomputer uses, in order: the class being lowered (its own declaration always
wins over a resolver's copy), the supplied `resolver`, and facts deduced from the
code. A resolver does not have to be complete:

- A missing class never aborts recomputation. Superclass chains are walked as far
  as they are known; when the known parts of two chains meet, that class is the
  exact merge, even if neither chain reaches `java/lang/Object`.
- When they do not meet, the merge falls back to `java/lang/Object`. That only
  matters if the merged value later reaches an instruction that requires a more
  specific class. Since the input is assumed to verify with *some* frames, every
  merged class must then be a subtype of the required class. The recomputer
  records that, runs the method's dataflow again (at most four passes), and the
  join gets the required class. A merged class whose superclass chain is fully
  known and lacks the required type means the required type is an interface,
  which accepts any reference.
- Whatever stays imprecise is reported, never silent: Rust callers get
  `FrameWarning`s from `ClassModel::to_classfile_with_report` /
  `to_bytes_with_report`, `FrameComputationResult::warnings`, and
  `PreparedRewrite::frame_warnings`. Python lowering and `JarFile.rewrite` emit
  `pytecode.model.FrameComputationWarning` (escalate it with
  `warnings.simplefilter("error", FrameComputationWarning)`; `rewrite` raises
  before committing). `verify_classmodel` reports them as warnings when a resolver
  is supplied, and the CLI lists them in its rewrite reports.

Deduced facts are only as good as the input: frames for bytecode that does not
verify may be as invalid as the input. Supplying the full runtime hierarchy
(application, dependencies, and the JDK) still gives the most precise frames.

StackMapTable frames are emitted where the type checker needs them: branch targets,
exception handlers, the starts of normalized unreachable regions, and the
fall-through of a conditional branch that had to be inverted around a `goto_w`.
Methods with `jsr`/`ret` keep a frame at every basic-block start.

This analysis does not certify every JVM verification rule. In particular, ordinary
reference operands are checked as initialized references; a supplied hierarchy
resolver improves joins but does not establish all operand assignability or member
access constraints. Use independent JVM verification for transformed output.

## Bootstrap methods

`ClassModel.bootstrap_methods` (Rust: `bootstrap_methods` /
`set_bootstrap_methods`) exposes the `BootstrapMethods` attribute symbolically:
each entry is a bootstrap `MethodHandleValue` plus static arguments as loadable
constants (`BootstrapArgument` in Python, `LdcValue` in Rust). The arguments can
name classes through method handles, method types, and class literals that the JVM
resolves when the call site links, so class and member renames must rewrite them
along with the instructions. Assigning re-encodes the attribute in place; values
left unchanged keep their original constant-pool indices, so an unchanged
round-trip is byte-for-byte identical. `invokedynamic` instructions and dynamic
constants address entries by position, so keep existing entries at their index.
An empty list removes the attribute.

### Call-site and dynamic-constant descriptors

An `invokedynamic` call site's descriptor lives in its `CONSTANT_InvokeDynamic`
entry, not in any method, and the JVM only resolves it when the call site first
links. A rename that rewrites method descriptors therefore has to rewrite the
call sites too, or the class loads and verifies but fails with a linkage error at
the call site's first execution.

`InvokeDynamicInsn.descriptor` is writable (Rust: the public `descriptor` field).
Python instructions are detached values, so assign the descriptor on the items of
`code.instructions.to_list()` and write the list back (`code.instructions =
items`), or build a replacement with `InvokeDynamicInsn(index, name, descriptor)`.
The setter rejects anything that is not a method descriptor. Lowering re-encodes
the call site as a `NameAndType` under the same bootstrap method index, reusing
existing constant-pool entries, so a call site whose descriptor is unchanged lowers
to the same bytes. The default constant-pool layout keeps the old descriptor as an
unreferenced entry; nothing refers to it.

Dynamic constants (`CONSTANT_Dynamic`) are symmetric: `DynamicValue.descriptor` is
writable (it must be a field descriptor), and `LdcInsn.from_dynamic` /
`BootstrapArgument.from_dynamic` wrap an edited value back into an `ldc` or a
bootstrap argument.

Writing back an instruction list never counts as a code change by itself: in
`FrameComputationMode.PRESERVE`, preserved StackMapTable frames stay valid as long
as the instructions are equal, even though rebuilt raw instructions no longer carry
their original offsets. A real change to an instruction in a method with frames
still needs `RECOMPUTE`, as it does for any operand edit.

## Archive mutation and transactions

Python `JarFile.files` and its `JarInfo` values are authoritative. Changes to bytes,
entry filenames, and supported `zipinfo` metadata are reflected in `rewrite`.
`infolist` is a view refreshed by archive operations. Rust public entry mutations
are likewise honored; an original ZIP index alone does not authorize raw copying.

Rewrites stage a temporary file in the destination directory, validate it, and build
the refreshed Python view before atomically replacing the destination. Failed
transforms, staging, refresh, or replacement preserve destination bytes and clean
temporary files. Atomic visibility does not guarantee durability after a crash.
User callbacks' external side effects are outside the archive transaction.

Archive comments are preserved as bytes. Supported entry metadata includes
compression, DOS timestamps, creating system, Unix permissions, UTF-8 comments,
and supported ZIP extra fields. This is not a promise to preserve every central-
directory flag, external-attribute bit, ZIP64 encoding, or original compressed
representation. Entries with extra fields use ordinary emission because the ZIP
backend's raw-copy operation drops those fields. Unsupported comment encodings or
extra fields may reject rewriting; the original archive stays intact.

Exact duplicate names and names colliding after POSIX `'/'` path
normalization (backslashes are rejected/normalized consistently across platforms)
are rejected. Absolute paths, drive-prefixed paths, and parent traversal are
rejected on every platform. Archives are not extracted to disk by these APIs.

Default read/rewrite limits are 100,000 entries, 256 MiB decompressed per entry, and
1 GiB total decompressed content. Python constructors expose `max_entries`,
`max_entry_bytes`, and `max_total_bytes`; Rust exposes `ArchiveReadLimits` and
`JarFile::open_with_limits`. Limits check declared sizes and actual decompression.
The ZIP backend parses central-directory metadata before the entry-count check;
these limits are not a total process-memory guarantee.

Signed-JAR signature resources remain ordinary resources. Rewriting changed class
bytes does not re-sign the archive and can invalidate existing signatures.

Multi-release entries are inventoried and preserved independently. Archive APIs do
not infer a target runtime. Before constructing a hierarchy, callers must choose
one definition per binary name: use the base entry unless the main manifest has
`Multi-Release: true`; otherwise select the highest entry version between 9 and the
chosen Java release, falling back to the base. `MappingClassResolver` rejects
duplicate binary names by default, preventing accidental dependence on ZIP entry
order. When combining overlapping universes on purpose (an application jar and a
dependency that both ship a class), pass `duplicates="first"` or `"last"` (Rust:
`DuplicateClassPolicy`), or combine resolvers with `merged`.

## Python ownership and packaging

Mutable class, method, code, and constant-pool views share Rust-owned state and
expose direct mutation APIs for fields, methods, interfaces, attributes,
instructions, exception handlers, debug tables, and type annotations, while
`pytecode.classfile.attributes` re-exports Rust-backed raw attribute and
annotation classes. Structural mutations to a class or code model bump generation
counters on child views so stale handles fail loudly rather than observing stale
state; retain the owner when working with its views, or use `copy.copy` /
`copy.deepcopy` on `ClassModel`, `FieldModel`, `MethodModel`, or `CodeModel`
to obtain an independent detached copy (pickle serialization is not supported;
use `ClassModel.to_bytes()` / `ClassModel.from_bytes()` for serialization).

These views use `Rc<RefCell<...>>` and PyO3's unsendable classes. Use them only on
their creating thread. The module explicitly requires the GIL (`gil_used = true`);
CPU-bound binding calls do not detach from it. Free-threaded CPython is not a
supported wheel target. Use separate processes for parallel independent work.

The abi3 wheels target ordinary CPython 3.12 and newer. Installed-artifact CI tests
3.12, 3.13, and 3.14 on native x86-64 and ARM64 runners for Linux, Windows, and
macOS, and rebuilds a wheel from the sdist before release publishing. Local checks cover Windows x86-64; the remote installed-artifact matrix has also passed on all six native targets.
Runner labels follow the [GitHub runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

Development pins Rust 1.98.1, Python 3.14.7, uv 0.12.10, and maturin 1.15.0. Rust
1.94 remains the minimum and passes the upgraded workspace check. PyO3 0.29.2
uses Windows raw-dylib support; the obsolete `generate-import-lib` feature was
removed following the [migration guide](https://pyo3.rs/v0.29.2/migration).

Source distributions use maturin's Git generator with `[tool.maturin] exclude` to
omit test fixtures, sample JARs, tests, docs, examples, fuzz targets, and helper
tools while retaining all workspace crate manifests and `Cargo.lock` so `cargo`
and `maturin` remain self-contained for isolated `locked = true` source builds;
no Git checkout is needed to build a wheel from that archive.
