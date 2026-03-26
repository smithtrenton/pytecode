# pytecode

`pytecode` is a Python 3.14+ library for parsing, inspecting, and beginning to manipulate JVM class files, bytecode, and JAR files.

See `ARCHITECTURE.md` for a summary of the current parser + Phase 1 editing model and the roadmap toward a full classfile manipulation library.

## Current public API

- `pytecode.ClassReader` parses `.class` bytes eagerly into an `info.ClassFile` tree.
- `pytecode.JarFile` reads JARs, separates `.class` entries from non-class resources, and parses classes via `ClassReader`.
- `pytecode.ClassModel` provides the current Phase 1 mutable editing model with symbolic class, field, and method references.

## Requirements

- Python `3.14+`
- `uv` for development workflows

This repository keeps broad dependency ranges in `pyproject.toml` and commits `uv.lock` for reproducible development environments.

## Getting started

Create and sync a local environment with the development tools:

```powershell
uv sync --extra dev
```

Run the sample parser against the checked-in JAR:

```powershell
uv run python .\run.py .\225.jar
```

The script writes extracted output under `.\output\225\`.

## Development commands

Lint and check import ordering:

```powershell
uv run ruff check .
```

Verify formatting:

```powershell
uv run ruff format --check .
```

Run type checking:

```powershell
uv run basedpyright
```

Run the validation suite:

```powershell
uv run pytest -q
```

## Script validation

`run.py` is a manual smoke-test helper for the checked-in `225.jar` sample. Running it writes extracted output under `output\225` for inspection or comparison.

`tools\parse_wiki_instructions.py` supports deterministic generation from a local HTML file you provide:

```powershell
uv run python .\tools\parse_wiki_instructions.py --input-html .\path\to\wiki_instructions.html --output .\instruction_dump.txt
```

If `--input-html` is omitted, the script fetches the Wikipedia source page and prints the generated mapping to stdout.
