# Baseline Performance Metrics

Captured on `rust-experiment` branch before any Rust code is wired into the Python pipeline.

## Environment

- **OS**: Windows (x86_64)
- **Python**: CPython 3.14
- **Rust**: 1.94.0
- **Test JAR**: `225.jar` (1,228 classes, 17,906 methods)

## 225.jar Full Pipeline

| Stage | Elapsed (s) | Description |
|-------|------------|-------------|
| 1. jar-read | 0.085 | Read ZIP metadata and entry bytes |
| 2. jar-classify | 0.001 | Classify entries (class vs resource) |
| 3. class-parse | 5.862 | Parse `.class` bytes → `ClassFile` |
| 4. model-lift | 8.815 | Lift `ClassFile` → `ClassModel` |
| 5. model-lower | 7.922 | Lower `ClassModel` → `ClassFile` |
| 6. class-write | 5.784 | Serialize `ClassFile` → bytes |
| **Total** | **28.469** | |

## Key Observations

- **class-parse** (5.9s) and **class-write** (5.8s) are the binary I/O hot paths → Phase 1 & 3 targets.
- **model-lift** (8.8s) is the largest single stage → Phase 4 target (ConstantPoolBuilder, label lifting).
- **model-lower** (7.9s) is the second-largest → Phase 4 target (label resolution, code lowering).
- **jar-read** is negligible (ZIP I/O via Python `zipfile`).

## Tests

- 1,670 tests pass in ~10s.

## Notes for Current Branch State

- This baseline was captured before the Rust-backed wrappers were wired into the Python pipeline.
- The current branch has 1,688 passing pytest tests; the difference comes from tests added during the Rust experiment.
- Compare this file with [Phase 1 benchmarks](phase-1-benchmarks.md) for the current sampled corpus results after the persistent-wrapper redesign.
