from __future__ import annotations

import argparse
import time
from collections.abc import Sequence
from pathlib import Path
from pprint import pprint

from pytecode import ClassModel, ClassReader, JarFile
from pytecode.jar import JarInfo


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Parse a JAR file and write class and resource output alongside it.")
    parser.add_argument("jar", type=Path, help="Path to the JAR file to parse.")
    return parser.parse_args(argv)


def write_classfiles(
    output_dir: Path,
    classes: list[tuple[JarInfo, ClassReader]],
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


def lift_models(classes: Sequence[tuple[JarInfo, ClassReader]]) -> list[tuple[JarInfo, ClassModel]]:
    return [(jar_info, ClassModel.from_classfile(class_reader.class_info)) for jar_info, class_reader in classes]


def write_classmodels(
    output_dir: Path,
    class_models: Sequence[tuple[JarInfo, ClassModel]],
    other_files: Sequence[JarInfo],
) -> None:
    for jar_info, class_model in class_models:
        output_path = output_dir / jar_info.filename
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with output_path.open("wb") as handle:
            handle.write(class_model.to_bytes())

    for other_file in other_files:
        output_path = output_dir / other_file.filename
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with output_path.open("wb") as handle:
            handle.write(other_file.bytes)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    jar_path = args.jar.expanduser().resolve()
    output_dir = jar_path.parent / "output" / jar_path.stem
    parsed_output_dir = output_dir / "parsed"
    rewritten_output_dir = output_dir / "rewritten"

    start = time.time()
    jar = JarFile(jar_path)
    end = time.time()
    print(f"Read time: {end - start}s")

    start = time.time()
    classes, other_files = jar.parse_classes()
    end = time.time()
    print(f"Parse time: {end - start}s")
    print(f"\tclasses: {len(classes)}")
    print(f"\tother_files: {len(other_files)}")

    start = time.time()
    class_models = lift_models(classes)
    end = time.time()
    print(f"Lift time: {end - start}s")
    print(f"\tclass_models: {len(class_models)}")

    start = time.time()
    write_classfiles(parsed_output_dir, classes, other_files)
    end = time.time()
    print(f"Write time: {end - start}s")
    print(f"\tdir: {parsed_output_dir}")

    start = time.time()
    write_classmodels(rewritten_output_dir, class_models, other_files)
    end = time.time()
    print(f"Rewrite time: {end - start}s")
    print(f"\tdir: {rewritten_output_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
