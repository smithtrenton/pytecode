# Writer performance, September 2026

The retained optimization writes nested attributes and code into the final output
buffer and fills their checked length fields afterward. It removes temporary
32 KiB buffers and payload copies. Error paths still discard the unpublished
output; classfile length and nesting checks remain in place.

## Corpus and method

Windows x86-64, Rust 1.98.1, release profile with thin LTO and one codegen unit.
Corpus: the checked-in `byte-buddy-1.17.5.jar`, 5,928 class entries, 21,373,914
raw class bytes. SHA-256:
`71568c9f8396677219f650268fbf6493ded484edcdbdf2dae6129ca5be81e8db`.

Criterion used one second of warmup, a three-second minimum measurement time, and
30 samples per workload. The slower roundtrip workload extended collection to
roughly 18 seconds. A preliminary run overlapped compilation and was discarded;
the comparison below used runs without concurrent builds. These are local corpus
results, not universal throughput or an automated CI performance threshold.

| Workload | Before estimate (95% CI) | After estimate (95% CI) | Interpretation |
| --- | --- | --- | --- |
| Write all lowered classes | 42.214 ms (41.584–43.084) | 21.338 ms (20.821–22.049) | About 49% less time; significant in this run |
| Parse, lift, lower, write | 594.40 ms (583.66–606.93) | 577.00 ms (565.16–590.51) | No statistically clear change at the 5% level |

A separate allocator-instrumented executable measured raw writes. Its counters
include reallocations and requested capacity, not physical memory committed by the
OS. Instrumentation was absent from Criterion timings.

| Allocation metric, one corpus pass | Before | After |
| --- | ---: | ---: |
| Allocation/reallocation calls | 202,791 | 42,741 |
| Requested bytes, including reallocations | 5,308,260,864 | 63,742,464 |
| Peak additional live allocated bytes above loaded corpus | 229,376 | 131,072 |
| Output bytes | 21,373,914 | 21,373,914 |

Peak live allocation excludes allocator overhead and is not process RSS. Requested
bytes are cumulative allocation traffic, not a claim that the process held 5.3 GB
at once. Existing exact roundtrip and malformed-emission tests validate output
behavior independently of these counters.

## Reproduce

Run the first command on the pre-optimization source, then the second on the changed
source, with other builds stopped. Compare samples and confidence intervals before
accepting a performance claim.

```powershell
cargo bench -p pytecode-engine --bench pipeline --locked -- 'class-write|full-roundtrip' --warm-up-time 1 --measurement-time 3 --sample-size 30 --save-baseline before
cargo bench -p pytecode-engine --bench pipeline --locked -- 'class-write|full-roundtrip' --warm-up-time 1 --measurement-time 3 --sample-size 30 --baseline before
cargo run --release --locked -p pytecode-engine --example writer_allocations -- crates/pytecode-engine/fixtures/jars/byte-buddy-1.17.5.jar
```

Frame cloning, queue membership, descriptor caching, archive copying, and detached
Python work remain profiling candidates. They were deliberately not changed without
workload-specific evidence; no concurrency or streaming API was added. Measure
recomputation, no-op archive rewriting, native transforms, and Python callbacks
separately before selecting the next optimization.
