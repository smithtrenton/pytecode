from __future__ import annotations

import argparse
import time
from collections.abc import Sequence
from pathlib import Path
from pprint import pprint

import pytecode
from pytecode.jar import JarInfo


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Parse a JAR file and write class and resource output alongside it.")
    parser.add_argument("jar", type=Path, help="Path to the JAR file to parse.")
    return parser.parse_args(argv)


def write_outputs(
    output_dir: Path,
    classes: list[tuple[JarInfo, pytecode.ClassReader]],
    other_files: list[JarInfo],
) -> None:
    for jar_info, class_reader in classes:
        output_path = output_dir / f"{jar_info.filename}.output"
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with output_path.open("w", encoding="utf-8") as handle:
            pprint(class_reader.class_info, handle)

    for other_file in other_files:
        output_path = output_dir / other_file.filename
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with output_path.open("wb") as handle:
            handle.write(other_file.bytes)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    jar_path = args.jar.expanduser().resolve()
    output_dir = jar_path.parent / "output" / jar_path.stem

    start = time.time()
    jar = pytecode.JarFile(str(jar_path))
    end = time.time()
    print(f"Read time: {end - start}s")

    start = time.time()
    classes, other_files = jar.parse_classes()
    end = time.time()
    print(f"Parse time: {end - start}s")
    print(f"\tclasses: {len(classes)}")
    print(f"\tother_files: {len(other_files)}")

    start = time.time()
    write_outputs(output_dir, classes, other_files)
    end = time.time()
    print(f"Write time: {end - start}s")
    print(f"\tdir: {output_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
