# Benchmarks

This document explains the benchmark reports used in `pytecode` and how to
reproduce them locally.

## What the benchmark reports measure

The repository includes two complementary benchmark styles:

1. **Native Rust workflows** through `pytecode-cli`
2. **Python API workflows** through the packaged `pytecode` module

Both are useful:

- the CLI numbers show the cost of the engine and archive crates directly
- the Python numbers show the cost of the public package, including model
  materialization and Python callback execution where those are part of the
  workflow

## What to expect

- Parsing, writing, validation, and archive rewriting are driven by Rust-backed
  implementations.
- The largest Python-side overhead appears when a workflow materializes large
  numbers of Python-visible objects or executes user-defined Python callbacks.
- Comparisons are only meaningful when the Python extension is built in
  **release** mode.

## Recommended benchmark commands

```powershell
# Native Rust CLI timing
cargo run --release -p pytecode-cli -- bench-smoke --iterations 5

# Python API timing on the same jar
uv run python tools\benchmark_jar_pipeline.py ^
  crates\pytecode-engine\fixtures\jars\byte-buddy-1.17.5.jar ^
  --iterations 5

# Side-by-side comparison output
uv run python tools\compare_rust_python_benchmarks.py ^
  --jar crates\pytecode-engine\fixtures\jars\byte-buddy-1.17.5.jar ^
  --iterations 5 ^
  --output output\benchmarks\rust-vs-python-byte-buddy.json
```

## Benchmark interpretation notes

- Use the same corpus for both Rust and Python runs.
- Rebuild the extension in release mode before drawing conclusions about Python
  overhead.
- Treat callback-heavy transform workloads separately from purely declarative
  transform workloads, because they exercise different cost centers.
