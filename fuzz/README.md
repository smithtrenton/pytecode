# Bounded fuzzing

Use the pinned nightly and cargo-fuzz versions from `.github/workflows/fuzz.yml`.
Install cargo-fuzz with `cargo install cargo-fuzz --version 0.13.2 --locked`, then:

```text
python tools/seed_fuzz_corpus.py
cargo +nightly-2026-09-03 fuzz run class_parse -- -max_total_time=30 -timeout=10 -rss_limit_mb=2048 -max_len=262144
```

Other targets are `model_lift` and `verify_bounded`. The verifier target caps input
bytes, methods, and constant-pool entries. It also recomputes frames when the model
has at most 512 instruction/label items and 32 handlers, referenced local slots are
below 256, and constant-pool UTF-8 entries are at most 512 bytes. These extra caps
bound the frame-analysis workload while exercising constructor and join errors.
Targets exercise parser/writer/model
error handling; JVM acceptance is covered by separate runtime tests. Seeds come
from checked-in class fixtures; generated corpora and artifacts are ignored.

The runner does not accept `--locked`. CI checks the fuzz lockfile with locked Cargo
metadata before execution and checks for lockfile changes afterward. Both workspace
and fuzz lockfiles must be refreshed deliberately when dependencies change.

Minimize any failure with `cargo fuzz tmin`, add a small permanent Rust/Python
regression, and keep its input in a tracked regression fixture. The weekly lane
runs each target for ten minutes; change-time runs last thirty seconds.

Local Windows/MSVC AddressSanitizer smoke run, 2026-09-04:

| Target | Executions | Time | Result |
| --- | ---: | ---: | --- |
| class_parse | 130,601 | 16 s | Passed |
| model_lift | 38,592 | 16 s | Passed |
| verify_bounded | 49,335 | 16 s | Passed |

Compiler: `1.100.0-nightly (2e2b193f8 2026-09-02)`; cargo-fuzz 0.13.2;
libfuzzer-sys 0.4.13. These short runs establish that the harnesses execute; they
do not establish absence of malformed-input bugs.

Post-optimization rerun on the same toolchain (10-second configured budgets,
11 seconds reported by libFuzzer): `class_parse` 107,971 executions, `model_lift`
41,317 executions, and `verify_bounded` 48,657 executions. All passed. The local
corpora include discoveries from the earlier runs, so execution rates are not
direct performance comparisons.

The follow-up verifier target now also exercises bounded frame recomputation. Its
local ASan run completed 18,129 executions in 11 seconds with no failure (463 MB
reported RSS), using the same nightly and the accumulated local corpus. This is
a smoke test of the expanded target, not a comparison with the previous target's
execution rate or an exhaustive verification result.
