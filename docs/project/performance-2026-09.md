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

## Follow-up: frame worklists and archive classification

The follow-up uses the same Windows/Rust release environment, 30 samples, one
second of warmup, and a three-second minimum measurement period. The archive
workload extended collection to about five seconds. Before/after runs did not
overlap builds. The pre-optimization algorithms and new frame benchmark are
preserved in `ec2d966`; the frame measurements preceded a module-only extraction.

Frame worklists now track queued nodes by index instead of scanning the queue.
Typed pops no longer allocate a vector of values that the caller discards, and
pushes move supplied values instead of cloning them again. Public `FrameState`
layout and immutable operations stay compatible.

| Workload | Before estimate (95% CI) | After estimate (95% CI) | Interpretation |
| --- | --- | --- | --- |
| Recompute 32-way switch | 34.136 us (33.943–34.313) | 31.515 us (31.238–31.762) | About 8% less time |
| Recompute 256-way switch | 252.95 us (248.89–256.08) | 238.62 us (236.91–240.22) | About 6% less time |
| Recompute 2,048-way switch | 3.0456 ms (3.0177–3.0721) | 2.2403 ms (2.2097–2.2635) | About 27% less time |
| Recompute javac fixtures | 589.62 us (584.10–595.96) | 586.37 us (575.50–597.73) | No clear change, p = 0.55 |
| Open archive and classify/count entries | 172.38 ms (170.33–174.27) | 154.21 ms (152.14–156.15) | About 11% less time |

The switch workload is synthetic, with one integer local and independent return
blocks; it specifically stresses a wide worklist. The javac workload analyzes all
code-bearing methods from these fixed classfiles (SHA-256):

- `InstructionShowcase.class`: `d3d6b24bf0b9fc01bf24a91103e0eee483dd9bbefab25751396124fe146ae444`
- `TryCatchExample.class`: `adaf454364427fa21dbd7b18b97a324411d11f4d945d535d1810535aceef5adc`
- `SwitchExpressions.class`: `fa051fd2095e9a509b74ec2f5d04bc3a66bc0e8a33c9d25ece3c2ac9d52e877d`

Archive measurements use the Byte Buddy corpus above. CLI reporting previously
called `parse_classes()` only to count entries, cloning every class payload twice
and every resource once. Counting borrowed entries removes 42,790,079 bytes of
payload copies per classification on this corpus: 5,928 classes contain 21,373,914
bytes and six resources contain 42,251 bytes. This is a calculation from actual
entry lengths, excluding metadata/allocation overhead; it is not a measured RSS
reduction. Benchmark input caching also avoids the discarded metadata/resource
copies. Existing owned `parse_classes()` results remain compatible.

To reproduce, run these on the baseline, then repeat on the optimized revision
with `--baseline followup-before` in place of `--save-baseline followup-before`:

```powershell
cargo bench -p pytecode-engine --bench frames --locked -- --warm-up-time 1 --measurement-time 3 --sample-size 30 --save-baseline followup-before
cargo bench -p pytecode-archive --bench jar_read --locked -- --warm-up-time 1 --measurement-time 3 --sample-size 30 --save-baseline followup-before
```

These results support the internal changes above. They do not establish an
end-to-end transform speedup, a descriptor-cache benefit, or a need for a streaming
or detached-GIL API. Those ownership changes would require separate workloads and
compatibility decisions; they are not necessary to obtain these improvements.

The existing `tools/bench_transform_pipeline.py` was also run on the source-built
release extension as an exploratory five-run comparison. Apply-only medians were
1.3 ms for the native pipeline, 3.3 ms for mixed callbacks, 14.0 ms for live-view
callbacks, and 16.9 ms for the Python callback pipeline. Loading models took
527–595 ms per pass. These small samples have no pre-change baseline and are not a
thread-scaling measurement. They favor using the existing native pipeline for
supported transforms before adding a new concurrency/ownership API.
