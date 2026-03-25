# pytecode

`pytecode` is a Python 3.14 library for parsing and analyzing JVM bytecode and JAR files.

See `ARCHITECTURE.md` for a summary of the current parser-focused design and the roadmap toward a full classfile manipulation library.

## Requirements

- Python `3.14`
- `uv` for development workflows

This repository intentionally keeps broad dependency ranges in `pyproject.toml` and does not commit a lockfile.

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
uv run python -m unittest discover -s tests -p "test_*.py" -v
```

## Script validation

`run.py` is regression-tested against the checked-in `225.jar` sample and the expected `output\225` tree.

`tools\parse_wiki_instructions.py` now supports deterministic validation from a local HTML fixture:

```powershell
uv run python .\tools\parse_wiki_instructions.py --input-html .\tests\fixtures\wiki_instructions_sample.html
```

If `--input-html` is omitted, the script fetches the Wikipedia source page and prints the generated mapping to stdout.
