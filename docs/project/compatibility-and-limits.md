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

Exact duplicate names and names colliding after path normalization are rejected.
Absolute paths, drive-prefixed paths, and parent traversal are rejected on every
platform. Archives are not extracted to disk by these APIs.

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
duplicate binary names, preventing accidental dependence on ZIP entry order.

## Python ownership and packaging

Mutable class, method, code, and constant-pool views share Rust-owned state. Member
views can update their owning model; instruction list views support inspection,
while structural instruction changes use transform APIs. Retain the owner when
working with its views, and create separate models for independent edits.

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

Source distributions use maturin's Git generator to retain every workspace member
and preserve the meaning of `Cargo.lock`. Build release sdists from a checkout
whose intended sources are tracked. `locked = true` also applies to builds from
the source archive; no Git checkout is needed to build a wheel from that archive.
