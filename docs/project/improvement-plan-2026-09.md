# Repository improvement plan

Reviewed 2026-09-04 at commit `bb357f9`. The review below is retained as the implementation checklist. A follow-up goal requested implementation and delivery to remote `main` with passing CI. The implementation is delivered to remote `main`. Local validation and remote execution evidence are recorded below; CI checks every delivered revision.

## Follow-up delivery

The requested follow-up review found and fixed additional constructor-analysis
errors. Initialization state now survives local-variable overwrites and joins;
failed constructor calls invalidate saved receiver aliases; `null` cannot merge
with uninitialized references; and the root Object constructor is handled
correctly. Model lowering validates constructor owners and fields assigned before
initialization. Independent JVM tests cover valid aliases, invalid constructors,
and exception handlers, including errors that cannot be represented by a modern
StackMapTable. Standalone-analysis context limits are explicit in
[compatibility and limits](compatibility-and-limits.md#frame-recomputation).

Generated generic-signature tests cover nested type arguments, bounds, arrays,
inner types, method results, throws clauses, and trailing-input rejection. Plain
`pytest` now searches `tests/`, avoiding accidental collection of packaging
snapshots under `output/`. Frame simulation and worklist logic have their own
module without changing public FrameState layout.

Measured follow-ups replace linear queue membership checks, remove discarded
typed-pop allocations, and count borrowed archive entries. The wide-switch case
is about 27% faster and archive open/classification about 11% faster on the recorded
workloads. The javac fixture workload has no statistically clear timing change.
[Performance evidence and commands](performance-2026-09.md#follow-up-frame-worklists-and-archive-classification)
record the baseline, uncertainty, corpus hashes, and copy-volume calculation.

Final follow-up local checks: 243 Rust tests; 233 Python tests with two Java 26
skips; 37 separate Java 26 checks; rustfmt, Clippy, Ruff, Basedpyright, Rust 1.94,
and API documentation coverage (262/262). The CI and installed-artifact workflow
histories linked below verify delivery revisions on remote `main`.

## Original implementation progress

| Batch | Status and evidence |
| --- | --- |
| 1: Tooling/baseline | Local checks pass with source-built release extension: 212 Rust tests, 182 Python tests, Ruff, Basedpyright, docs, rustfmt, Clippy, and Rust 1.94 workspace check. Rust 1.98.1/Python 3.14.7 pinned; uv.lock now tracked; CI platform/MSRV jobs added; source/JDK-keyed fixture cache protected by process/thread file locks. Remote CI runs the platform and minimum-Rust checks. |
| 2: Frames | Core fixes implemented locally: typed stack/local effects, category-2 shapes, constructors, array joins, resolver error propagation, raw-code normalization, fail-fast, and checked frame bounds. javac regressions execute with `java -Xverify:all`, with and without a supplied resolver. Broader independent negative coverage remains in batch 5. |
| 3: Archives | Staged transaction implemented in Rust and Python, including validation/Python refresh before commit, temporary cleanup, live public-entry edits, archive comments, creating system, duplicate-name rejection, and configurable decompression limits. Four new Rust integrity tests and 13 new Python cases pass; all archive/frame Python tests total 62 passing. ZIP raw-copy drops extra fields, so such entries use normal emission. Full metadata contract/docs audit remains in batch 9. |
| 4: Classfile hardening | Descriptor dimensions/names/parameter slots, checked nested writer lengths, strict whole-class parsing, switch/reserved-opcode checks, and reader/writer/signature nesting budgets implemented with regressions. Added UTF-16-backed `JavaString` for LDC values and lossless Python constant-pool/string conversion. Seven Python surrogate cases pass, including javac literals, annotations/constants, model recomputation, and JVM execution. Remaining operand/malformed-input expansion continues with batch 5. |
| 5: Independent verification/fuzzing | Shared JVM harness now forces method resolution and distinguishes execution failure; invalid-method, wide-local, and valid recomputation cases pass. UTF-16/descriptor/model property tests added with deterministic CI seed. Three AddressSanitizer fuzz targets compiled and passed short local runs (130,601 / 38,592 / 49,335 executions); change-time and weekly CI lanes added. Multi-release preservation covered; duplicate resolver names are rejected. |
| 6: Java 26 | Major 70 and explicit inspection/runtime preview policies implemented. Dedicated JDK 26 CI lane added. Locally, 21 Java 26/frame/string cases pass, including GA and preview execution. Independent JVM execution found and fixed category-2 slot padding incorrectly emitted as an extra stack-map type. |
| 7: Dependencies/artifacts | Upgraded the listed stable Python/Rust dependencies, including PyO3 0.29.2 and ZIP 8.6.0. Rust 1.94 still passes. Both RustSec audits report zero advisories/warnings. Final locked sdist rebuild and installed-wheel smoke checks pass on Windows x86-64/Python 3.12, 3.13, and 3.14. Native six-target artifact gates now precede release publishing and have passed remotely. |
| 8: Performance | Direct attribute/code output replaces temporary 32 KiB buffers, reducing writer time about 49% and allocation traffic about 99% on the fixed corpus. Full pipeline timing has no statistically clear change. [Measurements and reproduction](performance-2026-09.md) record uncertainty, allocation counts, and peak live bytes. Other candidates remain measurement-driven follow-up work. |
| 9: Maintainability/docs | Extracted attribute emission, stack-map encoding, and Python constant-pool bindings; shared internal/member-name validation; consolidated Java fixtures; removed the global attribute-access suppression with only two specific descriptor-assignment exceptions; added stub/runtime checks; revised compatibility, ownership, quality gates, roadmap, historical audit, and PowerShell benchmark examples. |

## Final local validation

The earlier counts in batches 1–6 record intermediate checks. The final combined
source was rebuilt in release mode and checked as follows:

| Gate | Result |
| --- | --- |
| Rust workspace tests | 237 passed |
| Python suite, JDK 25 | 224 passed, 2 Java 26 cases intentionally skipped |
| Separate JDK 26 lane | 21 passed, including GA/preview execution and frame/string regressions |
| rustfmt / Clippy with warnings denied | Passed |
| Ruff lint/format / Basedpyright strict attribute checks | Passed |
| API documentation coverage | 262/262 symbols |
| Rust 1.94 minimum / uv lock check | Passed |
| Workflow syntax | actionlint 1.7.12 passed; shellcheck/pyflakes integration disabled locally |
| RustSec | No advisories or warnings for 135 workspace / 36 fuzz dependencies; database commit `5a0ebedfe8bdd2e295b171f4162f8c977bcad9a5` |
| Installed final sdist-built wheel | Python 3.12.14, 3.13.15, 3.14.7 on Windows x86-64, fresh environments outside the checkout |
| Sanitizer fuzzing | Three short runs passed before optimization; final post-optimization passes are recorded in `fuzz/README.md` |

The source-distribution test found that maturin's Cargo generator removes the CLI
workspace member without pruning its lockfile. `sdist-generator = "git"` now
preserves the full workspace and `locked = true` enforces the shipped graph.
The final local packaging check used a disposable indexed snapshot of the current
files before committing the implementation. No commit was needed
for that packaging test.

## Delivery verification

The implementation and CI corrections are delivered to `origin/main`. The current
[CI run history for main](https://github.com/smithtrenton/pytecode/actions/workflows/ci.yml?query=branch%3Amain)
records build, lint, Rust/Python platform tests, minimum Rust, and Java 26 results
for each delivered revision.

Remote evidence already obtained:

- [Installed artifacts](https://github.com/smithtrenton/pytecode/actions/runs/33939424208)
  passed on `5c895f8`: all six native platform/architecture wheels ran on CPython
  3.12, 3.13, and 3.14; the source distribution also rebuilt and passed those checks.
- [Fuzz](https://github.com/smithtrenton/pytecode/actions/runs/33939424242) passed all
  three sanitizer targets on `5c895f8`.
- [Dependency advisories](https://github.com/smithtrenton/pytecode/actions/runs/33939287212)
  passed on `0f3ddde`; the audited lockfiles are unchanged in the delivery fixes.

CI exposed two portability issues that local Windows checks did not reveal: the
Rust-only MSRV job's unused uv cache failed during cleanup, and a ZIP fixture
warning assertion incorrectly assumed Windows filename normalization on POSIX.
The unused cache is disabled, and warning expectations now use the standard
library's actual emitted filename. The archive duplicate-rejection assertion stays
unchanged.

`main` is the delivery branch. The follow-up checks repository settings and moves
the default to `main` after successful CI so scheduled maintenance uses this code.
No release tag, PyPI publish, or GitHub release is part of this delivery.
Broader verifier conformance, large-module refactors beyond the affected areas,
streaming/concurrency APIs, and unmeasured performance candidates remain follow-up
work under the documented compatibility limits, not claims established by this batch.

The highest-value work is fixing frame computation and archive integrity, supported by a reproducible build and stronger independent validation. The four-crate architecture is a useful foundation and does not need replacement. Existing tests and tooling are substantial, but passing them currently misses several reproducible correctness bugs.

## Evidence and baseline

Review covered the engine's parser, writer, descriptors, modified UTF-8, frame simulation and hierarchy code; Python model/archive bindings; archive implementation; benchmarks; CI/release workflows; and existing quality/conformance documents. It was a targeted review, not exhaustive verification of every opcode, attribute, CLI transform, or dependency advisory.

| Check | Result |
| --- | --- |
| Python tests | `180 passed in 26.08s` with the existing environment/extension |
| Ruff lint and format | Passed; 36 files formatted correctly |
| Basedpyright | 0 errors, warnings, or notes |
| Rust formatting | Passed with Rust 1.98.1 |
| Clippy with `-D warnings` | Failed: `manual_filter` in `reader.rs:1211` |
| API documentation check | Found 262/262 documented symbols, then failed printing a checkmark to a cp1252 console |
| Initial Rust workspace test run | Compiled successfully; stopped on an archive test's fixture-cache `PermissionDenied` error; two other archive tests passed |
| Serial Rust workspace rerun | 210 passed, 1 failed: Java 25 fixture compilation requires a newer JDK than the installed 21.0.11; the archive tests passed on this rerun |

The Python results used the existing `_rust.pyd` dated April 21, not a freshly installed extension. Its observed failures agree with the inspected source, but reproductions must be rerun against a fresh build before fixes. Installed maturin/pytest/Ruff were 1.12.6/9.0.2/0.15.9, while `uv.lock` specifies 1.13.1/9.0.3/0.15.10. Python is 3.14.7; local Java is 21.0.11, below the JDK 25 requirement for the full fixture suite. `uv` was absent from this shell's PATH. These are environment findings, not proof that a clean CI build fails in the same way.

## Implementation order

Each row is a suggested independently reviewable batch. Sizes are relative: S is a focused change, M spans several modules, L needs multiple focused changes and independent verification. Do not combine dependency migrations, semantic fixes, and broad refactors into one patch.

| Order | Priority | Batch | Size | Depends on |
| --- | --- | --- | --- | --- |
| 1 | P1 | Reproducible tooling, baseline, and platform checks | M | None |
| 2 | P0 | Correct stack effects and verifier state | L | 1 |
| 3 | P0 | Preserve archive edits and make rewrite failures transactional | M | 1 |
| 4 | P1 | Preserve valid Java strings; harden descriptors and emission limits | L | 1 |
| 5 | P1 | Independent JVM verification and fuzz/property coverage | L | Start with 2; expand with 3–4 |
| 6 | P1 | Java 26 support and documented compatibility policy | M | 4–5 |
| 7 | P1 | Dependency refresh and installed-artifact release checks | M | 1; retain regressions from 2–4 |
| 8 | P2 | Profile and reduce allocation/copying | M | Correctness baseline |
| 9 | P2 | Focused maintainability and documentation cleanup | M | Alongside affected batches |

P0 here means the library can reject ordinary valid bytecode or silently lose intended edits. It does not imply a confirmed remotely exploitable vulnerability.

## 1. Establish a reproducible toolchain and reliable checks

**Files:** `Cargo.toml`, `pyproject.toml`, both lockfiles, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `tools/generate_api_docs.py`, `crates/pytecode-engine/src/fixtures.rs`.

- Add a pinned `rust-toolchain.toml` with rustfmt and Clippy. Use Rust **1.98.1** as the initial candidate; it is already installed locally. Keep `rust-version = "1.94"` only if a dedicated minimum-supported-Rust check passes; otherwise raise the minimum deliberately. Development compiler and minimum supported compiler serve different purposes. Rust 1.98.1 fixes a code-generation bug in 1.98.0. [Rust release announcement](https://blog.rust-lang.org/2026/09/03/Rust-1.98.1/)
- Use Python **3.14.7** for development and preserve the existing **3.12+** public compatibility baseline initially. Add Python 3.12/3.13/3.14 test coverage. Python 3.15 is still a prerelease; consider a separate informational job rather than making it a release requirement. [Python releases](https://www.python.org/downloads/)
- Provision JDK **25** for the existing full suite, then add **26** compatibility work in batch 6. Installing a newer JDK alone does not extend the parser's accepted classfile versions. Oracle lists 26 as the latest platform release and 25 as the latest LTS. [Java downloads](https://www.oracle.com/java/technologies/downloads/)
- Record the uv version and use `uv sync --locked --extra dev` and `cargo ... --locked` in validation. Rebuild the extension from the reviewed source. Record versions and extension path in test/benchmark logs.
- Fix the new Clippy finding and the docs checker's Unicode-only status markers. Validate the latter with a cp1252 output stream as well as UTF-8.
- Add Windows and macOS execution coverage, at least a representative Python/JDK combination on each, plus the broader Python matrix on Linux. CI currently executes all checks only on Ubuntu/Python 3.12; release builds for other systems do not substitute for running tests there.
- Investigate the observed fixture-cache failure. It disappeared in the serial rerun; a race is a plausible explanation, not yet a confirmed cause. `ensure_compiled_source_cached()` checks and removes a shared cache entry before publishing, without per-entry synchronization. Test concurrent callers and compiler changes. Use immutable keys including source/JDK identity or locking as appropriate; do not merely hide the issue by serializing the whole suite.

**Done when:** a clean checkout can install and run the documented checks; source-built extension identity is clear; baseline failures are fixed or individually recorded; supported native platforms have executing tests.

## 2. Repair frame computation and validation

**Files:** `crates/pytecode-engine/src/analysis/mod.rs`, `analysis/hierarchy.rs`, `analysis/verify.rs`, `model/mod.rs`; corresponding Rust tests and `tests/test_rust_bindings.py`.

**Reproduced:** `ineg`, `lneg`, `fneg`, and `dneg` are grouped with binary arithmetic and pop two operands. Valid minimal methods fail recomputation with stack underflow. `lshl` pops four slots; its long-plus-int input has three. The same source grouping affects `lshr` and `lushr`.

**Reproduced:** `lconst_0; dup; pop; pop2; return` is accepted by recomputation and produces no model-verifier diagnostics, although `dup` cannot duplicate a category-2 value. The internal stack stores long/double as a value plus `Top`; fixed-slot shuffling can represent legal `dup*` forms, but it does not enforce operand categories. [JVM instruction definitions](https://docs.oracle.com/javase/specs/jvms/se26/html/jvms-6.html)

Additional concrete source concerns to cover in the same workstream:

- Primitive loads synthesize their type without checking the local or extending the required local count. `set_local()` does not invalidate a category-2 value when its second slot is overwritten.
- `simulate_method()` ignores `_class_name` and initializes `UninitializedThis` to the called constructor's owner. A `super()` call must leave `this` typed as the current class. Add constructors with subsequent branches and subclass field/method access.
- `common_superclass()` reduces unequal arrays immediately to `java/lang/Object`; `merge_vtypes()` silently substitutes Object when resolution fails. Test reference-array joins followed by array operations and define the missing-class policy.
- `simulate()` casts maximum stack/local sizes to `u16` without checked conversion.
- `verify_classfile_inner(..., _fail_fast)` ignores the argument. A class with two errors returns both for `fail_fast=True`; the model verifier has a separate collector that does honor early termination.
- A raw code body installed using `MethodModel.set_raw_code()` can leave local instructions as raw items which simulation does not support. Test this API before and after emit/reparse, and normalize or reject unsupported representation states consistently.

**Work:** fix negation and long shifts first; add a table-driven matrix for opcode stack effects and all legal/illegal category forms. Then enforce local/stack types, constructor initialization, array merges, bounds, and consistent fail-fast behavior. Preserve the distinction between structural diagnostics and JVM type verification in the public API/docs.

**Done when:** valid regressions recompute and load with the JVM verifier; invalid cases produce contextual diagnostics rather than accepted invalid frames, panics, or unrelated underflow errors. Cover both native entry points and Python APIs.

## 3. Make archive state and rewrite integrity dependable

**Files:** `pytecode/archive.py`, `crates/pytecode-python/src/archive.rs`, `crates/pytecode-archive/src/lib.rs`, archive tests.

**Reproduced:** assigning `jar.files['a.txt'].bytes = b'new'` then calling `rewrite()` emits the old bytes. The public mutable `files` map and private `_entry_states` have diverged: rewrite serializes only `_entry_states`. Metadata changes through `zipinfo` have the same structural problem. Rust exposes mutable `entries`, while raw-copy eligibility trusts `original_index` without checking whether content/metadata changed.

**Reproduced:** the archive-level ZIP comment is lost even in a no-transform rewrite. Entry metadata reading sets the creating system to Unknown/255; preservation is incomplete on paths that rebuild entries. Python dictionaries also collapse duplicate normalized names, while Rust holds a vector. Establish explicit preservation/rejection semantics for duplicate names and normalization collisions.

**Reproduced:** an intentional callback exception leaves a `*.tmp` archive behind. Rewrite uses a timestamp-derived name plus `File::create`, without an owned temporary-file cleanup guard. It renames the destination before rereading it; a reread failure restores only in-memory state after disk has changed.

**Work:**

- Establish one authoritative mutable archive state, or immutable public snapshots with explicit replacement methods. Prefer preserving the documented mutable surface if feasible. Track dirtiness across data and metadata; raw-copy only proven-unchanged entries.
- Create unique temporary files exclusively in the destination directory with automatic cleanup. Finish and validate the staged archive before replacement, close the input before replacing it where necessary, and use a cross-platform replacement primitive with tested existing-destination behavior.
- Ensure callback, compression, write, validation, and replacement failures preserve the original/destination bytes and leave no temporary artifacts. Specify durability separately from atomic visibility; add sync operations only for the chosen guarantee.
- Preserve archive comments and supported entry metadata. Define behavior for fields the ZIP backend cannot roundtrip. Keep the existing signed-JAR policy explicit; signature stripping/re-signing should be a separate intentional feature.
- Add read limits for entry count, individual expanded size, and total expanded bytes. `read_archive_entries()` currently preallocates from each declared size and reads every entry without a budget. Use checked sizes, bounded reads, and useful entry-specific errors.

**Done when:** direct edits survive, no-op rewrites honor documented metadata semantics, duplicate-name behavior is deterministic, and fault-injection tests establish disk/state rollback on Linux, Windows, and macOS. Resource-limit tests should be small and bounded.

## 4. Preserve valid classfiles and reject malformed output

**Files:** `modified_utf8.rs`, `descriptors.rs`, `signatures.rs`, `reader.rs`, `writer.rs`, `raw/instructions.rs`, model lowering and associated tests.

**Reproduced:** javac successfully compiles `return "\uD800";`. The raw reader accepts the result, but `ClassModel.from_bytes()` rejects it because `String::from_utf16()` rejects an unpaired surrogate. Java string content cannot always be represented losslessly by Rust `String`. Design a representation that preserves UTF-16 code units/raw modified-UTF-8, with explicit conversion at Python boundaries; do not replace unmatched surrogates with U+FFFD. Test raw roundtrip, lift/lower, LDC strings, annotations, and Python conversion.

**Reproduced:** fields with descriptor `Lbad[name;` and an array descriptor containing 256 `[` characters are emitted and receive no raw-verifier diagnostics. `validate_internal_name()` in descriptors checks `.` and empty segments but not `[`, unlike the verifier's separate name helper. Array parsing is recursively unbounded. Add context-aware descriptor checks including array dimension and method parameter slot limits, retaining valid Unicode names. [Classfile descriptors and constraints](https://docs.oracle.com/javase/specs/jvms/se26/html/jvms-4.html)

**Source findings requiring boundary regressions:**

- The raw writer checks top-level pool/field/method counts but uses unchecked `as u16`/`as u8` casts for UTF-8 payload lengths, attributes, stack maps, annotations, and other nested counts. Reject overflow before emission using shared checked length helpers. Cover encoded-byte length, not just string character count.
- The raw reader returns after the last attribute without checking trailing input. Decide whether the public whole-class reader is strict and, if so, require full consumption; keep any streaming reader a separate explicit API.
- Opcode `0xCA` (breakpoint) is accepted as a simple classfile instruction, although it is reserved for JVM-internal use. Also audit tableswitch's accepted zero-length range, lookup key ordering, branch targets, and constant-pool operand kinds. Preserve a permissive inspection mode only if deliberately specified. [Reserved opcodes](https://docs.oracle.com/javase/specs/jvms/se26/html/jvms-6.html#jvms-6.2)
- Recursion in signatures/annotation payloads should have a depth budget or an iterative implementation. Validate declared lengths against remaining bytes before large allocations. Add fuzz regressions instead of attempting enormous allocations in the ordinary suite.

**Done when:** valid javac edge cases roundtrip through both representations; oversized models fail before producing corrupt bytes; malformed inputs fail predictably with useful context and bounded resource use.

## 5. Make correctness tests independent of the implementation

**Files:** Rust engine tests/fixtures, `tests/resources/VerifierHarness.java`, `tests/resources/oracle/RecordingAnalyzer.java`, `tests/javap_parser.py`, CI.

The suite has useful fixture roundtrips and structural assertions. Those can preserve the same mistake through both parser and writer. The repository contains verifier/oracle infrastructure, but the reviewed Rust/Python tests do not invoke `VerifierHarness` or `java -Xverify:all` as a systematic emitted-code acceptance gate.

- Activate a JVM verification harness for classes after transform and frame recomputation. Ensure classes/methods are actually resolved or exercised so lazy verification is not mistaken for acceptance.
- Start with the failures in batches 2 and 4, then constructors, exception handlers, wide locals, category-2 operations, array joins, switches, conditional-branch widening, and legacy subroutines.
- Test multi-release JAR entries independently for raw roundtrip; define a target-runtime selection policy before constructing a hierarchy from overlapping class names.
- Add property tests for descriptor/signature parse–emit, instruction lift–lower, and legal bounded models. Add cargo-fuzz targets for class parsing, model lifting, and bounded verification; seed them with minimized fixtures and run longer jobs on a schedule.
- Consider ASM or the JDK Class-File API as a second structural/frame oracle where useful. Oracle agreement complements the specification and runtime tests; it does not replace them.

**Done when:** a deterministic CI subset detects the bugs found in this review, fuzz failures become small permanent regressions, and reports distinguish raw roundtrip, successful frame computation, and JVM acceptance.

## 6. Update Java support and its policy

`MAX_SUPPORTED_CLASS_MAJOR` is 69, so current code rejects Java 26 classfiles (major 70). Add support using the Java SE 26 specification, not just by incrementing the constant. Audit format changes and preview semantics, add JDK 26 compilation/roundtrip/verification cases, and retain older fixtures. [JVMS 26 classfile format](https://docs.oracle.com/javase/specs/jvms/se26/html/jvms-4.html)

Replace the hardcoded `class_version_supported_by_java_se_25` naming with an explicit tooling support policy. The existing audit document claims historical preview minors are rejected, but current code accepts minor 65535 for every major 56–69. Decide whether the library parses historical preview classfiles for inspection separately from validating executability on a selected JVM. Update tests and docs to match that decision.

**Done when:** Java 26 GA fixtures are supported, future unsupported versions have clear diagnostics, preview rules have a documented target, and the support matrix agrees with the implementation.

## 7. Update dependencies and validate shipped artifacts

Versions below were queried directly from PyPI and crates.io on 2026-09-04. They are upgrade candidates, not a claim that the upgrades have passed this repository's checks. Recheck at implementation time; consult release notes and advisories before resolving lockfiles.

| Dependency | Locked/current requirement | Latest stable observed | Primary source |
| --- | --- | --- | --- |
| basedpyright | 1.39.0 | 1.39.10 | [PyPI](https://pypi.org/pypi/basedpyright/json) |
| maturin | 1.13.1 | 1.15.0 | [PyPI](https://pypi.org/pypi/maturin/json) |
| pdoc | 16.0.0 | 16.0.0 | [PyPI](https://pypi.org/pypi/pdoc/json) |
| pytest | 9.0.3 | 9.1.1 | [PyPI](https://pypi.org/pypi/pytest/json) |
| Ruff | 0.15.10, capped below 0.16 | 0.16.6 | [PyPI](https://pypi.org/pypi/ruff/json) |
| uv | minimum 0.10.12; not locked | 0.12.10 | [PyPI](https://pypi.org/pypi/uv/json) |
| PyO3 | 0.28.3 | 0.29.2 | [crates.io](https://crates.io/api/v1/crates/pyo3) |
| zip | 8.5.1 | 8.6.0 | [crates.io](https://crates.io/api/v1/crates/zip) |
| bitflags | 2.11.0 | 2.13.1 | [crates.io](https://crates.io/api/v1/crates/bitflags) |
| clap | 4.6.0 | 4.6.6 | [crates.io](https://crates.io/api/v1/crates/clap) |
| regex | 1.12.3 | 1.13.1 | [crates.io](https://crates.io/api/v1/crates/regex) |
| serde | 1.0.228 | 1.0.229 | [crates.io](https://crates.io/api/v1/crates/serde) |
| serde_json | 1.0.149 | 1.0.151 | [crates.io](https://crates.io/api/v1/crates/serde_json) |
| thiserror | 2.0.18 | 2.0.20 | [crates.io](https://crates.io/api/v1/crates/thiserror) |
| rustc-hash | 2.1.2 | 2.1.3 | [crates.io](https://crates.io/api/v1/crates/rustc-hash) |
| walkdir | 2.5.0 | 2.5.0 | [crates.io](https://crates.io/api/v1/crates/walkdir) |
| criterion | 0.8.2 | 0.8.2 | [crates.io](https://crates.io/api/v1/crates/criterion) |

- Refresh Python tools, ordinary Rust updates, PyO3, and ZIP in separate batches. Ruff's upper bound and PyO3's pre-1.0 compatibility range require manifest changes. Follow the [PyO3 migration guide](https://pyo3.rs/v0.29.2/migration), with particular attention to conversion and class/thread behavior.
- Add the Cargo ecosystem to Dependabot; only Actions and uv are configured today. Add a scheduled Rust advisory check and review transitive lockfile updates. No complete advisory audit was performed in this review.
- Make wheel builds use the intended Rust/maturin versions and locked Rust resolution. `maturin-action` build-tool selection is separate from the Python dev dependency pin.
- Install each native wheel into a fresh environment outside the checkout and run import, parse, edit, recompute, and archive smoke tests. Cover the advertised abi3 Python versions; explicitly state the status of cross-built architectures that cannot run natively in CI.
- Build a wheel from the generated sdist in isolation. Verify it contains required Rust/Python sources, stubs, `py.typed`, and licenses, and does not accidentally ship unrelated workspace artifacts. The local top-level sample JARs are not a suitable source-distribution dependency.
- Review the release tag-validation command's implicit `uv run` project build in wheel jobs. A lightweight metadata validation step should not accidentally require a host editable build before the configured wheel builder runs.

**Done when:** lockfiles reproduce the tested dependency graph, artifact tests pass before publishing, and supported platform/Python claims are backed by installed-package execution.

## 8. Improve performance from measurements

These are source-based candidates; no new performance measurements were collected during this review. Use the existing Criterion suites and Python comparison tools with a **release-built** extension, fixed corpus hashes, warmups, multiple samples, toolchain provenance, and peak memory measurements.

| Candidate | Evidence | Experiment |
| --- | --- | --- |
| Frame allocations | `FrameState::push/pop/set_local` clone stack and locals repeatedly; propagation clones candidate states | Mutate one working frame per instruction/block; clone only at graph joins, preserving exact semantics |
| Queue scans | `propagate()` uses `worklist.contains()` | Use an indexed queued-membership vector and compare branch-heavy methods |
| Repeated descriptor parsing | Simulation reparses field/method descriptors on visits | Cache parsed descriptor/type effects per instruction for one analysis run |
| Attribute writer allocations | Every `ByteWriter::new()` reserves 32 KiB, including small nested attributes | Reuse buffers or backpatch lengths; measure allocation count and tiny/large-class throughput |
| Archive memory copies | Eager decompression, Python bytes/state duplication, `parse_classes()` cloning, and rewrite rereads | Borrow/share unchanged data and avoid redundant refreshes; add lazy/streaming APIs only if measurements justify the public complexity |
| Python concurrency | Bindings use unsendable `Rc<RefCell<...>>` views and no `detach()` calls were found | First document thread ownership; benchmark detached work only after moving an owned Rust snapshot across the boundary safely |

Keep parse/write, recomputation, no-op rewrite, native transforms, and Python callbacks as separate workloads. Accept optimizations only with stable improvements and unchanged correctness. Choose a regression threshold from observed measurement noise instead of imposing an arbitrary universal percentage.

## 9. Improve maintainability where it supports the fixes

- Split large modules along existing responsibilities after adding regression coverage: Python `model.rs` (~3,400 lines), bindings `lib.rs` (~2,400), attributes (~2,400), engine model (~2,300), and verifier (~2,200). Avoid public API churn solely to reduce file length.
- Consolidate duplicated name/descriptor validation and opcode facts so parser, verifier, simulator, and Python enums do not drift. Retain independent spec/JVM tests so shared tables do not make the tests tautological.
- Test `_rust.pyi` against runtime signatures and mutation behavior. Narrow the global `reportAttributeAccessIssue = false` suppression where feasible; the current strict type-check label does not validate those accesses.
- Consolidate duplicated Java fixture sources under `tests/resources` and engine fixtures, while keeping Python and Rust harnesses independent.
- Revise `roadmap.md` and `rust-jvms-25-audit.md`: their completion statements overstate what the current implementation/tests establish. Link this plan, distinguish verified capabilities from remaining work, and document raw versus symbolic string behavior, mutable views, thread ownership, signed archives, and resource limits.
- Fix PowerShell benchmark examples that use cmd.exe `^` line continuation. Keep commands executable in the named shell.

## Minimal reproductions to turn into regressions

Run from the repository root after installing a fresh extension. The following valid minimal methods currently raise underflow during frame recomputation:

```python
from pathlib import Path
from pytecode.archive import FrameComputationMode
from pytecode.model import ClassModel, MethodModel

base = Path("crates/pytecode-engine/fixtures/classes/HelloWorld/HelloWorld.class").read_bytes()
cases = [
    ("ineg", "()I", "04 74 ac"),
    ("lneg", "()J", "09 75 ad"),
    ("fneg", "()F", "0b 76 ae"),
    ("dneg", "()D", "0e 77 af"),
    ("lshl", "()J", "09 04 79 ad"),
]
for name, descriptor, code in cases:
    model = ClassModel.from_bytes(base)
    method = MethodModel(name, descriptor, 0x0009)  # public static
    method.set_raw_code(4, 0, bytes.fromhex(code))
    model.methods = [method]
    try:
        model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    except Exception as error:
        print(name, error)
```

For archive edits, create a temporary ZIP containing `a.txt = b'old'`, open it with `JarFile`, assign `jar.files['a.txt'].bytes = b'new'`, rewrite to a second temporary path, and read that entry with Python `zipfile`: the result was `b'old'`. Also set `ZipFile.comment` before opening it: the rewritten archive's comment was empty. Raise from a class transform callback and assert the destination is unchanged and no sibling `*.tmp` remains; the cleanup assertion currently fails.

For fail-fast, load the base class, set `access_flags = 0x0411` (public/final/abstract) and `super_name = None`, emit, and call `verify_classfile(..., fail_fast=True)`: both structural errors are returned. For modified UTF-8, compile a temporary Java class with a method returning `"\uD800"`; raw parsing succeeds and symbolic lifting fails.

## New-session starting instruction

> The implementation and concrete follow-up fixes are delivered. Read the delivery evidence and compatibility limits first, inspect the current diff and repository instructions, and establish a source-built baseline before making further changes. Treat the original review below the delivery record as historical evidence, not a fresh list of unfixed bugs. Choose further work from a reproducible failure or a measured workload and retain independent JVM verification for semantic changes.
